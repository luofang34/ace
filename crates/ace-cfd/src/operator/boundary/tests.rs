use super::*;
use crate::{IdealGas, Mesh, SphereGrid};

fn operator(outflow: Outflow) -> FlowOperator {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let mesh = Mesh::sphere(SphereGrid {
        radius: 0.01,
        outer_radius: 0.02,
        radial_cells: 12,
        angular_cells: 12,
        stretching: 2.0,
        shock_shape_mach: None,
    })
    .unwrap();
    let inflow = Primitive {
        density: 1000.0 / (gas.gas_constant * 300.0),
        velocity: [0.0; 3],
        pressure: 1000.0,
    };
    FlowOperator::new(mesh, gas, inflow, 300.0)
        .unwrap()
        .with_outflow(outflow)
}

#[test]
fn one_sided_outlet_transmits_affine_conductive_energy_flux() {
    for mode in [
        Outflow::ZeroGradient,
        Outflow::OneSided,
        Outflow::CellFlux,
        Outflow::ZeroGradientCellFlux,
    ] {
        let op = operator(mode);
        let slope = 1.0e4;
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                op.gas.conservative(Primitive {
                    density: op.inflow.pressure
                        / (op.gas.gas_constant * (300.0 + slope * cell.center[0])),
                    ..op.inflow
                })
            })
            .collect();
        let mut workspace = op.workspace();
        op.evaluate(&state, true, &mut workspace).unwrap();
        for i in 2..10 {
            let face = &op.mesh.faces[op.mesh.cells[11 * 12 + i].faces[3]];
            let flux = op.face_flux(face, true, &workspace);
            let expected = if matches!(mode, Outflow::ZeroGradient | Outflow::ZeroGradientCellFlux)
            {
                0.0
            } else {
                -op.gas.conductivity * slope
            };
            assert!((flux[4] - expected).abs() < 1e-8, "{mode:?}: {flux:?}");
        }
    }
}

#[test]
fn outlet_choices_preserve_uniform_rest() {
    for mode in [
        Outflow::ZeroGradient,
        Outflow::OneSided,
        Outflow::CellFlux,
        Outflow::ZeroGradientCellFlux,
    ] {
        let op = operator(mode);
        let residual = op.residual(&op.initial_state(), true).unwrap();
        assert!(residual.iter().flatten().all(|value| value.abs() < 1e-5));
    }
}

#[test]
fn copied_outlet_convection_preserves_all_other_fluxes_and_viscous_terms() {
    let mut op = operator(Outflow::ZeroGradient);
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.02,
        radial_cells: 12,
        angular_cells: 12,
        stretching: 2.0,
        shock_shape_mach: Some(3.0),
    };
    op.mesh = Mesh::curved_sphere(grid).unwrap();
    let state: Vec<_> = op
        .mesh
        .cells
        .iter()
        .map(|c| {
            op.gas.conservative(Primitive {
                density: op.inflow.density * (1.0 + 10.0 * c.center[1]),
                velocity: [200.0 + 1000.0 * c.center[0], 1000.0 * c.center[1], 0.0],
                pressure: 1000.0 + 1000.0 * c.center[0],
            })
        })
        .collect();
    let before = op.face_diagnostics(&state).unwrap();
    let cells = op.cell_diagnostics(&state).unwrap();
    op.outflow = Outflow::ZeroGradientCellFlux;
    let after = op.face_diagnostics(&state).unwrap();
    let mut changed = false;
    for (a, b) in before.iter().zip(&after) {
        if !op.cell_flux_outflow(&op.mesh.faces[a.face]) {
            assert_eq!(a.total_flux, b.total_flux);
            continue;
        }
        let expected = cells[b.owner].primitive;
        assert_eq!(b.left.components(), expected.components());
        let convective_before = crate::flux::physical(op.gas, a.left, a.normal);
        let convective_after = crate::flux::physical(op.gas, b.left, b.normal);
        for k in 0..5 {
            let measured = b.total_flux[k] - a.total_flux[k];
            let expected = convective_after[k] - convective_before[k];
            assert!((measured - expected).abs() < 1e-9 * expected.abs().max(1.0));
        }
        changed |= a.total_flux != b.total_flux;
    }
    assert!(changed, "the field must exercise outlet reconstruction");
}

#[test]
fn wall_heat_flux_reproduces_a_quadratic_normal_temperature() {
    let op = operator(Outflow::ZeroGradient);
    let face = op
        .mesh
        .faces
        .iter()
        .find(|face| face.neighbor.is_none() && matches!(face.boundary, Boundary::Wall))
        .unwrap();
    let slope = 1.0e4;
    for curvature in [0.0, 1.0e8] {
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                let distance = -dot(sub(cell.center, face.center), face.normal);
                let temperature = 300.0 + slope * distance + curvature * distance.powi(2);
                op.gas.conservative(Primitive {
                    density: op.inflow.pressure / (op.gas.gas_constant * temperature),
                    ..op.inflow
                })
            })
            .collect();
        // Other wall faces do not impose this local polynomial. Its normal
        // derivative is checked only at the plane defining the field.
        let mut workspace = op.workspace();
        op.evaluate(&state, true, &mut workspace).unwrap();
        let heat = op.face_flux(face, true, &workspace)[4];
        let exact = op.gas.conductivity * slope;
        assert!((heat / exact - 1.0).abs() < 1.0e-11, "{heat}, {exact}");
    }
}

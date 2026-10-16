use super::*;
use crate::{Convection, IdealGas, Primitive, SphereGrid};

fn operator() -> FlowOperator {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.02,
        radial_cells: 32,
        angular_cells: 24,
        stretching: 1.0,
        shock_shape_mach: None,
    };
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let inflow = Primitive {
        density: 0.01,
        pressure: 3000.0,
        velocity: [0.0; 3],
    };
    FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0)
        .unwrap()
        .with_convection(Convection::Ausmdv)
}

fn weak_pressure_jump(jump: f64) -> (Vec<f64>, f64) {
    let op = operator();
    let (gas, inflow) = (op.gas, op.inflow);
    let state: Vec<_> = (0..op.mesh.cell_count())
        .map(|i| {
            let pressure = inflow.pressure * (1.0 + if i % 32 < 16 { jump } else { 0.0 });
            gas.conservative(Primitive { pressure, ..inflow })
        })
        .collect();
    let mut ws = op.workspace();
    op.prepare(&state, &mut ws).unwrap();
    assert!(ws.shock_sensor.iter().all(|s| *s == 0.0));
    let indicators = (0..state.len())
        .map(|i| {
            let stencil = &op.pressure_stencils[i];
            model_defect(
                stencil,
                &ws,
                i,
                coefficients(stencil, |n| ws.primitive[n].pressure, i),
            )
        })
        .collect();
    let mut overshoot = 0.0_f64;
    for face in &op.mesh.faces {
        for point in face.integration_points() {
            let point_face = face.at_point(point);
            for cell in std::iter::once(face.owner).chain(face.neighbor) {
                let p = op.face_state(&point_face, cell, true, &ws).pressure;
                assert!(p.is_finite() && p > 0.0);
                overshoot =
                    overshoot.max((inflow.pressure - p).max(p - inflow.pressure * (1.0 + jump)));
            }
        }
    }
    (indicators, overshoot / (inflow.pressure * jump))
}

#[test]
fn mean_prediction_detects_weak_jumps_without_pressure_overshoot() {
    let (small, small_overshoot) = weak_pressure_jump(1e-4);
    let (large, large_overshoot) = weak_pressure_jump(1e-2);
    assert!(small.iter().any(|s| *s > 0.5));
    let difference = small
        .iter()
        .zip(large)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        difference < 1e-7,
        "jump amplitude changed normalized indicators by {difference}"
    );
    assert!(
        small_overshoot < 1e-8 && large_overshoot < 1e-8,
        "relative pressure overshoot: small={small_overshoot}, large={large_overshoot}"
    );
}

#[test]
fn remote_jump_detection_does_not_flatten_an_affine_pressure_stencil() {
    let op = operator();
    let affine = |x: f64| 3000.0 + 10000.0 * x;
    let state: Vec<_> = op
        .mesh
        .cells
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let pressure = affine(cell.center[0]) + if i % 32 >= 16 { 100.0 } else { 0.0 };
            op.gas.conservative(Primitive {
                pressure,
                ..op.inflow
            })
        })
        .collect();
    let mut ws = op.workspace();
    op.prepare(&state, &mut ws).unwrap();
    let i = 8 * 32 + 12;
    let stencil = &op.pressure_stencils[i];
    assert!(stencil.neighbors.iter().all(|(n, _)| n % 32 < 16));
    assert_eq!(ws.shock_sensor[i], 0.0);
    let support = stencil
        .neighbors
        .iter()
        .map(|(n, _)| ws.pressure_defect[*n])
        .fold(ws.pressure_defect[i], f64::max);
    assert!(
        support > 0.1,
        "remote discontinuity must activate the indicator: {support}"
    );
    let error = op.mesh.cells[i]
        .faces
        .iter()
        .flat_map(|id| {
            let face = &op.mesh.faces[*id];
            face.integration_points().map(|point| {
                let p = op.face_state(&face.at_point(point), i, true, &ws).pressure;
                (p / affine(point.center[0]) - 1.0).abs()
            })
        })
        .fold(0.0_f64, f64::max);
    assert!(
        error < 1e-11,
        "affine pressure was attenuated by indicator {support}: error={error}"
    );
}

use super::*;
use crate::{IdealGas, Mesh, SphereGrid};

fn curved_operator() -> (FlowOperator, SphereGrid) {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.02,
        radial_cells: 24,
        angular_cells: 24,
        stretching: 2.0,
        shock_shape_mach: Some(3.0),
    };
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let inflow = Primitive {
        density: 0.01,
        velocity: [0.0; 3],
        pressure: 1000.0,
    };
    (
        FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0).unwrap(),
        grid,
    )
}

#[test]
fn limiter_preserves_affine_fields_in_cell_moment_coordinates() {
    let (op, grid) = curved_operator();
    for axial in [4000.0, 6500.0] {
        let pressure = |x: f64, yy: f64| 1000.0 + axial * x + 2e6 * yy;
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                let [x, yy] = axisymmetry::moment_coordinates(cell)[0];
                op.gas.conservative(Primitive {
                    pressure: pressure(x, yy),
                    ..op.inflow
                })
            })
            .collect();
        let mut workspace = op.workspace();
        op.prepare(&state, &mut workspace).unwrap();
        let mut error = 0.0_f64;
        let mut unlimited_error = 0.0_f64;
        for j in 0..3 {
            for i in 8..16 {
                let id = j * grid.radial_cells + i;
                let cell = &op.mesh.cells[id];
                for face in cell.faces {
                    for point in op.mesh.faces[face].integration_points() {
                        let actual = reconstruct(
                            workspace.primitive[id],
                            workspace.gradient[id],
                            cell,
                            point.center,
                        );
                        let exact = pressure(point.center[0], point.center[1] * point.center[1]);
                        error = error.max((actual.pressure / exact - 1.0).abs());
                        let mut gradient = workspace.gradient[id];
                        gradient.limited = std::array::from_fn(|k| gradient.regular[k]);
                        let unlimited =
                            reconstruct(workspace.primitive[id], gradient, cell, point.center);
                        unlimited_error =
                            unlimited_error.max((unlimited.pressure / exact - 1.0).abs());
                    }
                }
            }
        }
        assert!(
            unlimited_error < 1e-12,
            "moment gradient error={unlimited_error}"
        );
        assert!(
            error < 1e-12,
            "affine moment-coordinate pressure error={error}, axial={axial}"
        );
    }
}

#[test]
fn shock_flattening_preserves_mean_pressure_and_the_face_radius() {
    let (op, grid) = curved_operator();
    let rho = op.inflow.density;
    let strain = 1000.0;
    let state: Vec<_> = op
        .mesh
        .cells
        .iter()
        .map(|cell| {
            let [x, yy] = axisymmetry::moment_coordinates(cell)[0];
            let pressure = if x < -0.014 { 300.0 } else { 3000.0 };
            // These are exact conservative means of constant density and v_r=a*r.
            [
                rho,
                0.0,
                rho * strain * cell.center[1],
                0.0,
                pressure / (op.gas.gamma - 1.0) + 0.5 * rho * strain * strain * yy,
            ]
        })
        .collect();
    let mut workspace = op.workspace();
    op.prepare(&state, &mut workspace).unwrap();
    let mut selected = 0_usize;
    for j in 0..3 {
        for i in 3..grid.radial_cells - 3 {
            let id = j * grid.radial_cells + i;
            if workspace.shock_sensor[id] != 1.0 {
                continue;
            }
            selected = selected.wrapping_add(1);
            for face_id in op.mesh.cells[id].faces {
                let face = &op.mesh.faces[face_id];
                if face.neighbor.is_none() {
                    continue;
                }
                for point in face.integration_points() {
                    let q = op.face_state(&face.at_point(point), id, true, &workspace);
                    let pressure_error =
                        (q.pressure / workspace.primitive[id].pressure - 1.0).abs();
                    let radial_error = (q.velocity[1] / (strain * point.center[1]) - 1.0).abs();
                    assert!(
                        pressure_error < 1e-12 && radial_error < 1e-12,
                        "cell={id}, pressure={pressure_error}, radial velocity={radial_error}"
                    );
                }
            }
        }
    }
    assert!(
        selected > 3,
        "physical pressure jump must trigger the sensor"
    );
}

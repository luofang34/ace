use super::*;
use crate::{Mesh, SphereGrid};

fn operator() -> FlowOperator {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.02,
        radial_cells: 24,
        angular_cells: 24,
        stretching: 2.0,
        shock_shape_mach: None,
    };
    let inflow = Primitive {
        density: 0.01,
        velocity: [0.0; 3],
        pressure: 1000.0,
    };
    FlowOperator::new(Mesh::sphere(grid).unwrap(), gas, inflow, 300.0).unwrap()
}

#[test]
fn shock_flattening_preserves_planar_primitive_bounds() {
    let op = operator();
    for slope in [100.0, 200.0, 500.0] {
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                let [x, y, _] = cell.center;
                op.gas.conservative(Primitive {
                    pressure: 1000.0 * (slope * (x + y)).exp(),
                    velocity: [1000.0 * (x.hypot(y) - 0.01), 0.0, 0.0],
                    ..op.inflow
                })
            })
            .collect();
        let mut workspace = op.workspace();
        op.prepare(&state, &mut workspace).unwrap();
        let mut active = 0_usize;
        for (i, cell) in op.mesh.cells.iter().enumerate() {
            if workspace.shock_sensor[i] == 0.0 {
                continue;
            }
            active = active.wrapping_add(1);
            let own = components(op.gas, workspace.primitive[i]);
            let (mut lower, mut upper) = (own, own);
            for id in cell.faces {
                let face = &op.mesh.faces[id];
                if op.extrapolated_outflow(face) {
                    continue;
                }
                let (q, _) = op.sample(face, i, &workspace.primitive);
                let values = components(op.gas, q);
                for k in 0..5 {
                    lower[k] = lower[k].min(values[k]);
                    upper[k] = upper[k].max(values[k]);
                }
            }
            for id in cell.faces {
                let face = &op.mesh.faces[id];
                if face.neighbor.is_none() {
                    continue;
                }
                let q = op.face_state(face, i, true, &workspace);
                for (k, value) in q.components().into_iter().enumerate() {
                    let tolerance = 2e-12 * lower[k].abs().max(upper[k].abs()).max(1.0);
                    assert!(
                        value >= lower[k] - tolerance && value <= upper[k] + tolerance,
                        "cell={i}, component={k}, value={value}, bounds={:?}",
                        [lower[k], upper[k]]
                    );
                }
            }
        }
        assert!(active > 10, "pressure variation must exercise flattening");
    }
}

#[test]
fn planar_pressure_source_integrates_the_bounded_face_polynomial() {
    let op = operator();
    let state: Vec<_> = op
        .mesh
        .cells
        .iter()
        .map(|cell| {
            op.gas.conservative(Primitive {
                pressure: 300.0 + 2e5 * cell.center[1],
                velocity: [
                    1000.0 * (cell.center[0].hypot(cell.center[1]) - 0.01),
                    0.0,
                    0.0,
                ],
                ..op.inflow
            })
        })
        .collect();
    let mut workspace = op.workspace();
    op.prepare(&state, &mut workspace).unwrap();
    let mut active = 0_usize;
    for (i, cell) in op.mesh.cells.iter().enumerate() {
        if workspace.shock_sensor[i] == 0.0 {
            continue;
        }
        active = active.wrapping_add(1);
        let source = crate::operator::pressure::planar_mean(
            cell,
            workspace.primitive[i].pressure,
            workspace.pressure[i],
        );
        for id in cell.faces {
            let face = &op.mesh.faces[id];
            if face.neighbor.is_none() {
                continue;
            }
            let q = op.face_state(face, i, true, &workspace);
            // A linear face polynomial integrates to its value at the planar centroid.
            let mean = q.pressure
                + dot(
                    workspace.gradient[i].limited[4],
                    sub(cell.planar_center, face.center),
                );
            assert!(
                (source - mean).abs() < 2e-12 * source.abs(),
                "cell={i}, pressure source={source}, face polynomial mean={mean}"
            );
        }
    }
    assert!(active > 10, "pressure variation must exercise flattening");
}

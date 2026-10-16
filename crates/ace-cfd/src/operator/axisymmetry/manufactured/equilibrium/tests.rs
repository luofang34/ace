use crate::operator::Workspace;
use crate::{Convection, FlowOperator, Mesh, Primitive, SphereGrid};

fn operator(n: usize, aspect: usize, shaped: bool) -> (FlowOperator, SphereGrid) {
    let (base, mut grid) = super::super::super::tests::strain_operator(n, Convection::Ausmdv);
    grid.angular_cells = aspect * n;
    grid.shock_shape_mach = shaped.then_some(3.0);
    let op = FlowOperator::new(
        Mesh::curved_sphere(grid).unwrap(),
        base.gas,
        base.inflow,
        300.0,
    )
    .unwrap()
    .with_convection(Convection::Ausmdv);
    (op, grid)
}

fn term_magnitudes(op: &FlowOperator, ws: &Workspace, i: usize) -> [f64; 2] {
    let cell = &op.mesh.cells[i];
    let mut magnitude = [0.0; 2];
    for id in cell.faces {
        let face = &op.mesh.faces[id];
        for point in face.integration_points() {
            let flux = op.face_flux(&face.at_point(point), true, ws);
            for (sum, value) in magnitude.iter_mut().zip(&flux[1..3]) {
                *sum += value.abs() * point.area;
            }
        }
    }
    let pressure =
        crate::operator::pressure::planar_mean(cell, ws.primitive[i].pressure, ws.pressure[i]);
    let stress = crate::operator::axial::hoop_stress(op, ws, i);
    magnitude[1] += (pressure - stress).abs() * cell.planar_area;
    magnitude.map(|v| v / cell.volume)
}

fn affine_balance(n: usize, aspect: usize) -> ([f64; 2], [f64; 2]) {
    let (op, grid) = operator(n, aspect, false);
    let (density, strain) = (0.01, 10000.0);
    let state: Vec<_> = (0..n * grid.angular_cells)
        .map(|k| {
            super::super::cell_mean(grid, k % n, k / n, |x, y| {
                let q = Primitive {
                    density,
                    velocity: [-2.0 * strain * x, strain * y, 0.0],
                    pressure: 10000.0 - density * strain * strain * (2.0 * x * x + 0.5 * y * y),
                };
                (op.gas.conservative(q), [0.0; 5])
            })
            .0
        })
        .collect();
    let mut ws = op.workspace();
    op.evaluate(&state, true, &mut ws).unwrap();
    let mut force = [0.0_f64; 2];
    let mut cancellation = [0.0_f64; 2];
    for i in n / 3..2 * n / 3 {
        let terms = term_magnitudes(&op, &ws, i);
        for k in 0..2 {
            let residual = ws.residual[i][k + 1].abs();
            force[k] = force[k]
                .max(residual / (density * strain * strain * op.mesh.cells[i].center[k].abs()));
            cancellation[k] = cancellation[k].max(residual / terms[k]);
        }
    }
    (force, cancellation)
}

#[test]
fn pressure_balanced_strain_preserves_both_axis_momentum_equations() {
    for aspect in [1, 4] {
        let measurements = [48, 96, 192].map(|n| affine_balance(n, aspect));
        // A reproduced polynomial equilibrium has no truncation error whose order
        // can be measured. Bound cancellation relative to the terms being summed.
        assert!(
            measurements.iter().all(|(force, cancellation)| {
                force.iter().all(|e| *e < 1e-3) && cancellation.iter().all(|e| *e < 1e-13)
            }),
            "aspect={aspect}, force/cancellation errors={measurements:?}"
        );
    }
}

fn potential_balance(n: usize, aspect: usize) -> [f64; 2] {
    let (op, grid) = operator(n, aspect, false);
    let (density, a, b) = (0.01, 1e4, 3e5);
    let state: Vec<_> = (0..n * grid.angular_cells)
        .map(|k| {
            super::super::cell_mean(grid, k % n, k / n, |x, y| {
                let velocity = [
                    -2.0 * a * x + b * (3.0 * x * x - 1.5 * y * y),
                    a * y - 3.0 * b * x * y,
                    0.0,
                ];
                let pressure = 1e4 - 0.5 * density * crate::vector::dot(velocity, velocity);
                (
                    op.gas.conservative(Primitive {
                        density,
                        velocity,
                        pressure,
                    }),
                    [0.0; 5],
                )
            })
            .0
        })
        .collect();
    let mut ws = op.workspace();
    op.evaluate(&state, true, &mut ws).unwrap();
    assert!((n / 3..2 * n / 3).all(|i| ws.shock_sensor[i] == 0.0));
    std::array::from_fn(|k| {
        (n / 3..2 * n / 3)
            .map(|i| {
                let [x, y, _] = op.mesh.cells[i].center;
                let u = -2.0 * a * x + b * (3.0 * x * x - 1.5 * y * y);
                let v = a * y - 3.0 * b * x * y;
                let acceleration = if k == 0 {
                    u * (-2.0 * a + 6.0 * b * x) - v * 3.0 * b * y
                } else {
                    -u * 3.0 * b * y + v * (a - 3.0 * b * x)
                };
                ws.residual[i][k + 1].abs() / (density * acceleration.abs())
            })
            .fold(0.0_f64, f64::max)
    })
}

#[test]
fn harmonic_cubic_potential_preserves_axis_momentum_balance_under_refinement() {
    for aspect in [1, 4] {
        let e = [48, 96, 192].map(|n| potential_balance(n, aspect));
        assert!(
            (0..2).all(|k| e[0][k] / e[1][k] > 3.0 && e[1][k] / e[2][k] > 3.0 && e[2][k] < 1e-3),
            "aspect={aspect}, harmonic potential errors={e:?}"
        );
    }
}

fn cubic_pressure_force(n: usize, aspect: usize) -> [f64; 2] {
    let curvature = 2e8;
    let (op, grid) = operator(n, aspect, true);
    let data: Vec<_> = (0..n * grid.angular_cells)
        .map(|k| {
            super::super::cell_mean(grid, k % n, k / n, |x, _| {
                let q = Primitive {
                    density: 0.01,
                    pressure: 3000.0 + curvature * x * x * x,
                    velocity: [0.0; 3],
                };
                (
                    op.gas.conservative(q),
                    [0.0, -3.0 * curvature * x * x, 0.0, 0.0, 0.0],
                )
            })
        })
        .collect();
    let state: Vec<_> = data.iter().map(|v| v.0).collect();
    let mut ws = op.workspace();
    op.evaluate(&state, true, &mut ws).unwrap();
    let mut peaks = [0.0_f64; 2];
    for j in 0..grid.angular_cells / 3 {
        for i in n / 3..2 * n / 3 {
            let id = j * n + i;
            assert_eq!(ws.shock_sensor[id], 0.0);
            for (k, peak) in peaks.iter_mut().enumerate() {
                *peak = peak.max(
                    (ws.residual[id][k + 1] - data[id].1[k + 1]).abs()
                        / (curvature * grid.radius * grid.radius),
                );
            }
        }
    }
    peaks
}

#[test]
fn axial_cubic_pressure_has_no_spurious_radial_force() {
    for aspect in [1, 4] {
        let errors = [24, 48, 96].map(|n| cubic_pressure_force(n, aspect));
        assert!(
            errors.iter().flatten().all(|e| *e < 1e-8),
            "aspect={aspect}, cubic pressure errors={errors:?}"
        );
    }
}

fn exponential_pressure_force(n: usize, aspect: usize) -> ([f64; 2], [f64; 2]) {
    let (op, grid) = operator(n, aspect, true);
    let data: Vec<_> = (0..n * grid.angular_cells)
        .map(|k| {
            super::super::cell_mean(grid, k % n, k / n, |x, y| {
                let pressure = 3000.0 * (150.0 * x + 5000.0 * y * y).exp();
                let q = Primitive {
                    density: 0.01,
                    pressure,
                    velocity: [0.0; 3],
                };
                (
                    op.gas.conservative(q),
                    [0.0, -150.0 * pressure, -10000.0 * y * pressure, 0.0, 0.0],
                )
            })
        })
        .collect();
    let state: Vec<_> = data.iter().map(|v| v.0).collect();
    let mut ws = op.workspace();
    op.evaluate(&state, true, &mut ws).unwrap();
    assert!((n / 3..2 * n / 3).all(|i| ws.shock_sensor[i] == 0.0));
    let errors = std::array::from_fn(|k| {
        (n / 3..2 * n / 3)
            .map(|i| (ws.residual[i][k + 1] / data[i].1[k + 1] - 1.0).abs())
            .fold(0.0_f64, f64::max)
    });
    let noise = std::array::from_fn(|k| {
        (n / 3..2 * n / 3)
            .map(|i| {
                (ws.residual[i][k + 1] - data[i].1[k + 1]).abs()
                    / (1e-13 * term_magnitudes(&op, &ws, i)[k])
            })
            .fold(0.0_f64, f64::max)
    });
    (errors, noise)
}

#[test]
fn refined_exponential_force_respects_truncation_and_arithmetic_bounds() {
    for aspect in [1, 4] {
        let measurements = [48, 96, 192].map(|n| exponential_pressure_force(n, aspect));
        // A ratio of errors beneath the cancellation budget cannot measure order.
        // Resolved waves separately enforce the order on these same fine grids.
        assert!(
            measurements
                .iter()
                .all(|(e, _)| e.iter().all(|e| *e < 1e-3))
                && measurements.windows(2).all(|pair| (0..2).all(|k| {
                    pair[0].0[k] / pair[1].0[k] > 3.0
                        || (pair[0].1[k] <= 1.0 && pair[1].1[k] <= 1.0)
                })),
            "aspect={aspect}, exponential force/cancellation errors={measurements:?}"
        );
    }
}

fn pressure_wave_force(n: usize, aspect: usize) -> ([f64; 2], [f64; 2]) {
    let (op, grid) = operator(n, aspect, true);
    let (amplitude, frequency) = (60.0, 1500.0);
    let data: Vec<_> = (0..n * grid.angular_cells)
        .map(|id| {
            super::super::cell_mean(grid, id % n, id / n, |x, y| {
                let pressure = 3000.0 + amplitude * ((frequency * x).cos() + (frequency * y).cos());
                let q = Primitive {
                    density: 0.01,
                    pressure,
                    velocity: [0.0; 3],
                };
                (
                    op.gas.conservative(q),
                    [
                        0.0,
                        amplitude * frequency * (frequency * x).sin(),
                        amplitude * frequency * (frequency * y).sin(),
                        0.0,
                        0.0,
                    ],
                )
            })
        })
        .collect();
    let state: Vec<_> = data.iter().map(|v| v.0).collect();
    let mut ws = op.workspace();
    op.evaluate(&state, true, &mut ws).unwrap();
    assert!((n / 3..2 * n / 3).all(|i| ws.shock_sensor[i] == 0.0));
    let errors = std::array::from_fn(|k| {
        (n / 3..2 * n / 3)
            .map(|i| {
                let scale = if k == 0 {
                    amplitude * frequency
                } else {
                    amplitude * frequency * frequency * op.mesh.cells[i].center[1]
                };
                (ws.residual[i][k + 1] - data[i].1[k + 1]).abs() / scale
            })
            .fold(0.0_f64, f64::max)
    });
    let noise = std::array::from_fn(|k| {
        (n / 3..2 * n / 3)
            .map(|i| {
                (ws.residual[i][k + 1] - data[i].1[k + 1]).abs()
                    / (1e-13 * term_magnitudes(&op, &ws, i)[k])
            })
            .fold(0.0_f64, f64::max)
    });
    (errors, noise)
}

#[test]
fn resolved_pressure_waves_retain_the_axis_force_convergence_order() {
    for aspect in [1, 4] {
        let values = [48, 96, 192].map(|n| pressure_wave_force(n, aspect));
        let e = values.map(|(e, _)| e);
        let noise = values.map(|(_, noise)| noise);
        assert!(
            (0..2).all(|k| e[0][k] / e[1][k] > 3.0
                && e[1][k] / e[2][k] > 3.0
                && e[2][k] < 1e-3
                && noise[2][k] > 1.0),
            "aspect={aspect}, pressure wave errors={e:?}, noise ratios={noise:?}"
        );
    }
}

fn directional_axis_force(angular: usize) -> [f64; 2] {
    let n = 48;
    let (op, grid) = operator(n, angular / n, true);
    let state: Vec<_> = (0..n * angular)
        .map(|id| {
            super::super::cell_mean(grid, id % n, id / n, |x, _| {
                let q = Primitive {
                    density: 0.01,
                    pressure: 3000.0 + 60.0 * (1500.0 * x).cos(),
                    velocity: [0.0; 3],
                };
                (op.gas.conservative(q), [0.0; 5])
            })
            .0
        })
        .collect();
    let mut ws = op.workspace();
    op.evaluate(&state, true, &mut ws).unwrap();
    assert!((n / 3..2 * n / 3).all(|i| ws.shock_sensor[i] == 0.0));
    let error = (n / 3..2 * n / 3)
        .map(|i| ws.residual[i][2].abs() / op.mesh.cells[i].center[1])
        .fold(0.0_f64, f64::max);
    let noise = (n / 3..2 * n / 3)
        .map(|i| 1e-13 * term_magnitudes(&op, &ws, i)[1] / op.mesh.cells[i].center[1])
        .fold(0.0_f64, f64::max);
    [error, noise]
}

#[test]
fn angular_refinement_does_not_create_a_pressure_force_pole() {
    let measurements = [48, 96, 192, 384].map(directional_axis_force);
    // A regular axisymmetric radial force is O(y). Axial interpolation error
    // must not create a 1/y pole when only the angular cells shrink.
    let bound = 2.0 * (measurements[0][0] + measurements[0][1]);
    assert!(
        measurements
            .iter()
            .all(|[force, noise]| force - noise <= bound),
        "fixed-radial axis force divided by radius, with arithmetic envelope: {measurements:?}"
    );
}

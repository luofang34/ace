use super::*;
use crate::SphereGrid;

fn operator(n: usize, wall: f64) -> FlowOperator {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let mesh = Mesh::sphere(SphereGrid {
        radius: 0.01,
        outer_radius: 0.02,
        radial_cells: n,
        angular_cells: n,
        stretching: 2.0,
        shock_shape_mach: None,
    })
    .unwrap();
    let inflow = Primitive {
        density: 1000.0 / (287.05 * 300.0),
        velocity: [0.0; 3],
        pressure: 1000.0,
    };
    FlowOperator::new(mesh, gas, inflow, wall).unwrap()
}

#[test]
fn quadratic_temperature_has_the_correct_axisymmetric_laplacian_at_the_axis() {
    let errors = [12, 24, 48].map(|n| {
        let op = operator(n, 300.0);
        let curvature = 1.0e6;
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                let temperature = 300.0 + curvature * cell.center[1].powi(2);
                op.gas.conservative(Primitive {
                    density: op.inflow.pressure / (op.gas.gas_constant * temperature),
                    ..op.inflow
                })
            })
            .collect();
        let residual = op.residual(&state, true).unwrap();
        let exact = 4.0 * curvature * op.gas.conductivity;
        residual
            .iter()
            .take(n - 2)
            .skip(2)
            .map(|r| (r[4] / exact - 1.0).abs())
            .fold(0.0_f64, f64::max)
    });
    assert!(
        errors.iter().all(|error| *error < 1.0e-8),
        "axis heat errors: {errors:?}"
    );
}

#[test]
fn axial_temperature_curvature_converges_at_the_axis() {
    let errors = [12, 24, 48].map(|n| {
        let mut op = operator(n, 300.0);
        op.mesh = Mesh::curved_sphere(SphereGrid {
            radius: 0.01,
            outer_radius: 0.02,
            radial_cells: n,
            angular_cells: n,
            stretching: 2.0,
            shock_shape_mach: None,
        })
        .unwrap();
        let curvature = 1e6;
        let state = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                let temperature = 300.0 + curvature * cell.center[0].powi(2);
                op.gas.conservative(Primitive {
                    density: op.inflow.pressure / (op.gas.gas_constant * temperature),
                    ..op.inflow
                })
            })
            .collect::<Vec<_>>();
        let residual = op.residual(&state, true).unwrap();
        let exact = 2.0 * curvature * op.gas.conductivity;
        (n / 3..2 * n / 3)
            .map(|i| (residual[i][4] / exact - 1.0).abs())
            .fold(0.0_f64, f64::max)
    });
    assert!(
        errors[0] / errors[1] > 3.0 && errors[1] / errors[2] > 3.0,
        "axial-curvature errors at the axis: {errors:?}"
    );
}

#[test]
fn affine_axisymmetric_strain_has_zero_viscous_force_at_the_axis() {
    let mut op = operator(12, 300.0);
    let state: Vec<_> = op
        .mesh
        .cells
        .iter()
        .map(|c| {
            op.gas.conservative(Primitive {
                velocity: [-200.0 * c.center[0], 100.0 * c.center[1], 0.0],
                ..op.inflow
            })
        })
        .collect();
    let first = op.residual(&state, true).unwrap();
    op.gas.viscosity *= 2.0;
    let second = op.residual(&state, true).unwrap();
    for i in 2..9 {
        for k in 1..3 {
            assert!(
                (second[i][k] - first[i][k]).abs() < 1.0e-6,
                "cell={i}, component={k}, difference={}",
                second[i][k] - first[i][k]
            );
        }
    }
}

#[test]
fn affine_axisymmetric_convection_preserves_axis_strain_under_refinement() {
    let errors = [24, 48, 96].map(|n| {
        let op = operator(n, 300.0);
        let strain: f64 = 1000.0;
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                op.gas.conservative(Primitive {
                    velocity: [-2.0 * strain * cell.center[0], strain * cell.center[1], 0.0],
                    ..op.inflow
                })
            })
            .collect();
        let residual = op.residual(&state, true).unwrap();
        (n / 3..2 * n / 3)
            .map(|i| {
                let exact = -op.inflow.density * strain.powi(2) * op.mesh.cells[i].center[1];
                (residual[i][2] / exact - 1.0).abs()
            })
            .fold(0.0_f64, f64::max)
    });
    assert!(errors[2] < 0.01, "axis strain errors: {errors:?}");
    assert!(
        errors[0] / errors[1] > 3.5 && errors[1] / errors[2] > 3.5,
        "axis strain convergence: {errors:?}"
    );
}

#[test]
fn shock_flattening_preserves_a_smooth_transverse_velocity() {
    let op = operator(12, 300.0);
    let state: Vec<_> = op
        .mesh
        .cells
        .iter()
        .map(|cell| {
            op.gas.conservative(Primitive {
                pressure: 100.0 + 1.0e6 * (cell.center[0] + 0.02),
                velocity: [0.0, 100.0 * cell.center[1], 0.0],
                ..op.inflow
            })
        })
        .collect();
    let mut workspace = op.workspace();
    op.prepare(&state, &mut workspace).unwrap();
    let face = &op.mesh.faces[op.mesh.cells[8].faces[3]];
    assert!(workspace.shock_sensor[face.owner] > 0.1);
    for cell in [face.owner, face.neighbor.unwrap()] {
        let q = op.face_state(face, cell, true, &workspace);
        assert!((q.velocity[1] - 100.0 * face.center[1]).abs() < 1e-10);
    }
}

#[test]
fn shock_sensor_transitions_keep_the_spatial_residual_continuous() {
    for scheme in [Convection::default(), Convection::Ausmdv] {
        let op = operator(12, 300.0).with_convection(scheme);
        for threshold in [0.05, 0.20] {
            let differences = [1.0e-4, 1.0e-6, 1.0e-8].map(|epsilon| {
                let residuals = [-1.0, 1.0].map(|sign| {
                    let ratio = threshold + sign * epsilon;
                    let pressure = op.inflow.pressure * (1.0 + ratio) / (1.0 - ratio);
                    let state: Vec<_> = op
                        .mesh
                        .cells
                        .iter()
                        .enumerate()
                        .map(|(i, _)| {
                            op.gas.conservative(Primitive {
                                pressure: if i % 12 < 6 {
                                    op.inflow.pressure
                                } else {
                                    pressure
                                },
                                velocity: [200.0, 10.0, 0.0],
                                ..op.inflow
                            })
                        })
                        .collect();
                    op.residual(&state, true).unwrap()
                });
                residuals[0]
                    .iter()
                    .flatten()
                    .zip(residuals[1].iter().flatten())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0_f64, f64::max)
            });
            // A hard switch leaves a finite residual jump as the input
            // perturbation shrinks; a continuous transition contracts it.
            assert!(
                differences[0] > 0.0
                    && differences[1] < 0.02 * differences[0]
                    && differences[2] < 0.02 * differences[1],
                "{scheme:?}, threshold={threshold}, residual jumps={differences:?}"
            );
        }
    }
}

#[test]
fn directional_reconstruction_keeps_positive_thermodynamic_face_states() {
    let op = operator(8, 300.0);
    for slope in [200.0, 500.0, 1000.0, 2000.0] {
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                op.gas.conservative(Primitive {
                    pressure: 1000.0 * (slope * (cell.center[0] + cell.center[1])).exp(),
                    density: op.inflow.density * (slope * (cell.center[0] - cell.center[1])).exp(),
                    ..op.inflow
                })
            })
            .collect();
        let mut workspace = op.workspace();
        op.prepare(&state, &mut workspace).unwrap();
        for face in &op.mesh.faces {
            for cell in [Some(face.owner), face.neighbor].into_iter().flatten() {
                let q = op.face_state(face, cell, true, &workspace);
                assert!(
                    q.density > 0.0 && q.pressure > 0.0,
                    "slope={slope}, cell={cell}, rho={}, p={}",
                    q.density,
                    q.pressure
                );
            }
        }
    }
}

#[test]
fn axis_reflection_preserves_tangential_position() {
    let op = operator(12, 300.0);
    let q = Primitive {
        velocity: [1.0, 2.0, 0.0],
        ..op.inflow
    };
    let states = vec![q; op.mesh.cell_count()];
    for face in op
        .mesh
        .faces
        .iter()
        .filter(|f| f.neighbor.is_none() && matches!(f.boundary, Boundary::Axis))
    {
        let (ghost, position) = op.sample(face, face.owner, &states);
        let center = op.mesh.cells[face.owner].center;
        assert!((position[0] - center[0]).abs() < 1.0e-15);
        assert!((position[1] + center[1]).abs() < 1.0e-15);
        assert_eq!(ghost.velocity, [1.0, -2.0, 0.0]);
    }
}

#[test]
fn prescribed_inflow_temperature_is_applied_at_the_boundary_face() {
    let op = operator(12, 400.0);
    let state = vec![
        op.gas.conservative(Primitive {
            density: op.inflow.pressure / (op.gas.gas_constant * 400.0),
            ..op.inflow
        });
        op.mesh.cell_count()
    ];
    let mut workspace = op.workspace();
    op.prepare(&state, &mut workspace).unwrap();
    for face in op
        .mesh
        .faces
        .iter()
        .filter(|face| face.neighbor.is_none() && matches!(face.boundary, Boundary::Inflow))
    {
        let gradient = op.face_gradient(
            face,
            workspace.primitive[face.owner],
            workspace.gradient[face.owner],
            &workspace,
        );
        let delta = sub(face.center, op.mesh.cells[face.owner].center);
        assert!((dot(gradient[3], delta) + 100.0).abs() < 1.0e-9);
    }
}

#[test]
fn uniform_rest_is_an_exact_discrete_equilibrium() {
    let op = operator(12, 300.0);
    let residual = op.residual(&op.initial_state(), true).unwrap();
    for r in residual {
        assert!(r.iter().all(|v| v.abs() < 1.0e-5), "{r:?}");
    }
    let wall = op.wall_samples(&op.initial_state()).unwrap();
    assert!(wall.iter().all(|w| w.heat_flux.abs() < 1.0e-8));
}

#[test]
fn spherical_conduction_interior_residual_decreases_under_refinement() {
    let errors = [12, 24, 48].map(|n| {
        let op = operator(n, 400.0);
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|c| {
                let temperature = 200.0 + 2.0 / crate::vector::norm(c.center);
                op.gas.conservative(Primitive {
                    density: op.inflow.pressure / (op.gas.gas_constant * temperature),
                    ..op.inflow
                })
            })
            .collect();
        let residual = op.residual(&state, true).unwrap();
        let mut square = 0.0;
        let mut volume = 0.0;
        // Boundary-adjacent stencils see the faceted wall, not the analytic sphere.
        for j in 0..n - 2 {
            for i in 2..n - 2 {
                let k = j * n + i;
                let v = op.mesh.cells[k].volume;
                square += residual[k][4].powi(2) * v;
                volume += v;
            }
        }
        (square / volume).sqrt()
    });
    assert!(errors[0] / errors[1] > 3.0, "{errors:?}");
    assert!(errors[1] / errors[2] > 3.0, "{errors:?}");
}

#[test]
fn spherical_conduction_wall_flux_converges_to_analytic_value() {
    let errors = [32, 64, 128].map(|n| {
        let op = operator(n, 400.0);
        // Align each radial T=A+B/r profile with its faceted wall face.
        // This verifies the wall derivative, not a solved flow or global conduction field.
        let states: Vec<_> = op
            .mesh
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let r = crate::vector::norm(c.center);
                let wall_face = &op.mesh.faces[op.mesh.cells[(i / n) * n].faces[0]];
                let face_radius_factor = crate::vector::norm(wall_face.center) / op.mesh.radius;
                let temperature = 200.0 + 2.0 * face_radius_factor / r;
                op.gas.conservative(Primitive {
                    density: 1000.0 / (op.gas.gas_constant * temperature),
                    velocity: [0.0; 3],
                    pressure: 1000.0,
                })
            })
            .collect();
        let wall = op.wall_samples(&states).unwrap();
        let average = wall.iter().map(|w| w.heat_flux * w.area).sum::<f64>()
            / wall.iter().map(|w| w.area).sum::<f64>();
        assert!(average < 0.0);
        let exact = wall
            .iter()
            .map(|w| -500.0 * op.mesh.radius / crate::vector::norm(w.position) * w.area)
            .sum::<f64>()
            / wall.iter().map(|w| w.area).sum::<f64>();
        (average / exact - 1.0).abs()
    });
    assert!(errors[0] > errors[1] && errors[1] > errors[2], "{errors:?}");
    assert!(errors[2] < 0.02, "{errors:?}");
    assert!(errors[0] / errors[1] > 3.5, "{errors:?}");
    assert!(errors[1] / errors[2] > 3.5, "{errors:?}");
}

#[test]
fn wall_conduction_is_the_energy_flux_used_by_the_residual() {
    let op = operator(8, 250.0);
    let state = op.initial_state();
    let mut workspace = op.workspace();
    op.evaluate(&state, true, &mut workspace).unwrap();
    let volume_rate: f64 = workspace
        .residual
        .iter()
        .zip(&op.mesh.cells)
        .map(|(r, c)| r[4] * c.volume)
        .sum();
    let wall_rate: f64 = op
        .wall_samples(&state)
        .unwrap()
        .iter()
        .map(|w| w.heat_flux * w.area)
        .sum();
    assert!((volume_rate * std::f64::consts::TAU + wall_rate).abs() < 1.0e-7);
}

#[test]
fn residual_directional_derivative_is_repeatable_without_mutating_state() {
    let op = operator(8, 300.0);
    let state = op.initial_state();
    let direction: Vec<_> = state
        .iter()
        .map(|u| [0.001 * u[0], 0.0, 0.0, 0.0, 0.0])
        .collect();
    let a = op.jacobian_vector(&state, &direction, 1.0e-3).unwrap();
    let b = op.jacobian_vector(&state, &direction, 5.0e-4).unwrap();
    let error: f64 = a
        .iter()
        .zip(b)
        .map(|(a, b)| a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>())
        .sum();
    let magnitude: f64 = a.iter().flatten().map(|x| x * x).sum();
    assert!(magnitude > 0.0 && (error / magnitude).sqrt() < 1.0e-4);
    assert_eq!(state, op.initial_state());
}

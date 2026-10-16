use crate::{Convection, FlowOperator, IdealGas, Mesh, Primitive, SphereGrid};

fn axis_stress_error(angular_cells: usize, cubic: bool) -> f64 {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: if cubic { 1.8e-3 } else { 1.8e-5 },
        conductivity: 0.025,
    };
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.019047715851066883,
        radial_cells: 48,
        angular_cells,
        stretching: 2.2753462770828685,
        shock_shape_mach: None,
    };
    let inflow = Primitive {
        density: 0.01,
        pressure: 10000.0,
        velocity: [0.0; 3],
    };
    let data: Vec<_> = (0..48 * angular_cells)
        .map(|k| {
            super::super::cell_mean(grid, k % 48, k / 48, |x, y| {
                let strain = if cubic {
                    1e8 * y * y
                } else {
                    20000.0 + 1e6 * x
                };
                let q = Primitive {
                    velocity: [0.0, y * strain, 0.0],
                    ..inflow
                };
                let force = if cubic {
                    (16.0 / 3.0) * gas.viscosity * 1e8 * y
                } else {
                    0.0
                };
                (gas.conservative(q), [0.0, 0.0, force, 0.0, 0.0])
            })
        })
        .collect();
    let state: Vec<_> = data.iter().map(|(u, _)| *u).collect();
    let residuals = [1.0, 0.5].map(|fraction| {
        let material = IdealGas {
            viscosity: fraction * gas.viscosity,
            ..gas
        };
        FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), material, inflow, 300.0)
            .unwrap()
            .with_convection(Convection::Ausmdv)
            .residual(&state, true)
            .unwrap()
    });
    // Changing viscosity isolates the stress divergence through the complete operator.
    // For v=y*(a+b*x) it vanishes; for v=c*y³ it is (32/3)*mu*c*y.
    // The viscosity difference is mu/2, and the independent quadrature averages y.
    (12..24)
        .map(|i| {
            let reference = if cubic {
                data[i].1[2].abs()
            } else {
                gas.viscosity * 1e6
            };
            (residuals[0][i][2] - residuals[1][i][2] - data[i].1[2]).abs() / reference
        })
        .fold(0.0_f64, f64::max)
}

#[test]
fn affine_regular_strain_has_no_spurious_viscous_axis_force() {
    let errors = [48, 192, 768].map(|n| axis_stress_error(n, false));
    assert!(
        errors.into_iter().all(|e| e < 1e-7),
        "axis stress errors={errors:?}"
    );
}

#[test]
fn cubic_radial_velocity_has_the_analytic_viscous_axis_force() {
    let errors = [48, 192, 768].map(|n| axis_stress_error(n, true));
    assert!(
        errors.into_iter().all(|e| e < 1e-6),
        "cubic radial strain errors={errors:?}"
    );
}

fn harmonic_velocity_stress_error(n: usize, aspect: usize) -> [f64; 2] {
    let (base, mut grid) = super::super::super::tests::strain_operator(n, Convection::Ausmdv);
    grid.angular_cells = aspect * n;
    let gas = base.gas;
    let (a, b) = (1e4, 3e5);
    let state: Vec<_> = (0..n * grid.angular_cells)
        .map(|k| {
            super::super::cell_mean(grid, k % n, k / n, |x, y| {
                let velocity = [
                    -2.0 * a * x + b * (3.0 * x * x - 1.5 * y * y),
                    a * y - 3.0 * b * x * y,
                    0.0,
                ];
                (
                    gas.conservative(Primitive {
                        velocity,
                        ..base.inflow
                    }),
                    [0.0; 5],
                )
            })
            .0
        })
        .collect();
    let residuals = [1.0, 0.5].map(|fraction| {
        let material = IdealGas {
            viscosity: fraction * gas.viscosity,
            ..gas
        };
        FlowOperator::new(
            Mesh::curved_sphere(grid).unwrap(),
            material,
            base.inflow,
            300.0,
        )
        .unwrap()
        .with_convection(Convection::Ausmdv)
        .residual(&state, true)
        .unwrap()
    });
    // The velocity is the gradient of a harmonic cubic potential. Its vector
    // Laplacian and dilatation vanish, hence so does the full viscous divergence.
    std::array::from_fn(|k| {
        (n / 3..2 * n / 3)
            .map(|i| {
                (residuals[0][i][k + 1] - residuals[1][i][k + 1]).abs() / (0.5 * gas.viscosity * b)
            })
            .fold(0.0_f64, f64::max)
    })
}

#[test]
fn harmonic_quadratic_velocity_has_no_spurious_viscous_axis_force() {
    let errors =
        [1, 4].map(|aspect| [48, 96, 192].map(|n| harmonic_velocity_stress_error(n, aspect)));
    assert!(
        errors.iter().flatten().flatten().all(|e| *e < 1e-7),
        "harmonic velocity viscous force errors={errors:?}"
    );
}

#[test]
fn axial_curvature_preserves_volume_means_and_planar_stress() {
    let velocity = |x: f64, y: f64| {
        30.0 + 400.0 * x + 2e6 * x * x + 3e6 * y * y + 2e8 * x * y * y + 2e9 * y.powi(4)
    };
    for n in [12, 24] {
        let (base, mut grid) = super::super::super::tests::strain_operator(n, Convection::Ausmdv);
        grid.shock_shape_mach = Some(3.0);
        let op = FlowOperator::new(
            Mesh::curved_sphere(grid).unwrap(),
            base.gas,
            base.inflow,
            300.0,
        )
        .unwrap();
        let state: Vec<_> = (0..n * n)
            .map(|k| {
                super::super::cell_mean(grid, k % n, k / n, |x, y| {
                    (
                        op.gas.conservative(Primitive {
                            velocity: [velocity(x, y), 0.0, 0.0],
                            ..op.inflow
                        }),
                        [0.0; 5],
                    )
                })
                .0
            })
            .collect();
        let mut ws = op.workspace();
        op.prepare(&state, &mut ws).unwrap();
        for j in 0..2 * n / 3 {
            for i in n / 3..2 * n / 3 {
                let id = j * n + i;
                let cell = &op.mesh.cells[id];
                let reconstructed = |x, y| {
                    ws.primitive[id].velocity[0]
                        + crate::operator::axial::change(cell, ws.axial[id], [x, y, 0.0])
                };
                for face in cell.faces {
                    for point in op.mesh.faces[face].integration_points() {
                        let [x, y, _] = point.center;
                        assert!((reconstructed(x, y) / velocity(x, y) - 1.0).abs() < 1e-9);
                    }
                }
                let mean = super::super::cell_mean(grid, i, j, |x, y| {
                    ([reconstructed(x, y), 0.0, 0.0, 0.0, 0.0], [0.0; 5])
                })
                .0[0];
                assert!((mean / ws.primitive[id].velocity[0] - 1.0).abs() < 1e-10);
                let exact = super::super::cell_mean(grid, i, j, |x, y| {
                    (
                        [
                            -2.0 / 3.0 * op.gas.viscosity * (400.0 + 4e6 * x + 2e8 * y * y) / y,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                        ],
                        [0.0; 5],
                    )
                })
                .0[0]
                    * cell.volume
                    / cell.planar_area;
                let actual = crate::operator::axial::hoop_stress(&op, &ws, id);
                assert!(
                    (actual - exact).abs() / op.gas.viscosity < 1e-4,
                    "n={n}, cell={id}, planar hoop stress={actual}, exact={exact}"
                );
            }
        }
    }
}

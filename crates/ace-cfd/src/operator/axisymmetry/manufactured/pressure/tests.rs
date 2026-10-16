use crate::{Convection, FlowOperator, IdealGas, Mesh, Primitive, SphereGrid};

fn axis_pressure_force(angular_cells: usize, second_order: bool) -> [f64; 4] {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.019047715851066883,
        radial_cells: 48,
        angular_cells,
        stretching: 2.2753462770828685,
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
        pressure: 300.0,
        velocity: [0.0; 3],
    };
    let op = FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0)
        .unwrap()
        .with_convection(Convection::Ausmdv);
    let state: Vec<_> = (0..48 * angular_cells)
        .map(|k| {
            super::super::cell_mean(grid, k % 48, k / 48, |x, y| {
                let transition = ((x.hypot(y) - 0.013) / 0.00008).tanh();
                let pressure = 300.0 + 1350.0 * (1.0 - transition);
                (gas.conservative(Primitive { pressure, ..inflow }), [0.0; 5])
            })
            .0
        })
        .collect();
    let mut workspace = op.workspace();
    op.evaluate(&state, second_order, &mut workspace).unwrap();
    std::array::from_fn(|k| {
        let i = 27 + k;
        assert_eq!(workspace.shock_sensor[i], 1.0);
        workspace.residual[i][2] / op.mesh.cells[i].center[1]
    })
}

#[test]
fn pressure_source_and_flattened_faces_preserve_a_regular_axis_force() {
    for second_order in [false, true] {
        let forces = [48, 192, 768].map(|n| axis_pressure_force(n, second_order));
        // A radial pressure layer has a radial-momentum residual proportional to y.
        // Holding radial resolution fixed must not introduce a geometric 1/y pole.
        for refined in &forces[1..] {
            for (coarse, fine) in forces[0].iter().zip(refined) {
                assert!(
                    (fine / coarse - 1.0).abs() < 1e-6,
                    "second_order={second_order}, axis force divided by radius={forces:?}"
                );
            }
        }
    }
}

#[test]
fn isothermal_wall_retains_an_affine_pressure_force() {
    let slope = 3e4;
    for n in [24, 48, 96] {
        let grid = SphereGrid {
            radius: 0.01,
            outer_radius: 0.019,
            radial_cells: n,
            angular_cells: n,
            stretching: 2.275,
            shock_shape_mach: None,
        };
        let gas = IdealGas {
            gamma: 1.4,
            gas_constant: 287.05,
            viscosity: 1.8e-5,
            conductivity: 0.025,
        };
        let inflow = Primitive {
            density: 3600.0 / (gas.gas_constant * 300.0),
            pressure: 3600.0,
            velocity: [0.0; 3],
        };
        let op = FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0)
            .unwrap()
            .with_convection(Convection::Ausmdv);
        let state: Vec<_> = (0..n * n)
            .map(|k| {
                super::super::cell_mean(grid, k % n, k / n, |x, _| {
                    let pressure = 3600.0 + slope * (x + grid.radius);
                    let q = Primitive {
                        pressure,
                        density: pressure / (gas.gas_constant * 300.0),
                        ..inflow
                    };
                    (gas.conservative(q), [0.0; 5])
                })
                .0
            })
            .collect();
        let mut workspace = op.workspace();
        op.evaluate(&state, true, &mut workspace).unwrap();
        // No-slip and fixed temperature do not prescribe a zero pressure gradient.
        // A uniform momentum source balances this manufactured pressure field; the
        // unforced operator must retain its exact pressure force beside the wall.
        for j in 0..n - 2 {
            for i in 0..2 {
                let k = j * n + i;
                let g = workspace.gradient[k].raw[4];
                let r = workspace.residual[k];
                assert!(
                    (g[0] / slope - 1.0).abs() < 1e-7 && (g[1] / slope).abs() < 1e-7,
                    "n={n}, cell={k}, pressure gradient={g:?}"
                );
                assert!(
                    (r[1] / slope + 1.0).abs() < 1e-7 && (r[2] / slope).abs() < 1e-7,
                    "n={n}, cell={k}, momentum residual={r:?}"
                );
            }
        }
        for wall in op.wall_samples(&state).unwrap().iter().take(n - 2) {
            let expected = 3600.0 + slope * (wall.position[0] + grid.radius);
            assert!(
                (wall.pressure / expected - 1.0).abs() < 1e-10,
                "n={n}, wall pressure={}, expected={expected}",
                wall.pressure
            );
        }
    }
}

#[test]
fn isobaric_thermal_layer_has_no_instantaneous_mass_or_momentum_force() {
    let errors = [24, 48, 96].map(|n| {
        let grid = SphereGrid {
            radius: 0.01,
            outer_radius: 0.02,
            radial_cells: n,
            angular_cells: n,
            stretching: 2.0,
            shock_shape_mach: None,
        };
        let gas = IdealGas {
            gamma: 1.4,
            gas_constant: 287.05,
            viscosity: 1.8e-5,
            conductivity: 0.025,
        };
        let pressure = 3000.0;
        let inflow = Primitive {
            density: pressure / (gas.gas_constant * 840.0),
            pressure,
            velocity: [0.0; 3],
        };
        let op = FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0)
            .unwrap()
            .with_convection(Convection::Ausmdv);
        let state: Vec<_> = (0..n * n)
            .map(|k| {
                super::super::cell_mean(grid, k % n, k / n, |x, y| {
                    let temperature =
                        300.0 + 540.0 * -(-(x.hypot(y) - grid.radius) / 0.00016).exp_m1();
                    (
                        gas.conservative(Primitive {
                            density: pressure / (gas.gas_constant * temperature),
                            ..inflow
                        }),
                        [0.0; 5],
                    )
                })
                .0
            })
            .collect();
        let residual = op.residual(&state, true).unwrap();
        let mut error = [0.0_f64; 3];
        for j in 0..n / 3 {
            for i in 0..n / 3 {
                let r = residual[j * n + i];
                error[0] = error[0]
                    .max(r[0].abs() * grid.radius / (inflow.density * gas.sound_speed(inflow)));
                error[1] = error[1].max(r[1].abs() * grid.radius / pressure);
                error[2] = error[2].max(r[2].abs() * grid.radius / pressure);
            }
        }
        error
    });
    // Thermal diffusion changes energy, but a stationary isobaric field has
    // zero instantaneous mass and momentum derivatives.
    assert!(
        errors.iter().flatten().all(|e| *e < 1e-8),
        "isobaric layer mass/momentum errors={errors:?}"
    );
}

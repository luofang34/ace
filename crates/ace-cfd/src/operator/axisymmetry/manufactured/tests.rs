use crate::vector::Vector;
use crate::{Conserved, Convection, FlowOperator, IdealGas, Mesh, Primitive};

fn wall_thermal_field(gas: IdealGas, x: f64, y: f64, strain: f64) -> Conserved {
    let radius = x.hypot(y);
    let distance = radius - 0.01;
    let sine = y / radius;
    let cosine = -x / radius;
    let rho = 0.01 * (1.0 + 0.4 * sine * sine) * (1.0 + 80.0 * distance);
    let temperature =
        300.0 + 5e5 * (1.0 + 0.5 * sine * sine) * distance + 2e7 * distance * distance;
    let tangential = strain * sine * distance;
    let normal = -strain * cosine * distance * distance / 0.01;
    gas.conservative(Primitive {
        density: rho,
        pressure: rho * gas.gas_constant * temperature,
        velocity: [
            -normal * cosine + tangential * sine,
            normal * sine + tangential * cosine,
            0.0,
        ],
    })
}

#[test]
fn wall_heat_converges_with_coupled_density_temperature_and_no_slip_strain() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let inflow = Primitive {
        density: 0.01,
        pressure: 0.01 * gas.gas_constant * 300.0,
        velocity: [0.0; 3],
    };
    for strain in [0.0, 3e5] {
        let errors = [16, 32, 64].map(|n| {
            let grid = crate::SphereGrid {
                radius: 0.01,
                outer_radius: 0.015,
                radial_cells: n,
                angular_cells: n,
                stretching: 0.0,
                shock_shape_mach: None,
            };
            let op =
                FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0).unwrap();
            // Independently integrate conservative states, including kinetic energy.
            let state: Vec<_> = (0..n * n)
                .map(|k| {
                    super::cell_mean(grid, k % n, k / n, |x, y| {
                        (wall_thermal_field(gas, x, y, strain), [0.0; 5])
                    })
                    .0
                })
                .collect();
            let wall = op.wall_samples(&state).unwrap();
            let errors: Vec<_> = wall
                .iter()
                .enumerate()
                .map(|(j, sample)| {
                    let a = (j as f64 * std::f64::consts::FRAC_PI_2 / n as f64).cos();
                    let b = ((j + 1) as f64 * std::f64::consts::FRAC_PI_2 / n as f64).cos();
                    let sine_squared_mean = 1.0 - (a * a + a * b + b * b) / 3.0;
                    let exact = gas.conductivity * 5e5 * (1.0 + 0.5 * sine_squared_mean);
                    (sample.heat_flux / exact - 1.0).abs()
                })
                .collect();
            [errors[0], errors.into_iter().fold(0.0_f64, f64::max)]
        });
        for metric in 0..2 {
            assert!(
                errors[0][metric] / errors[1][metric] > 3.5
                    && errors[1][metric] / errors[2][metric] > 3.5
                    && errors[2][metric] < 0.001,
                "strain={strain}, axis/max relative wall errors={errors:?}"
            );
        }
    }
}

#[test]
fn resolved_thin_thermal_layer_retains_second_order_wall_heat() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025116875,
    };
    let thickness = 0.00016;
    let pressure = 3000.0;
    let inflow = Primitive {
        density: pressure / (gas.gas_constant * 300.0),
        pressure,
        velocity: [0.0; 3],
    };
    for strain in [0.0, 1.0] {
        let errors = [384, 768, 1536].map(|n| {
            let grid = crate::SphereGrid {
                radius: 0.01,
                outer_radius: 0.019047715851066883,
                radial_cells: n,
                angular_cells: 8,
                stretching: 2.2753462770828685,
                shock_shape_mach: None,
            };
            let op =
                FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0).unwrap();
            // Constant pressure couples a steep density profile to the thermal layer.
            // Exact conservative means distinguish fitting error from point sampling.
            let state: Vec<_> = (0..n * grid.angular_cells)
                .map(|k| {
                    super::cell_mean(grid, k % n, k / n, |x, y| {
                        let distance = x.hypot(y) - grid.radius;
                        let temperature = 300.0 + 540.0 * -(-distance / thickness).exp_m1();
                        let decay = (-distance / 0.002).exp();
                        let q = Primitive {
                            density: pressure / (gas.gas_constant * temperature),
                            velocity: [
                                strain * 3e8 * distance * distance * decay,
                                strain * y * 7e8 * distance * decay,
                                0.0,
                            ],
                            ..inflow
                        };
                        (gas.conservative(q), [0.0; 5])
                    })
                    .0
                })
                .collect();
            let exact = gas.conductivity * 540.0 / thickness;
            // Kinetic energy has zero wall-normal derivative at a no-slip wall.
            // Its fitting error can cancel thermal error in the surface integral;
            // the maximum local error must converge for both velocity fields.
            op.wall_samples(&state)
                .unwrap()
                .iter()
                .map(|w| (w.heat_flux / exact - 1.0).abs())
                .fold(0.0_f64, f64::max)
        });
        assert!(
            errors[0] / errors[1] > 3.5 && errors[1] / errors[2] > 3.5 && errors[2] < 0.0003,
            "strain={strain}, resolved thin-layer relative heat errors={errors:?}"
        );
    }
}

#[test]
fn axial_velocity_recovers_the_volume_mean_from_density_weighted_momentum() {
    let errors = [24, 48, 96].map(|n| {
        let (op, grid) = super::super::tests::strain_operator(n, Convection::Ausmdv);
        let state: Vec<_> = (0..n * n)
            .map(|k| {
                super::cell_mean(grid, k % n, k / n, |x, _y| {
                    (
                        op.gas.conservative(Primitive {
                            density: 0.01 * (1.0 + 20.0 * x),
                            velocity: [500.0 + 1e4 * x, 0.0, 0.0],
                            pressure: 1000.0,
                        }),
                        [0.0; 5],
                    )
                })
                .0
            })
            .collect();
        let mut workspace = op.workspace();
        op.prepare(&state, &mut workspace).unwrap();
        (n / 3..2 * n / 3)
            .map(|i| {
                (workspace.primitive[i].velocity[0] / (500.0 + 1e4 * op.mesh.cells[i].center[0])
                    - 1.0)
                    .abs()
            })
            .fold(0.0_f64, f64::max)
    });
    assert!(
        errors[0] / errors[1] > 12.0 && errors[1] / errors[2] > 12.0 && errors[2] < 1e-9,
        "density-weighted axial velocity recovery: {errors:?}"
    );
}

fn coupled_flow(gas: IdealGas, x: f64, y: f64) -> (Conserved, Conserved) {
    let rho = 0.01 * (1.0 + 20.0 * x + 300.0 * x * x + 2000.0 * y * y);
    let rho_x = 0.01 * (20.0 + 600.0 * x);
    let rho_y = 40.0 * y;
    let u = 100.0 + 1000.0 * x + 1e5 * (x * x + y * y);
    let u_x = 1000.0 + 2e5 * x;
    let u_y = 2e5 * y;
    let hoop = 1000.0 + 1e4 * x + 1e6 * y * y;
    let v = y * hoop;
    let v_x = 1e4 * y;
    let v_y = 1000.0 + 1e4 * x + 3e6 * y * y;
    let temperature = 300.0 * (1.0 + 5.0 * x + 500.0 * (x * x + y * y));
    let t_x = 300.0 * (5.0 + 1000.0 * x);
    let t_y = 3e5 * y;
    let divergence = u_x + v_y + hoop;
    let mass_divergence = u * rho_x + v * rho_y + rho * divergence;
    // The strong cylindrical Navier–Stokes equations supply the independent source.
    let viscous_x = gas.viscosity * (8.0 / 3.0 * 1e5 + 4e5 + 2.0 / 3.0 * 1e4);
    let viscous_y = gas.viscosity * (32.0 / 3.0 * 1e6 * y);
    let xx = gas.viscosity * (2.0 * u_x - 2.0 / 3.0 * divergence);
    let yy = gas.viscosity * (2.0 * v_y - 2.0 / 3.0 * divergence);
    let zz = gas.viscosity * (2.0 * hoop - 2.0 / 3.0 * divergence);
    let xy = gas.viscosity * (u_y + v_x);
    let heating = u * viscous_x
        + v * viscous_y
        + xx * u_x
        + xy * (u_y + v_x)
        + yy * v_y
        + zz * hoop
        + gas.conductivity * 9e5;
    let enthalpy = gas.cp() * temperature + 0.5 * (u * u + v * v);
    let h_x = gas.cp() * t_x + u * u_x + v * v_x;
    let h_y = gas.cp() * t_y + u * u_y + v * v_y;
    let state = gas.conservative(Primitive {
        density: rho,
        velocity: [u, v, 0.0],
        pressure: rho * gas.gas_constant * temperature,
    });
    let residual = [
        -mass_divergence,
        -u * mass_divergence
            - rho * (u * u_x + v * u_y)
            - gas.gas_constant * (rho_x * temperature + rho * t_x)
            + viscous_x,
        -v * mass_divergence
            - rho * (u * v_x + v * v_y)
            - gas.gas_constant * (rho_y * temperature + rho * t_y)
            + viscous_y,
        0.0,
        -enthalpy * mass_divergence - rho * (u * h_x + v * h_y) + heating,
    ];
    (state, residual)
}

#[derive(Debug)]
struct CoupledError {
    relative: [f64; 4],
    limited_cells: [usize; 5],
    sensor_maximum: f64,
    pressure_limiting: Vec<(usize, Vector, Vector, f64)>,
}

fn coupled_errors(n: usize, aspect: usize) -> CoupledError {
    let (base, mut grid) = super::super::tests::strain_operator(n, Convection::Ausmdv);
    grid.angular_cells = aspect * n;
    grid.shock_shape_mach = Some(3.0);
    let op = FlowOperator::new(
        Mesh::curved_sphere(grid).unwrap(),
        base.gas,
        base.inflow,
        300.0,
    )
    .unwrap()
    .with_convection(Convection::Ausmdv);
    let data: Vec<_> = (0..grid.angular_cells * n)
        .map(|k| super::cell_mean(grid, k % n, k / n, |x, y| coupled_flow(op.gas, x, y)))
        .collect();
    let mut workspace = op.workspace();
    op.evaluate(
        &data.iter().map(|(u, _)| *u).collect::<Vec<_>>(),
        true,
        &mut workspace,
    )
    .unwrap();
    let selected: Vec<_> = (0..2 * grid.angular_cells / 3)
        .flat_map(|j| (n / 3..2 * n / 3).map(move |i| j * n + i))
        .collect();
    let limited_cells = std::array::from_fn(|k| {
        selected
            .iter()
            .filter(|i| {
                let g = workspace.gradient[**i];
                crate::vector::norm(g.limited[k]) < (1.0 - 1e-9) * crate::vector::norm(g.regular[k])
            })
            .count()
    });
    let sensor_maximum = selected
        .iter()
        .map(|i| workspace.shock_sensor[*i])
        .fold(0.0, f64::max);
    let pressure_limiting = selected
        .iter()
        .filter_map(|i| {
            let g = workspace.gradient[*i];
            let fraction = crate::vector::norm(g.limited[4]) / crate::vector::norm(g.regular[4]);
            (fraction < 1.0 - 1e-9).then_some((*i, op.mesh.cells[*i].center, g.raw[4], fraction))
        })
        .take(3)
        .collect();
    let relative = std::array::from_fn(|k| {
        let component = [0, 1, 2, 4][k];
        let mut error = 0.0;
        let mut reference = 0.0;
        for j in 0..2 * grid.angular_cells / 3 {
            for i in n / 3..2 * n / 3 {
                let cell = j * n + i;
                error += (workspace.residual[cell][component] - data[cell].1[component]).powi(2)
                    * op.mesh.cells[cell].volume;
                reference += data[cell].1[component].powi(2) * op.mesh.cells[cell].volume;
            }
        }
        (error / reference).sqrt()
    });
    CoupledError {
        relative,
        limited_cells,
        sensor_maximum,
        pressure_limiting,
    }
}

#[test]
fn coupled_axial_and_radial_fields_converge_on_shaped_anisotropic_grids() {
    // Mixed truncation terms cancel on coarse isotropic cells. Keep the same
    // componentwise order requirement on a finer sequence with resolved curvature.
    let measurements = [
        [96, 192, 384].map(|n| coupled_errors(n, 1)),
        [24, 48, 96].map(|n| coupled_errors(n, 4)),
    ];
    for (aspect, checks) in [1, 4].into_iter().zip(&measurements) {
        let errors = checks.each_ref().map(|c| c.relative);
        assert!(
            (0..4).all(|k| errors[0][k] / errors[1][k] > 3.0
                && errors[1][k] / errors[2][k] > 3.0
                && errors[2][k] < 1e-3),
            "aspect={aspect}, conservative mean PDE measurements={measurements:?}; limited={:?}; pressure={:?}",
            checks.each_ref().map(|c| c.limited_cells),
            checks.each_ref().map(|c| &c.pressure_limiting)
        );
        assert!(
            checks.iter().all(|c| c.sensor_maximum == 0.0),
            "smooth field triggered shock dissipation: {checks:?}"
        );
    }
}

#[test]
fn transverse_velocity_recovers_the_volume_mean_with_variable_density() {
    let errors = [24, 48, 96].map(|n| {
        let (op, grid) = super::super::tests::strain_operator(n, Convection::Ausmdv);
        let samples: Vec<_> = (0..n * n)
            .map(|k| {
                super::cell_mean(grid, k % n, k / n, |x, y| {
                    let velocity = y * (10000.0 + 1e5 * x);
                    (
                        op.gas.conservative(Primitive {
                            density: 0.01 * (1.0 + 20.0 * x),
                            velocity: [0.0, velocity, 0.0],
                            pressure: 1000.0,
                        }),
                        [0.0, 0.0, velocity, 0.0, 0.0],
                    )
                })
            })
            .collect();
        let state: Vec<_> = samples.iter().map(|s| s.0).collect();
        let cells = op.cell_diagnostics(&state).unwrap();
        (n / 3..2 * n / 3)
            .map(|i| (cells[i].primitive.velocity[1] / samples[i].1[2] - 1.0).abs())
            .fold(0.0_f64, f64::max)
    });
    assert!(
        errors[0] / errors[1] > 12.0 && errors[1] / errors[2] > 12.0 && errors[2] < 1e-9,
        "density-weighted transverse velocity recovery: {errors:?}"
    );
}

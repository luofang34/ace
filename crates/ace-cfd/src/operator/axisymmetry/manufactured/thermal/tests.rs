use crate::{FlowOperator, IdealGas, Mesh, Primitive, SphereGrid};

#[test]
fn temperature_recovers_the_volume_mean_from_density_weighted_internal_energy() {
    let errors = [24, 48, 96].map(|n| {
        let (op, grid) = super::super::super::tests::strain_operator(n, crate::Convection::Ausmdv);
        let state: Vec<_> = (0..n * n)
            .map(|k| {
                super::super::cell_mean(grid, k % n, k / n, |x, _y| {
                    let density = 0.01 * (1.0 + 20.0 * x);
                    let temperature = 300.0 + 1e4 * x;
                    (
                        op.gas.conservative(Primitive {
                            density,
                            pressure: density * op.gas.gas_constant * temperature,
                            velocity: [0.0; 3],
                        }),
                        [0.0; 5],
                    )
                })
                .0
            })
            .collect();
        let cells = op.cell_diagnostics(&state).unwrap();
        (n / 3..2 * n / 3)
            .map(|i| (cells[i].temperature_mean / (300.0 + 1e4 * cells[i].center[0]) - 1.0).abs())
            .fold(0.0_f64, f64::max)
    });
    assert!(
        errors[0] / errors[1] > 12.0 && errors[1] / errors[2] > 12.0 && errors[2] < 1e-9,
        "density-weighted temperature recovery: {errors:?}"
    );
}

#[test]
fn quadratic_wall_temperature_has_zero_heat_and_convergent_boundary_balance() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025116875,
    };
    let curvature = 2e8;
    let inflow = Primitive {
        density: 0.01,
        pressure: 0.01 * gas.gas_constant * 300.0,
        velocity: [0.0; 3],
    };
    let errors = [48, 96, 192].map(|n| {
        let grid = SphereGrid {
            radius: 0.01,
            outer_radius: 0.019047715851066883,
            radial_cells: n,
            angular_cells: 24,
            stretching: 2.2753462770828685,
            shock_shape_mach: None,
        };
        let mut op =
            FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0).unwrap();
        let samples: Vec<_> = (0..n * grid.angular_cells)
            .map(|k| {
                super::super::cell_mean(grid, k % n, k / n, |x, y| {
                    let radius = x.hypot(y);
                    let distance = radius - grid.radius;
                    let temperature = 300.0 + curvature * distance * distance;
                    let mut source = [0.0; 5];
                    source[4] = gas.conductivity * curvature * (2.0 + 4.0 * distance / radius);
                    (
                        gas.conservative(Primitive {
                            pressure: inflow.density * gas.gas_constant * temperature,
                            ..inflow
                        }),
                        source,
                    )
                })
            })
            .collect();
        let state: Vec<_> = samples.iter().map(|sample| sample.0).collect();
        let wall = op.wall_samples(&state).unwrap();
        assert!(wall.iter().all(|w| w.heat_flux.abs() < 1e-6));
        let original = op.residual(&state, true).unwrap();
        op.gas.conductivity *= 2.0;
        let doubled = op.residual(&state, true).unwrap();
        let mut maximum = 0.0_f64;
        let mut integral = 0.0;
        for j in 0..grid.angular_cells {
            let i = j * n;
            let error = (doubled[i][4] - original[i][4] - samples[i].1[4]).abs();
            maximum = maximum.max(error / samples[i].1[4]);
            integral += error * op.mesh.cells[i].volume;
        }
        [maximum, integral]
    });
    // Two-point diffusion has a first-order local boundary truncation on a
    // smoothly stretched grid. Its shrinking wall-cell volume gives a
    // second-order integrated defect; exact wall heat alone does not test this.
    for level in 0..2 {
        assert!(errors[level][0] / errors[level + 1][0] > 1.9, "{errors:?}");
        assert!(errors[level][1] / errors[level + 1][1] > 3.8, "{errors:?}");
    }
    assert!(errors[2][0] < 0.005, "{errors:?}");
}

#[test]
fn resolved_thermal_waves_have_second_order_conduction_at_the_axis() {
    let frequency: f64 = 1000.0;
    let amplitude = 100.0;
    let errors = [48, 96, 192].map(|n| {
        let (mut op, grid) =
            super::super::super::tests::strain_operator(n, crate::Convection::Ausmdv);
        let samples: Vec<_> = (0..n * n)
            .map(|i| {
                super::super::cell_mean(grid, i % n, i / n, |x, y| {
                    let density = 0.01 * (1.0 + 0.2 * (frequency * x).cos());
                    let temperature =
                        500.0 + amplitude * ((frequency * x).cos() + (frequency * y).cos());
                    let mut source = [0.0; 5];
                    source[4] = -op.gas.conductivity
                        * amplitude
                        * (frequency.powi(2) * ((frequency * x).cos() + (frequency * y).cos())
                            + frequency * (frequency * y).sin() / y);
                    (
                        op.gas.conservative(Primitive {
                            density,
                            pressure: density * op.gas.gas_constant * temperature,
                            velocity: [0.0; 3],
                        }),
                        source,
                    )
                })
            })
            .collect();
        let state: Vec<_> = samples.iter().map(|sample| sample.0).collect();
        let conductivity = op.gas.conductivity;
        let residuals = [1.0, 2.0, 3.0].map(|multiple| {
            op.gas.conductivity = multiple * conductivity;
            op.residual(&state, true).unwrap()
        });
        let scale = conductivity * amplitude * frequency.powi(2);
        let mut maximum = 0.0_f64;
        for j in 0..n / 12 {
            for i in n / 3..2 * n / 3 {
                let cell = j * n + i;
                let first = residuals[1][cell][4] - residuals[0][cell][4];
                let second = residuals[2][cell][4] - residuals[1][cell][4];
                assert!((first - second).abs() / scale < 1e-9);
                maximum = maximum.max((first - samples[cell].1[4]).abs() / scale);
            }
        }
        maximum
    });
    // The same conservative means drive convection and diffusion. Varying only
    // conductivity isolates Fourier heat transport without assuming that this
    // manufactured field satisfies the source-free momentum equations.
    assert!(
        errors[0] / errors[1] > 3.5 && errors[1] / errors[2] > 3.5 && errors[2] < 0.005,
        "interior thermal diffusion errors: {errors:?}"
    );
}

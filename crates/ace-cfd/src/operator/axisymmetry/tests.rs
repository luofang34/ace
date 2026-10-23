use super::*;
use crate::{Conserved, Convection, Mesh, SphereGrid};

pub(super) fn strain_operator(n: usize, convection: Convection) -> (FlowOperator, SphereGrid) {
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
    let inflow = Primitive {
        density: 0.01,
        velocity: [0.0; 3],
        pressure: 1000.0,
    };
    let op = FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0)
        .unwrap()
        .with_convection(convection);
    (op, grid)
}

fn squared_coordinates(grid: SphereGrid, i: usize, j: usize) -> [f64; 2] {
    let a = crate::vector::norm(grid.node(i, j));
    let b = crate::vector::norm(grid.node(i + 1, j));
    let radial = 3.0 * (b.powi(5) - a.powi(5)) / (5.0 * (b.powi(3) - a.powi(3)));
    let c = (std::f64::consts::FRAC_PI_2 * j as f64 / grid.angular_cells as f64).cos();
    let d = (std::f64::consts::FRAC_PI_2 * (j + 1) as f64 / grid.angular_cells as f64).cos();
    let cosine_squared = (c * c + c * d + d * d) / 3.0;
    [radial * cosine_squared, radial * (1.0 - cosine_squared)]
}

fn strain_errors(convection: Convection) -> [f64; 3] {
    [24, 48, 96].map(|n| {
        let (op, grid) = strain_operator(n, convection);
        let a: f64 = 10000.0;
        let rho = op.inflow.density;
        let state: Vec<Conserved> = op
            .mesh
            .cells
            .iter()
            .enumerate()
            .map(|(k, cell)| {
                let [xx, yy] = squared_coordinates(grid, k % n, k / n);
                [
                    rho,
                    -2.0 * rho * a * cell.center[0],
                    rho * a * cell.center[1],
                    0.0,
                    op.inflow.pressure / (op.gas.gamma - 1.0) + 0.5 * rho * a * a * (4.0 * xx + yy),
                ]
            })
            .collect();
        let residual = op.residual(&state, true).unwrap();
        (n / 3..2 * n / 3)
            .map(|i| {
                let exact = -rho * a * a * op.mesh.cells[i].center[1];
                (residual[i][2] / exact - 1.0).abs()
            })
            .fold(0.0_f64, f64::max)
    })
}

#[test]
fn conservative_mean_strain_preserves_the_axis_momentum_balance() {
    for convection in [Convection::HybridHllc, Convection::Ausmdv] {
        let errors = strain_errors(convection);
        // An affine velocity has an exact quadratic kinetic-energy mean.
        // The remaining error is cancellation of the much larger pressure forces.
        assert!(
            errors.iter().all(|e| *e < 1e-8),
            "{convection:?}, errors={errors:?}"
        );
    }
}

#[test]
fn conservative_temperature_curvature_has_the_correct_axis_laplacian() {
    let errors = [24, 48, 96].map(|n| {
        let (mut op, grid) = strain_operator(n, Convection::Ausmdv);
        let curvature = 1e6;
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .enumerate()
            .map(|(k, _)| {
                let [_, yy] = squared_coordinates(grid, k % n, k / n);
                op.gas.conservative(Primitive {
                    pressure: op.inflow.density * op.gas.gas_constant * (300.0 + curvature * yy),
                    ..op.inflow
                })
            })
            .collect();
        let first = op.residual(&state, true).unwrap();
        let exact = 4.0 * curvature * op.gas.conductivity;
        op.gas.conductivity *= 2.0;
        let second = op.residual(&state, true).unwrap();
        (n / 3..2 * n / 3)
            .map(|i| ((second[i][4] - first[i][4]) / exact - 1.0).abs())
            .fold(0.0_f64, f64::max)
    });
    assert!(
        errors.iter().all(|e| *e < 1e-9),
        "mean temperature curvature: {errors:?}"
    );
}

#[test]
fn conservative_pressure_curvature_preserves_the_axis_force() {
    let errors = [24, 48, 96].map(|n| {
        let (op, grid) = strain_operator(n, Convection::Ausmdv);
        let curvature = 1e6;
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .enumerate()
            .map(|(k, _)| {
                let [_, yy] = squared_coordinates(grid, k % n, k / n);
                op.gas.conservative(Primitive {
                    pressure: 1000.0 + curvature * yy,
                    ..op.inflow
                })
            })
            .collect();
        let residual = op.residual(&state, true).unwrap();
        (n / 3..2 * n / 3)
            .map(|i| {
                let exact = -2.0 * curvature * op.mesh.cells[i].center[1];
                (residual[i][2] / exact - 1.0).abs()
            })
            .fold(0.0_f64, f64::max)
    });
    assert!(
        errors.iter().all(|e| *e < 1e-8),
        "mean pressure force: {errors:?}"
    );
}

#[test]
fn curved_reconstruction_keeps_every_quadrature_state_positive() {
    let (op, _) = strain_operator(8, Convection::Ausmdv);
    for slope in [200.0, 1000.0, 2000.0] {
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
            for point in face.integration_points() {
                for cell in [Some(face.owner), face.neighbor].into_iter().flatten() {
                    let q = op.face_state(&face.at_point(point), cell, true, &workspace);
                    assert!(q.density.is_finite() && q.density > 0.0);
                    assert!(q.pressure.is_finite() && q.pressure > 0.0);
                }
            }
        }
        assert!(
            op.residual(&state, true)
                .unwrap()
                .iter()
                .flatten()
                .all(|v| v.is_finite())
        );
    }
}

fn smooth_flow(gas: IdealGas, y: f64, quartic_density: f64) -> (Conserved, Conserved) {
    let (rho0, b, c, speed, strain, temperature) = (0.01, 2000.0, 1000.0, 100.0, 1000.0, 300.0);
    let yy = y * y;
    let rho = rho0 * (1.0 + b * yy + quartic_density * yy * yy);
    let drho = rho0 * (2.0 * b * y + 4.0 * quartic_density * yy * y);
    let velocity = speed * (1.0 + c * yy);
    let dvelocity = 2.0 * speed * c * y;
    let state = gas.conservative(Primitive {
        density: rho,
        velocity: [velocity, strain * y, 0.0],
        pressure: rho * gas.gas_constant * temperature,
    });
    let enthalpy = gas.cp() * temperature + 0.5 * (velocity * velocity + strain * strain * yy);
    let enthalpy_derivative = velocity * dvelocity + strain * strain * y;
    let residual = [
        -strain * (2.0 * rho + y * drho),
        -strain * (2.0 * rho * velocity + y * (drho * velocity + rho * dvelocity))
            + 4.0 * gas.viscosity * speed * c,
        -strain * strain * y * (3.0 * rho + drho * y) - gas.gas_constant * temperature * drho,
        0.0,
        -strain * (2.0 * rho * enthalpy + y * (drho * enthalpy + rho * enthalpy_derivative))
            + 4.0 * gas.viscosity * speed * speed * c * (1.0 + 2.0 * c * yy)
            + 4.0 / 3.0 * gas.viscosity * strain * strain,
    ];
    (state, residual)
}

fn smooth_cell_mean(
    gas: IdealGas,
    grid: SphereGrid,
    i: usize,
    j: usize,
    quartic_density: f64,
) -> (Conserved, Conserved) {
    super::manufactured::cell_mean(grid, i, j, |_x, y| smooth_flow(gas, y, quartic_density))
}

fn nonlinear_errors(quartic_density: f64) -> [[f64; 4]; 3] {
    [24, 48, 96].map(|n| {
        let (op, grid) = strain_operator(n, Convection::Ausmdv);
        let data: Vec<_> = (0..n * n)
            .map(|k| smooth_cell_mean(op.gas, grid, k % n, k / n, quartic_density))
            .collect();
        let state: Vec<_> = data.iter().map(|(u, _)| *u).collect();
        let actual = op.residual(&state, true).unwrap();
        std::array::from_fn::<_, 4, _>(|k| {
            let component = [0, 1, 2, 4][k];
            (n / 3..2 * n / 3)
                .map(|i| (actual[i][component] / data[i].1[component] - 1.0).abs())
                .fold(0.0_f64, f64::max)
        })
    })
}

#[test]
fn variable_density_momentum_and_energy_converge_at_the_axis() {
    let errors = nonlinear_errors(1e8);
    assert!(
        (0..4).all(|k| errors[0][k] / errors[1][k] > 3.0 && errors[1][k] / errors[2][k] > 3.0),
        "nonlinear conservative-mean residual errors: {errors:?}"
    );
    let quadratic = nonlinear_errors(0.0);
    assert!(
        quadratic[2].iter().all(|e| *e < 1e-6),
        "density-weighted axis errors: {quadratic:?}"
    );
}

#[test]
fn conservative_axial_temperature_curvature_converges_at_the_axis() {
    for shape in [None, Some(3.0)] {
        let errors = [24, 48, 96].map(|n| {
            let (mut op, mut grid) = strain_operator(n, Convection::Ausmdv);
            grid.shock_shape_mach = shape;
            op.mesh = Mesh::curved_sphere(grid).unwrap();
            let curvature = 1e6;
            let state: Vec<_> = (0..n * n)
                .map(|k| {
                    super::manufactured::cell_mean(grid, k % n, k / n, |x, _y| {
                        let q = Primitive {
                            pressure: op.inflow.density
                                * op.gas.gas_constant
                                * (300.0 + curvature * x * x),
                            ..op.inflow
                        };
                        (op.gas.conservative(q), [0.0; 5])
                    })
                    .0
                })
                .collect();
            let first = op.residual(&state, true).unwrap();
            let exact = 2.0 * curvature * op.gas.conductivity;
            op.gas.conductivity *= 2.0;
            let second = op.residual(&state, true).unwrap();
            (n / 3..2 * n / 3)
                .map(|i| ((second[i][4] - first[i][4]) / exact - 1.0).abs())
                .fold(0.0_f64, f64::max)
        });
        assert!(
            errors[0] / errors[1] > 3.0 && errors[1] / errors[2] > 3.0 && errors[2] < 1e-3,
            "conservative axial temperature shape={shape:?}: {errors:?}"
        );
    }
}

fn wall_axis_trace_gap(angular_cells: usize) -> f64 {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.02,
        radial_cells: 24,
        angular_cells,
        stretching: 2.0,
        shock_shape_mach: None,
    };
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let rho = 0.01;
    let pressure = rho * gas.gas_constant * 300.0;
    let inflow = Primitive {
        density: rho,
        pressure,
        velocity: [0.0; 3],
    };
    let op = FlowOperator::new(Mesh::curved_sphere(grid).unwrap(), gas, inflow, 300.0).unwrap();
    let state: Vec<_> = (0..grid.radial_cells * angular_cells)
        .map(|i| {
            super::manufactured::cell_mean(
                grid,
                i % grid.radial_cells,
                i / grid.radial_cells,
                |x, y| {
                    let radius = x.hypot(y);
                    let distance = radius - grid.radius;
                    // This streamfunction field has zero divergence and vanishes on
                    // the sphere; its normal component is quadratic in wall distance.
                    let velocity = [
                        200000.0 * (distance * distance + distance * y * y / radius),
                        -200000.0 * distance * x * y / radius,
                        0.0,
                    ];
                    (gas.conservative(Primitive { velocity, ..inflow }), [0.0; 5])
                },
            )
            .0
        })
        .collect();
    let mut workspace = op.workspace();
    op.prepare(&state, &mut workspace).unwrap();
    let mut sums = [[0.0; 2]; 2];
    for wall in op.mesh.faces.iter().filter(|f| {
        f.owner < 2 * grid.radial_cells
            && matches!(f.boundary, Boundary::Wall)
            && f.neighbor.is_none()
    }) {
        let row = wall.owner / grid.radial_cells;
        for point in wall.integration_points() {
            let face = wall.at_point(point);
            let q = op.face_state(&face, wall.owner, true, &workspace);
            sums[row][0] += point.area * dot(q.velocity, face.normal);
            sums[row][1] += point.area;
        }
    }
    (sums[1][0] / sums[1][1] - sums[0][0] / sums[0][1]).abs()
}

#[test]
fn wall_velocity_limiter_preserves_axis_trace_regularity() {
    let gaps = [48, 96, 192].map(wall_axis_trace_gap);
    // At fixed radial resolution, a smooth even trace has an angular difference
    // proportional to theta². An axis-only range exemption leaves a finite jump.
    assert!(
        gaps[0] / gaps[1] > 3.0 && gaps[1] / gaps[2] > 3.0,
        "axis trace gaps={gaps:?}"
    );
}

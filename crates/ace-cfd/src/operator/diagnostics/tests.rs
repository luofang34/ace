use super::*;
use crate::{IdealGas, Mesh, SphereGrid};

#[test]
fn wall_loads_export_their_angular_sampling_moment() {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.025,
        radial_cells: 4,
        angular_cells: 12,
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
        density: 1000.0 / (gas.gas_constant * 300.0),
        velocity: [0.0; 3],
        pressure: 1000.0,
    };
    for curved in [false, true] {
        let mesh = if curved {
            Mesh::curved_sphere(grid).unwrap()
        } else {
            Mesh::sphere(grid).unwrap()
        };
        let op = FlowOperator::new(mesh, gas, inflow, 300.0).unwrap();
        let samples = op.wall_samples(&op.initial_state()).unwrap();
        for (i, sample) in samples.iter().enumerate() {
            let expected = if curved {
                // Exact surface integral of theta² sin(theta) on a spherical band.
                let integral = |t: f64| -t * t * t.cos() + 2.0 * t * t.sin() + 2.0 * t.cos();
                let a = i as f64 * std::f64::consts::FRAC_PI_2 / 12.0;
                let b = (i + 1) as f64 * std::f64::consts::FRAC_PI_2 / 12.0;
                (integral(b) - integral(a)) / (a.cos() - b.cos())
            } else {
                sample.theta.powi(2)
            };
            assert!((sample.theta_squared_mean - expected).abs() < 1e-11);
        }
    }
}

#[test]
fn exported_quadrature_fluxes_reproduce_the_cell_residuals() {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.025,
        radial_cells: 8,
        angular_cells: 8,
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
        density: 0.0035,
        velocity: [1040.0, 0.0, 0.0],
        pressure: 300.0,
    };
    for curved in [false, true] {
        let vertices = grid.vertices().unwrap();
        let mesh = if curved {
            Mesh::curved_from_vertices(grid, &vertices).unwrap()
        } else {
            Mesh::from_vertices(grid, &vertices).unwrap()
        };
        let op = FlowOperator::new(mesh, gas, inflow, 300.0).unwrap();
        let state: Vec<_> = op
            .mesh
            .cells
            .iter()
            .map(|cell| {
                gas.conservative(Primitive {
                    density: inflow.density * (1.0 + 10.0 * cell.center[1]),
                    velocity: [
                        200.0 + 1000.0 * cell.center[0],
                        1000.0 * cell.center[1],
                        0.0,
                    ],
                    pressure: 300.0 + 1000.0 * cell.center[0],
                })
            })
            .collect();
        let residual = op.residual(&state, true).unwrap();
        let mut traced = vec![[0.0; 5]; state.len()];
        for face in op.face_diagnostics(&state).unwrap() {
            for (k, value) in face.total_flux.into_iter().enumerate() {
                traced[face.owner][k] -= value * face.area;
                if let Some(n) = face.neighbor {
                    traced[n][k] += value * face.area;
                }
            }
        }
        for (i, cell) in op.mesh.cells.iter().enumerate() {
            // These equations have no cylindrical volume source.
            for k in [0, 1, 4] {
                let actual = residual[i][k] * cell.volume;
                assert!((traced[i][k] - actual).abs() < 1e-12 * actual.abs().max(1.0));
            }
        }
    }
}

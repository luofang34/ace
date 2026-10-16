use super::*;
use crate::{FlowOperator, IdealGas, Primitive};

fn grid(nr: usize, nt: usize) -> SphereGrid {
    SphereGrid {
        radius: 0.01,
        outer_radius: 0.02,
        radial_cells: nr,
        angular_cells: nt,
        stretching: 2.0,
        shock_shape_mach: None,
    }
}

#[test]
fn curved_metrics_integrate_the_sphere_and_balance_pressure() {
    for (nr, nt) in [(4, 8), (16, 24), (192, 12)] {
        let mesh = Mesh::curved_sphere(grid(nr, nt)).unwrap();
        let volume: f64 = mesh.cells.iter().map(|c| c.volume).sum();
        let exact_volume = (0.02_f64.powi(3) - 0.01_f64.powi(3)) / 3.0;
        assert!((volume / exact_volume - 1.0).abs() < 1e-12);
        let area: f64 = mesh
            .faces
            .iter()
            .filter(|f| f.neighbor.is_none() && matches!(f.boundary, super::super::Boundary::Wall))
            .map(|f| f.area)
            .sum();
        assert!((area / 0.01_f64.powi(2) - 1.0).abs() < 1e-12);
        for (i, cell) in mesh.cells.iter().enumerate() {
            let mut sum = [0.0; 3];
            for id in cell.faces {
                let face = &mesh.faces[id];
                let sign = if face.owner == i { 1.0 } else { -1.0 };
                for point in face.integration_points() {
                    sum = add(sum, scale(point.normal, sign * point.area));
                }
            }
            assert!(sum[0].abs() / cell.planar_area < 1e-11);
            assert!((sum[1] / cell.planar_area - 1.0).abs() < 1e-11);
        }
    }
}

#[test]
fn curved_operator_preserves_rest_and_conserves_energy_on_a_shaped_grid() {
    let mut geometry = grid(16, 24);
    geometry.shock_shape_mach = Some(3.0);
    let mesh = Mesh::curved_sphere(geometry).unwrap();
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let inflow = Primitive {
        density: 1000.0 / (287.05 * 300.0),
        pressure: 1000.0,
        velocity: [0.0; 3],
    };
    let op = FlowOperator::new(mesh, gas, inflow, 300.0).unwrap();
    let rest = op.initial_state();
    assert!(
        op.residual(&rest, true)
            .unwrap()
            .iter()
            .flatten()
            .all(|r| r.abs() < 1e-5)
    );
    let state = op
        .mesh
        .cells
        .iter()
        .map(|c| {
            gas.conservative(Primitive {
                velocity: [100.0, 100.0 * c.center[1], 0.0],
                density: inflow.density * (1.0 + 10.0 * c.center[1]),
                ..inflow
            })
        })
        .collect::<Vec<_>>();
    let budget = op.energy_balance(&state).unwrap();
    let power = budget.wall_power.abs() + budget.inflow_power.abs() + budget.outflow_power.abs();
    assert!(budget.closure_error.abs() / power < 1e-12);
    let reported: f64 = op
        .wall_samples(&state)
        .unwrap()
        .iter()
        .map(|w| w.heat_flux * w.area)
        .sum();
    assert!((reported - budget.wall_power).abs() / power < 1e-12);
}

#[test]
fn nonpolar_vertices_are_rejected_by_the_curved_constructor() {
    let geometry = grid(8, 8);
    let mut vertices = geometry.vertices().unwrap();
    vertices[4 * 9 + 3][0] += 1e-5;
    assert!(Mesh::from_vertices(geometry, &vertices).is_ok());
    assert!(Mesh::curved_from_vertices(geometry, &vertices).is_err());
}

fn radial_temperature_average(lower: f64, upper: f64) -> f64 {
    // Integrate density, not point temperature, to provide conservative cell data.
    let step = (upper - lower) / 128.0;
    let integral: f64 = (0..=128)
        .map(|i| {
            let radius = lower + i as f64 * step;
            let temperature = 300.0 + 10000.0 * (radius - 0.01);
            let weight = if i == 0 || i == 128 {
                1.0
            } else if i % 2 == 0 {
                2.0
            } else {
                4.0
            };
            weight * radius * radius / temperature
        })
        .sum::<f64>()
        * step
        / 3.0;
    let volume = (upper - lower) * (upper * upper + upper * lower + lower * lower) / 3.0;
    volume / integral
}

#[test]
fn spherical_wall_heat_uses_cell_moments_without_an_angular_over_normal_error() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let inflow = Primitive {
        density: 1000.0 / (287.05 * 300.0),
        pressure: 1000.0,
        velocity: [0.0; 3],
    };
    let mut errors = Vec::new();
    for nr in [48, 96, 192] {
        let mut angular_errors = Vec::new();
        for nt in [12, 24, 48] {
            let geometry = grid(nr, nt);
            let vertices = geometry.vertices().unwrap();
            let op = FlowOperator::new(Mesh::curved_sphere(geometry).unwrap(), gas, inflow, 300.0)
                .unwrap();
            let state = (0..nr * nt)
                .map(|i| {
                    let k = i % nr;
                    let temperature =
                        radial_temperature_average(norm(vertices[k]), norm(vertices[k + 1]));
                    gas.conservative(Primitive {
                        density: inflow.pressure / (gas.gas_constant * temperature),
                        ..inflow
                    })
                })
                .collect::<Vec<_>>();
            let heat = op.wall_samples(&state).unwrap()[0].heat_flux;
            angular_errors.push((heat / (10000.0 * gas.conductivity) - 1.0).abs());
        }
        assert!(
            angular_errors.iter().all(|e| *e < 1e-4),
            "nr={nr}, errors={angular_errors:?}"
        );
        assert!(
            (angular_errors[0] - angular_errors[2]).abs() < 1e-9,
            "nr={nr}, errors={angular_errors:?}"
        );
        errors.push(angular_errors[2]);
    }
    assert!(
        errors[0] / errors[1] > 3.8 && errors[1] / errors[2] > 3.8,
        "radial errors={errors:?}"
    );
}

#[test]
fn conservative_wall_fit_separates_heat_from_unresolved_kinetic_energy() {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let inflow = Primitive {
        density: 0.01,
        pressure: 0.01 * 287.05 * 300.0,
        velocity: [0.0; 3],
    };
    for (nr, nt) in [(24, 12), (96, 12), (96, 48)] {
        let mesh = Mesh::curved_sphere(grid(nr, nt)).unwrap();
        let op = FlowOperator::new(mesh, gas, inflow, 300.0).unwrap();
        for strain in [0.0, 1e5] {
            let state = op
                .mesh
                .cells
                .iter()
                .map(|cell| {
                    let [m1, m2] = cell.radial_moments.unwrap().scalar;
                    let rho = inflow.density;
                    [
                        rho,
                        rho * strain * m1,
                        0.0,
                        0.0,
                        rho * (gas.cp() / gas.gamma * (300.0 + 10000.0 * m1)
                            + 0.5 * strain * strain * m2),
                    ]
                })
                .collect::<Vec<_>>();
            let heat = op.wall_samples(&state).unwrap()[0].heat_flux;
            assert!(
                (heat / (10000.0 * gas.conductivity) - 1.0).abs() < 1e-9,
                "nr={nr}, nt={nt}, strain={strain}, heat={heat}"
            );
        }
    }
}

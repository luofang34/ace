use super::*;
use crate::{Convection, IdealGas, Mesh, Primitive, SphereGrid};

#[test]
fn wall_postprocessing_and_conservation_match_with_flow_and_conduction() {
    let grid = SphereGrid {
        radius: 0.01,
        outer_radius: 0.025,
        radial_cells: 12,
        angular_cells: 12,
        stretching: 3.0,
        shock_shape_mach: Some(3.0),
    };
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-4,
        conductivity: 0.25,
    };
    let inflow = Primitive {
        density: 0.0035,
        velocity: [1040.0, 0.0, 0.0],
        pressure: 300.0,
    };
    let op = FlowOperator::new(Mesh::sphere(grid).unwrap(), gas, inflow, 300.0)
        .unwrap()
        .with_convection(Convection::Ausmdv);
    let state: Vec<_> = op
        .mesh
        .cells
        .iter()
        .map(|cell| {
            let distance = crate::vector::norm(cell.center) - grid.radius;
            gas.conservative(Primitive {
                density: 3500.0 / (gas.gas_constant * (300.0 + 20000.0 * distance)),
                velocity: [10000.0 * distance, 2000.0 * cell.center[1], 0.0],
                pressure: 3500.0,
            })
        })
        .collect();
    let balance = op.energy_balance(&state).unwrap();
    let reported: f64 = op
        .wall_samples(&state)
        .unwrap()
        .iter()
        .map(|w| w.area * w.heat_flux)
        .sum();
    assert!(reported > 1.0);
    assert!((balance.wall_power - reported).abs() < 1.0e-10);
    assert!(balance.closure_error.abs() < 1.0e-9);
    assert_eq!(balance.axis_power, 0.0);
    assert!(balance.absolute_volume_energy_rate >= balance.volume_energy_rate.abs());
}

use super::*;
use crate::{IdealGas, Mesh, Primitive, SphereGrid};

fn case(velocity: f64) -> (FlowOperator, SolverConfig) {
    let gas = IdealGas {
        gamma: 1.4,
        gas_constant: 287.05,
        viscosity: 1.8e-5,
        conductivity: 0.025,
    };
    let mesh = Mesh::sphere(SphereGrid {
        radius: 0.01,
        outer_radius: 0.025,
        radial_cells: 8,
        angular_cells: 8,
        stretching: 3.0,
        shock_shape_mach: None,
    })
    .unwrap();
    let q = Primitive {
        density: 300.0 / (287.05 * 300.0),
        velocity: [velocity, 0.0, 0.0],
        pressure: 300.0,
    };
    let config = SolverConfig {
        cfl: 0.1,
        max_steps: 5,
        first_order_steps: 0,
        check_interval: 1,
        residual_tolerance: 1.0e-6,
        heat_flux_tolerance: 1.0e-5,
    };
    (FlowOperator::new(mesh, gas, q, 300.0).unwrap(), config)
}

#[test]
fn iteration_limit_is_not_reported_as_convergence() {
    let (op, config) = case(1000.0);
    let result = solve(&op, config, None).unwrap();
    assert!(!result.converged);
    assert_eq!(result.steps, 5);
    assert!(result.wall.iter().any(|w| w.heat_flux > 0.0));
    let reported = result.history.last().unwrap().integrated_heating;
    let wall_power = op.energy_balance(&result.state).unwrap().wall_power;
    assert!(wall_power > 0.0);
    assert!((reported - wall_power).abs() < 1.0e-10);
}

#[test]
fn equilibrium_requires_three_consecutive_checks() {
    let (op, config) = case(0.0);
    let mut observed = Vec::new();
    let result = solve_observed(&op, config, None, |check| observed.push(check.step)).unwrap();
    assert!(result.converged);
    assert_eq!(observed, vec![1, 2, 3]);
}

#[test]
fn invalid_initial_state_and_controls_are_rejected() {
    let (op, config) = case(1000.0);
    assert!(solve(&op, config, Some(vec![[0.0; 5]; 64])).is_err());
    assert!(
        solve(
            &op,
            SolverConfig {
                cfl: f64::NAN,
                ..config
            },
            None
        )
        .is_err()
    );
    assert!(solve(&op, config, Some(vec![])).is_err());
}

#[test]
fn checkpoint_observation_preserves_the_accepted_trajectory() {
    let (op, config) = case(1000.0);
    let expected = solve(&op, config, None).unwrap();
    let mut snapshots = Vec::new();
    let actual = solve_checkpointed(&op, config, None, |sample, state| {
        snapshots.push((sample.step, state.to_vec()));
        Ok::<_, std::convert::Infallible>(())
    })
    .unwrap();
    assert_eq!(actual.state, expected.state);
    assert_eq!(actual.steps, expected.steps);
    assert_eq!(snapshots.last().unwrap().1, actual.state);
    let (step, snapshot) = &snapshots[1];
    assert_eq!(*step, 2);
    let reference = solve(
        &op,
        SolverConfig {
            max_steps: 2,
            ..config
        },
        None,
    )
    .unwrap();
    assert_eq!(*snapshot, reference.state);
    let before = snapshot.clone();
    let diagnostics = op.cell_diagnostics(snapshot).unwrap();
    assert_eq!(*snapshot, before);
    assert_eq!(
        diagnostics.iter().map(|d| d.residual).collect::<Vec<_>>(),
        op.residual(snapshot, true).unwrap()
    );
}

#[test]
fn observer_failure_stops_at_the_failed_check_and_preserves_its_source() {
    let (op, config) = case(1000.0);
    let mut observed = Vec::new();
    let result = solve_checkpointed(&op, config, None, |sample, _| {
        observed.push(sample.step);
        if sample.step == 2 {
            Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "checkpoint sink",
            ))
        } else {
            Ok(())
        }
    });
    assert_eq!(observed, vec![1, 2]);
    match result {
        Err(ObservationError::Observer(source)) => {
            assert_eq!(source.kind(), std::io::ErrorKind::WriteZero)
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

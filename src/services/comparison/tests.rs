use std::path::PathBuf;

use crate::services::analysis::ApplicationService;
use crate::test_support::example_scenario;

use super::installed_loading;

#[test]
fn turbofan_comparison_loading_includes_sizing_factor() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let baseline = installed_loading(&scenario);
    scenario.aircraft.propulsion.sizing_factor = 1.5;
    let resized = installed_loading(&scenario);
    assert!((resized - 1.5 * baseline).abs() < 1.0e-12);
    Ok(())
}

#[test]
fn piston_comparison_loading_includes_sizing_factor() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let baseline = installed_loading(&scenario);
    scenario.aircraft.propulsion.sizing_factor = 1.2;
    let resized = installed_loading(&scenario);
    assert!((resized - 1.2 * baseline).abs() < 1.0e-12);
    Ok(())
}

#[test]
fn comparison_headline_metric_is_completion_gated() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let paths = ["c172", "x15"]
        .map(|name| root.join(name).join("scenario.yaml"))
        .to_vec();
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let comparison =
        service.compare_blocking(&paths, &["feasibility.hard_constraints_passed".to_owned()])?;

    assert_eq!(
        comparison.scenarios[0].metrics["feasibility.hard_constraints_passed"].value,
        1.0
    );
    assert_eq!(
        comparison.scenarios[1].metrics["feasibility.hard_constraints_passed"].value,
        0.0
    );
    Ok(())
}

#[test]
fn comparison_propagates_achieved_cruise_metrics_and_validity()
-> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let metrics = [
        "performance.achieved_cruise_true_airspeed".to_owned(),
        "performance.minimum_cruise_excess_power".to_owned(),
        "performance.cruise_feasible".to_owned(),
    ];
    let comparison = service.compare_blocking(&[path], &metrics)?;
    let row = &comparison.scenarios[0];

    assert!(row.metrics[&metrics[0]].value > 0.0);
    assert!(row.metrics[&metrics[1]].value > 0.0);
    assert_eq!(row.metrics[&metrics[2]].value, 1.0);
    for metric in metrics {
        assert!(row.metric_validity.contains_key(&metric));
    }
    Ok(())
}

use std::fs;
use std::path::{Path, PathBuf};

use crate::services::analysis::ApplicationService;
use crate::test_support::example_scenario;

use super::installed_loading;

fn fuel_exhaustion_fixture(destination: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172");
    fs::create_dir_all(destination.join("profiles"))?;
    for path in [
        "aircraft.yaml",
        "mission.yaml",
        "requirements.yaml",
        "scenario.yaml",
        "profiles/engine.yaml",
        "profiles/propeller.yaml",
    ] {
        fs::copy(source.join(path), destination.join(path))?;
    }
    let aircraft_path = destination.join("aircraft.yaml");
    let mut aircraft: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&aircraft_path)?)?;
    aircraft["aircraft"]["mass"]["maximum_fuel_mass"] =
        serde_yaml::Value::String("1 kg".to_owned());
    fs::write(aircraft_path, serde_yaml::to_string(&aircraft)?)?;
    Ok(destination.join("scenario.yaml"))
}

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
    let temporary = tempfile::tempdir()?;
    let paths = vec![
        root.join("c172/scenario.yaml"),
        fuel_exhaustion_fixture(&temporary.path().join("incomplete"))?,
    ];
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

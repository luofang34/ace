use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::services::analysis::ApplicationService;

use super::concept_decision;

fn unsupported_report_fixture(destination: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
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
    remove_horizontal_tail(&destination.join("aircraft.yaml"))?;
    remove_cruise_segment(&destination.join("mission.yaml"))?;
    Ok(destination.join("scenario.yaml"))
}

fn fuel_exhaustion_report_fixture(
    destination: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/x15");
    fs::create_dir_all(destination.join("profiles"))?;
    for path in [
        "aircraft.yaml",
        "mission.yaml",
        "requirements.yaml",
        "scenario.yaml",
        "profiles/xlr99.yaml",
    ] {
        fs::copy(source.join(path), destination.join(path))?;
    }
    retain_single_vertical_tail(&destination.join("aircraft.yaml"))?;
    Ok(destination.join("scenario.yaml"))
}

fn retain_single_vertical_tail(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    let components = document["aircraft"]["topology"]["components"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology components"))?;
    components.retain(|component| component["id"].as_str() != Some("lower_vertical_tail"));
    let relationships = document["aircraft"]["topology"]["relationships"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology relationships"))?;
    relationships.retain(|relationship| {
        relationship["source"].as_str() != Some("lower_vertical_tail")
            && relationship["target"].as_str() != Some("lower_vertical_tail")
    });
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn remove_horizontal_tail(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    let components = document["aircraft"]["topology"]["components"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology components"))?;
    components.retain(|component| component["kind"].as_str() != Some("horizontal_tail"));
    let relationships = document["aircraft"]["topology"]["relationships"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology relationships"))?;
    relationships.retain(|relationship| {
        relationship["source"].as_str() != Some("horizontal_tail")
            && relationship["target"].as_str() != Some("horizontal_tail")
    });
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn remove_cruise_segment(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    let segments = document["mission"]["segments"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing mission segments"))?;
    segments.retain(|segment| segment["type"].as_str() != Some("cruise"));
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

#[test]
fn report_stops_at_topology_preflight_before_downstream_analysis()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let scenario = unsupported_report_fixture(&temporary.path().join("source"))?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let downstream = service.payload_range_blocking(&scenario, &Default::default());
    assert!(downstream.is_err_and(|error| error.to_string().contains("NO_CRUISE_SEGMENT")));
    let result = service.concept_report_blocking(&scenario, "native");

    assert!(result.is_err_and(|error| error.to_string().contains("UNSUPPORTED_BACKEND_TOPOLOGY")));
    Ok(())
}

#[test]
fn report_preserves_fuel_exhaustion_as_a_distinct_constraint()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let scenario = fuel_exhaustion_report_fixture(&temporary.path().join("source"))?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let report = service.concept_report_blocking(&scenario, "native")?;

    assert!(report.mission.fuel_exhausted);
    assert!(!report.mission.fuel_capacity_violation);
    assert!(!report.decision.feasible);
    assert!(
        report
            .decision
            .failed_constraints
            .iter()
            .any(|constraint| constraint == "fuel_exhausted")
    );
    assert!(
        !report
            .decision
            .failed_constraints
            .iter()
            .any(|constraint| constraint == "fuel_capacity")
    );
    let analysis = &report.feasibility.completed()?.baseline.analysis;
    assert_eq!(analysis.feasible, Some(false));
    assert!(
        analysis
            .provenance
            .warnings
            .iter()
            .any(|warning| warning.code == "FUEL_EXHAUSTED")
    );
    Ok(())
}

#[test]
fn report_counts_and_fails_unevaluable_hard_requirements() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = tempfile::tempdir()?;
    let scenario_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let scenario = service.resolve_blocking(&scenario_path, &Default::default())?;
    let feasibility = service.evaluate_feasibility_blocking(&scenario_path, "native", None)?;
    let mut completed = feasibility.completed()?.clone();
    completed
        .baseline
        .analysis
        .requirements
        .retain(|requirement| requirement.id != "cruise_speed");
    let (_, mission) = service.mission_blocking(&scenario_path, &Default::default())?;
    let decision = concept_decision(&completed, &mission, &scenario);

    assert!(!decision.feasible);
    assert_eq!(decision.hard_requirements_passed, 3);
    assert_eq!(decision.hard_requirements_total, 4);
    Ok(())
}

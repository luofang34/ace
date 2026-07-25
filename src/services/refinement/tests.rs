use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use crate::services::analysis::ApplicationService;

use super::RefinementSpec;

fn copy_c172_without_horizontal_tail(
    destination: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
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
    let mut document: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&aircraft_path)?)?;
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
    fs::write(aircraft_path, serde_yaml::to_string(&document)?)?;
    Ok(destination.join("scenario.yaml"))
}

#[test]
fn c172_refinement_returns_a_screened_design() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let result = service.auto_refine_design_blocking(RefinementSpec {
        scenario_path: &scenario,
        output_design_id: "refined_c172",
        display_name: "Refined C172",
        design_root: temporary.path(),
        backend: "native",
        artifact_path: None,
        max_iterations: 12,
    })?;
    let completed = result.evaluation.completed()?;
    assert!(result.converged);
    assert!(completed.failed_constraints.is_empty());
    assert!(result.backend_verification_passed);
    let structural = completed
        .baseline
        .analysis
        .structural_screen
        .as_ref()
        .ok_or_else(|| std::io::Error::other("missing structural screen"))?;
    assert!(structural.passed);
    let power = completed
        .baseline
        .analysis
        .mission_power_screen
        .as_ref()
        .ok_or_else(|| std::io::Error::other("missing mission power screen"))?;
    assert!(power.passed);
    assert!(power.minimum_reserve_margin.value >= 0.0);
    assert!(result.evaluated_candidates > 1);
    Ok(())
}

#[test]
fn refinement_preserves_source_design_overrides() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let baseline = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let source = service.create_design_blocking(
        "source_design",
        "Source Design",
        temporary.path(),
        None,
        Some(&baseline),
        &BTreeMap::from([(
            "aircraft.name".to_owned(),
            "Override-Preserving Aircraft".to_owned(),
        )]),
    )?;
    let result = service.auto_refine_design_blocking(RefinementSpec {
        scenario_path: &source.scenario_path,
        output_design_id: "refined_design",
        display_name: "Refined Design",
        design_root: temporary.path(),
        backend: "native",
        artifact_path: None,
        max_iterations: 2,
    })?;
    let resolved = service.resolve_blocking(&result.design.scenario_path, &BTreeMap::new())?;
    assert_eq!(resolved.aircraft.name, "Override-Preserving Aircraft");
    Ok(())
}

#[test]
fn refinement_rejects_topology_before_creating_output() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let source = copy_c172_without_horizontal_tail(&temporary.path().join("source"))?;
    let output_root = temporary.path().join("designs");
    let artifact = temporary.path().join("rejected.vsp3");
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.auto_refine_design_blocking(RefinementSpec {
        scenario_path: &source,
        output_design_id: "must_not_exist",
        display_name: "Rejected Design",
        design_root: &output_root,
        backend: "native",
        artifact_path: Some(&artifact),
        max_iterations: 12,
    });

    assert!(result.is_err_and(|error| error.to_string().contains("UNSUPPORTED_BACKEND_TOPOLOGY")));
    assert!(!output_root.exists());
    assert!(!artifact.exists());
    Ok(())
}

#[test]
fn refinement_rejects_backend_before_creating_output() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let output_root = temporary.path().join("designs");
    let artifact = temporary.path().join("rejected.vsp3");
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.auto_refine_design_blocking(RefinementSpec {
        scenario_path: &source,
        output_design_id: "must_not_exist",
        display_name: "Rejected Design",
        design_root: &output_root,
        backend: "typo",
        artifact_path: Some(&artifact),
        max_iterations: 12,
    });

    assert!(result.is_err_and(|error| error.to_string().contains("UNKNOWN_ANALYSIS_BACKEND")));
    assert!(!output_root.exists());
    assert!(!artifact.exists());
    Ok(())
}

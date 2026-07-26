use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::services::analysis::ApplicationService;

use super::FeasibilityResult;

fn copy_reference_fixture(
    example: &str,
    destination: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(example);
    fs::create_dir_all(destination.join("profiles"))?;
    let mut paths = vec![
        "aircraft.yaml",
        "mission.yaml",
        "requirements.yaml",
        "scenario.yaml",
        "profiles/engine.yaml",
    ];
    if source.join("profiles/propeller.yaml").is_file() {
        paths.push("profiles/propeller.yaml");
    }
    for path in paths {
        fs::copy(source.join(path), destination.join(path))?;
    }
    Ok(destination.join("scenario.yaml"))
}

fn remove_explicit_topology(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = directory.join("aircraft.yaml");
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
    let aircraft = document["aircraft"]
        .as_mapping_mut()
        .ok_or_else(|| io::Error::other("missing aircraft mapping"))?;
    aircraft.remove(serde_yaml::Value::String("topology".to_owned()));
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn add_twin_boom(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = directory.join("aircraft.yaml");
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
    let components = document["aircraft"]["topology"]["components"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology components"))?;
    components.push(serde_yaml::from_str(
        "{ id: twin_boom, kind: boom, count: 2 }",
    )?);
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn remove_horizontal_tail(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = directory.join("aircraft.yaml");
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
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
    document["aircraft"]["geometry"]["horizontal_tail"] = serde_yaml::Value::Null;
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn change_topology_engine_count(
    directory: &Path,
    count: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = directory.join("aircraft.yaml");
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
    let components = document["aircraft"]["topology"]["components"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology components"))?;
    let engine = components
        .iter_mut()
        .find(|component| component["kind"].as_str() == Some("engine"))
        .ok_or_else(|| io::Error::other("missing engine component"))?;
    engine["count"] = serde_yaml::to_value(count)?;
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn rewire_wing_attachment(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = directory.join("aircraft.yaml");
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
    let relationships = document["aircraft"]["topology"]["relationships"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology relationships"))?;
    let attachment = relationships
        .iter_mut()
        .find(|relationship| {
            relationship["kind"].as_str() == Some("attached_to")
                && relationship["source"].as_str() == Some("wing")
        })
        .ok_or_else(|| io::Error::other("missing wing attachment"))?;
    attachment["target"] = serde_yaml::Value::String("vertical_tail".to_owned());
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn rewire_engine_attachment(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = directory.join("aircraft.yaml");
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
    let relationships = document["aircraft"]["topology"]["relationships"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology relationships"))?;
    let attachment = relationships
        .iter_mut()
        .find(|relationship| relationship["source"].as_str() == Some("powerplant"))
        .ok_or_else(|| io::Error::other("missing engine attachment"))?;
    attachment["target"] = serde_yaml::Value::String("wing".to_owned());
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn duplicate_wing_attachment(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = directory.join("aircraft.yaml");
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
    let relationships = document["aircraft"]["topology"]["relationships"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology relationships"))?;
    let attachment = relationships
        .iter()
        .find(|relationship| relationship["source"].as_str() == Some("wing"))
        .cloned()
        .ok_or_else(|| io::Error::other("missing wing relationship"))?;
    relationships.push(attachment);
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn add_unmodeled_component_parameter(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = directory.join("aircraft.yaml");
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
    let components = document["aircraft"]["topology"]["components"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology components"))?;
    let wing = components
        .iter_mut()
        .find(|component| component["kind"].as_str() == Some("wing"))
        .ok_or_else(|| io::Error::other("missing wing component"))?;
    wing["parameters"] = serde_yaml::from_str("{ unsupported_twist: 1.0 }")?;
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn assert_unsupported_feature(
    result: FeasibilityResult,
    expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match result {
        FeasibilityResult::Unsupported(unsupported) => {
            assert_eq!(unsupported.backend, "native");
            assert!(
                unsupported
                    .unsupported_features
                    .iter()
                    .any(|feature| feature.contains(expected))
            );
            Ok(())
        }
        FeasibilityResult::Completed(_) => {
            Err(io::Error::other("unsupported topology was analyzed").into())
        }
    }
}

#[test]
fn installed_openvsp_refines_represented_reference_topologies()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let openvsp_available = service
        .list_analysis_backends()
        .into_iter()
        .find(|backend| backend.id == "openvsp")
        .is_some_and(|backend| backend.available);
    if !openvsp_available {
        return Ok(());
    }
    assert_installed_openvsp_reference(&service, temporary.path(), "c172", 30.0, 80.0)?;
    assert_installed_openvsp_reference(&service, temporary.path(), "b777", 1_500.0, 2_500.0)
}

fn assert_installed_openvsp_reference(
    service: &ApplicationService,
    directory: &Path,
    example: &str,
    minimum_wetted_area_m2: f64,
    maximum_wetted_area_m2: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let scenario = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(example)
        .join("scenario.yaml");
    let artifact = directory.join(format!("{example}.vsp3"));
    let result = service.evaluate_feasibility_blocking(&scenario, "openvsp", Some(&artifact))?;
    let completed = result.completed()?;
    let refinement = completed
        .refinement
        .as_ref()
        .ok_or_else(|| io::Error::other("missing OpenVSP refinement"))?;
    assert_eq!(completed.baseline.analysis.provenance.backend, "native");
    assert_eq!(refinement.analysis.provenance.backend, "openvsp");
    assert!(!refinement.analysis.polar.is_empty());
    let wetted_area_m2 = refinement.geometry.metrics.wetted_area.value;
    assert!(
        wetted_area_m2 > minimum_wetted_area_m2,
        "{example} wetted area {wetted_area_m2} is below {minimum_wetted_area_m2}"
    );
    assert!(
        wetted_area_m2 < maximum_wetted_area_m2,
        "{example} wetted area {wetted_area_m2} exceeds {maximum_wetted_area_m2}"
    );
    assert!(artifact.is_file());
    Ok(())
}

#[test]
fn inferred_topology_preserves_native_geometry() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let explicit_path = copy_reference_fixture("c172", &temporary.path().join("explicit"))?;
    let inferred_directory = temporary.path().join("inferred");
    let inferred_path = copy_reference_fixture("c172", &inferred_directory)?;
    remove_explicit_topology(&inferred_directory)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let explicit_result = service.evaluate_feasibility_blocking(&explicit_path, "native", None)?;
    let explicit_json = serde_json::to_value(&explicit_result)?;
    assert_eq!(explicit_json["status"], "completed");
    let explicit = explicit_result.completed()?;
    let inferred_result = service.evaluate_feasibility_blocking(&inferred_path, "native", None)?;
    let inferred = inferred_result.completed()?;
    assert_eq!(explicit.feasible, inferred.feasible);
    assert_eq!(
        explicit.baseline.geometry.metrics.wetted_area.value,
        inferred.baseline.geometry.metrics.wetted_area.value
    );
    Ok(())
}

#[test]
fn reference_topologies_analyze_natively() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    for example in ["c172", "b777"] {
        let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(example)
            .join("scenario.yaml");
        let result = service.evaluate_feasibility_blocking(&scenario, "native", None)?;
        assert_eq!(
            serde_json::to_value(&result)?["status"],
            "completed",
            "{example}"
        );
        assert!(
            result
                .completed()?
                .baseline
                .geometry
                .metrics
                .wetted_area
                .value
                > 0.0
        );
    }
    Ok(())
}

#[test]
fn unsupported_topology_is_an_explicit_result() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let fixture = temporary.path().join("unsupported");
    let scenario = copy_reference_fixture("c172", &fixture)?;
    add_twin_boom(&fixture)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.evaluate_feasibility_blocking(&scenario, "native", None)?;
    let result_json = serde_json::to_value(&result)?;
    assert_eq!(result_json["status"], "unsupported");
    assert!(result_json["feasible"].is_null());
    assert_eq!(result_json["path"], "aircraft.topology");
    match result {
        FeasibilityResult::Unsupported(unsupported) => {
            assert_eq!(unsupported.code, "UNSUPPORTED_BACKEND_TOPOLOGY");
            assert_eq!(unsupported.backend, "native");
            assert_eq!(unsupported.feasible, None);
            assert_eq!(unsupported.unsupported_component_kinds, ["boom"]);
            assert!(unsupported.unsupported_relationship_kinds.is_empty());
            assert!(unsupported.unsupported_features.is_empty());
        }
        FeasibilityResult::Completed(_) => {
            return Err(io::Error::other("unsupported topology was analyzed").into());
        }
    }
    Ok(())
}

#[test]
fn invalid_backend_precedes_topology_support() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let fixture = temporary.path().join("unsupported");
    let scenario = copy_reference_fixture("c172", &fixture)?;
    add_twin_boom(&fixture)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.evaluate_feasibility_blocking(&scenario, "typo", None);
    assert!(result.is_err_and(|error| error.to_string().contains("UNKNOWN_ANALYSIS_BACKEND")));
    Ok(())
}

#[test]
fn native_rejects_missing_modeled_tail() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let fixture = temporary.path().join("missing-tail");
    let scenario = copy_reference_fixture("c172", &fixture)?;
    remove_horizontal_tail(&fixture)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.evaluate_feasibility_blocking(&scenario, "native", None)?;
    assert_unsupported_feature(result, "exactly one horizontal_tail")
}

#[test]
fn native_rejects_topology_engine_count_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let fixture = temporary.path().join("engine-count");
    let scenario = copy_reference_fixture("c172", &fixture)?;
    change_topology_engine_count(&fixture, 2)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.evaluate_feasibility_blocking(&scenario, "native", None)?;
    assert_unsupported_feature(result, "engine count 1")
}

#[test]
fn native_rejects_rewired_supported_relationship() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let fixture = temporary.path().join("rewired");
    let scenario = copy_reference_fixture("c172", &fixture)?;
    rewire_wing_attachment(&fixture)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.evaluate_feasibility_blocking(&scenario, "native", None)?;
    assert_unsupported_feature(result, "relationships must exactly match")
}

#[test]
fn native_rejects_alternate_engine_attachment() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let fixture = temporary.path().join("engine-attachment");
    let scenario = copy_reference_fixture("c172", &fixture)?;
    rewire_engine_attachment(&fixture)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.evaluate_feasibility_blocking(&scenario, "native", None)?;
    assert_unsupported_feature(result, "relationships must exactly match")
}

#[test]
fn native_rejects_duplicate_relationships() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let fixture = temporary.path().join("duplicate-relationship");
    let scenario = copy_reference_fixture("c172", &fixture)?;
    duplicate_wing_attachment(&fixture)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.evaluate_feasibility_blocking(&scenario, "native", None)?;
    assert_unsupported_feature(result, "relationships must exactly match")
}

#[test]
fn native_rejects_unmodeled_component_parameters() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let fixture = temporary.path().join("parameters");
    let scenario = copy_reference_fixture("c172", &fixture)?;
    add_unmodeled_component_parameter(&fixture)?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let result = service.evaluate_feasibility_blocking(&scenario, "native", None)?;
    assert_unsupported_feature(result, "parameters on component wing")
}

#[test]
fn backend_descriptors_declare_exact_topology_capabilities()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let descriptors = service.list_analysis_backends();
    assert_eq!(descriptors.len(), 2);
    for descriptor in &descriptors {
        assert!(!descriptor.disciplines.is_empty());
        assert!(!descriptor.fidelity_levels.is_empty());
        assert!(!descriptor.topology.component_kinds.is_empty());
    }
    let native = descriptors
        .iter()
        .find(|descriptor| descriptor.id == "native")
        .ok_or_else(|| io::Error::other("missing native descriptor"))?;
    assert_eq!(
        native.topology.relationship_kinds,
        ["attached_to", "symmetric_about", "carries_load_to"]
    );
    assert!(native.topology.delegated_relationship_kinds.is_empty());
    let openvsp = descriptors
        .iter()
        .find(|descriptor| descriptor.id == "openvsp")
        .ok_or_else(|| io::Error::other("missing OpenVSP descriptor"))?;
    assert!(
        openvsp
            .topology
            .component_kinds
            .contains(&"engine".to_owned())
    );
    assert_eq!(
        openvsp.topology.relationship_kinds,
        ["attached_to", "symmetric_about"]
    );
    assert_eq!(
        openvsp.topology.delegated_relationship_kinds,
        ["carries_load_to"]
    );
    Ok(())
}

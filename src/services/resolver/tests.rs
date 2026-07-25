use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::domain::diagnostic::AexError;
use crate::domain::schema::{
    AircraftDocument, MissionDocument, ProfileDocument, RequirementsDocument, ScenarioDocument,
};
use crate::domain::study::EmbeddedStudyBaseline;
use crate::services::analysis::ApplicationService;
use crate::storage::profile_store::FileProfileStore;

use super::{ScenarioResolver, resolve_aircraft, resolve_embedded_study};

fn c172_aircraft_document() -> Result<AircraftDocument, Box<dyn std::error::Error>> {
    read_c172_document("aircraft.yaml")
}

fn read_c172_document<T: DeserializeOwned>(
    relative: &str,
) -> Result<T, Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/c172")
        .join(relative);
    Ok(serde_yaml::from_str(&fs::read_to_string(path)?)?)
}

fn embedded_c172() -> Result<EmbeddedStudyBaseline, Box<dyn std::error::Error>> {
    Ok(EmbeddedStudyBaseline {
        scenario: read_c172_document::<ScenarioDocument>("scenario.yaml")?,
        aircraft: c172_aircraft_document()?,
        mission: read_c172_document::<MissionDocument>("mission.yaml")?,
        requirements: read_c172_document::<RequirementsDocument>("requirements.yaml")?,
        profiles: vec![
            read_c172_document::<ProfileDocument>("profiles/engine.yaml")?,
            read_c172_document::<ProfileDocument>("profiles/propeller.yaml")?,
        ],
    })
}

fn assert_closed_planform(scenario: &crate::domain::schema::ResolvedScenario) {
    let wing = &scenario.aircraft.wing;
    assert!((wing.span_m.powi(2) / wing.area_m2 - wing.aspect_ratio).abs() < 1.0e-12);
}

fn collect_scenario_paths(
    directory: &std::path::Path,
    paths: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            collect_scenario_paths(&entry.path(), paths)?;
        } else if entry.file_name() == "scenario.yaml" {
            paths.push(entry.path());
        }
    }
    Ok(())
}

#[test]
fn explicit_reference_topology_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let aircraft = resolve_aircraft(c172_aircraft_document()?)?;

    assert!(!aircraft.topology.inferred);
    assert!(aircraft.topology.has_component_kind("horizontal_tail"));
    assert_eq!(aircraft.topology.relationships.len(), 6);
    Ok(())
}

#[test]
fn absent_topology_preserves_legacy_inference() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = c172_aircraft_document()?;
    document.aircraft.topology = None;
    let aircraft = resolve_aircraft(document)?;

    assert!(aircraft.topology.inferred);
    assert!(aircraft.topology.has_component_kind("fuselage"));
    assert!(aircraft.topology.has_component_kind("propeller"));
    assert!(!aircraft.topology.has_component_kind("lifting_body"));
    Ok(())
}

#[test]
fn embedded_study_applies_overrides_before_cross_document_validation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut embedded = embedded_c172()?;
    embedded
        .scenario
        .scenario
        .overrides
        .insert("mission.payload.mass".to_owned(), "400 kg".to_owned());

    assert!(matches!(
        resolve_embedded_study(&embedded),
        Err(AexError::Validation {
            code: "PAYLOAD_LIMIT_EXCEEDED",
            ..
        })
    ));
    Ok(())
}

#[test]
fn embedded_study_requires_referenced_profiles() -> Result<(), Box<dyn std::error::Error>> {
    let mut embedded = embedded_c172()?;
    embedded.aircraft.aircraft.propulsion.profile = "engine.missing".to_owned();

    assert!(matches!(
        resolve_embedded_study(&embedded),
        Err(AexError::ProfileNotFound { profile_id, .. }) if profile_id == "engine.missing"
    ));
    Ok(())
}

#[test]
fn embedded_study_retains_profile_sanity_warnings() -> Result<(), Box<dyn std::error::Error>> {
    let mut embedded = embedded_c172()?;
    let engine = embedded
        .profiles
        .iter_mut()
        .find(|profile| profile.profile.kind == "piston_engine")
        .ok_or_else(|| std::io::Error::other("missing embedded piston profile"))?;
    let parameters = engine
        .profile
        .parameters
        .as_mapping_mut()
        .ok_or_else(|| std::io::Error::other("profile parameters are not a mapping"))?;
    parameters.insert(
        serde_yaml::Value::String("rated_altitude".to_owned()),
        serde_yaml::Value::String("-1000 m".to_owned()),
    );

    let scenario = resolve_embedded_study(&embedded)?;

    assert!(scenario.warnings.iter().any(|warning| {
        warning.code == "PARAMETER_OUTSIDE_TYPICAL"
            && warning.path.as_deref() == Some("profile.parameters.rated_altitude")
    }));
    Ok(())
}

#[test]
fn inconsistent_override_is_rejected_and_paired_override_closes()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");

    assert!(matches!(
        service.resolve_blocking(
            &scenario,
            &BTreeMap::from([(
                "aircraft.geometry.wing.aspect_ratio".to_owned(),
                "20".to_owned()
            )])
        ),
        Err(AexError::Validation {
            code: "INCONSISTENT_WING_PLANFORM",
            ..
        })
    ));
    let paired = service.resolve_blocking(
        &scenario,
        &BTreeMap::from([
            (
                "aircraft.geometry.wing.aspect_ratio".to_owned(),
                "8".to_owned(),
            ),
            (
                "aircraft.geometry.wing.span".to_owned(),
                format!("{} m", (16.17_f64 * 8.0).sqrt()),
            ),
        ]),
    )?;
    assert_closed_planform(&paired);
    Ok(())
}

#[test]
fn every_shipped_example_resolves_a_closed_planform() -> Result<(), Box<dyn std::error::Error>> {
    let resolver = ScenarioResolver::new(Arc::new(FileProfileStore));
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut paths = Vec::new();
    collect_scenario_paths(&examples, &mut paths)?;
    paths.sort();
    assert!(!paths.is_empty());

    for path in paths {
        let scenario = resolver.resolve_blocking(&path, &BTreeMap::new())?;
        assert_closed_planform(&scenario);
    }
    Ok(())
}

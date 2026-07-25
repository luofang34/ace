use std::fs;
use std::path::PathBuf;

use serde::de::DeserializeOwned;

use crate::domain::diagnostic::AexError;
use crate::domain::schema::{
    AircraftDocument, MissionDocument, ProfileDocument, RequirementsDocument, ScenarioDocument,
};
use crate::domain::study::EmbeddedStudyBaseline;

use super::{resolve_aircraft, resolve_embedded_study};

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

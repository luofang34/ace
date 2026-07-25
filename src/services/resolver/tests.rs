use std::fs;
use std::path::PathBuf;

use crate::domain::schema::AircraftDocument;

use super::resolve_aircraft;

fn c172_aircraft_document() -> Result<AircraftDocument, Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/aircraft.yaml");
    Ok(serde_yaml::from_str(&fs::read_to_string(path)?)?)
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

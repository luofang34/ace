#![allow(clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use crate::domain::schema::AircraftDocument;

use super::super::resolve_aircraft;

fn c172_aircraft_document() -> Result<AircraftDocument, Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/aircraft.yaml");
    Ok(serde_yaml::from_str(&fs::read_to_string(path)?)?)
}

#[test]
fn partial_geometry_derives_only_missing_scalars() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = c172_aircraft_document()?;
    document
        .aircraft
        .geometry
        .fuselage
        .as_mut()
        .ok_or_else(|| std::io::Error::other("fixture lacks fuselage geometry"))?
        .diameter = None;
    let horizontal_tail = document
        .aircraft
        .geometry
        .horizontal_tail
        .as_mut()
        .ok_or_else(|| std::io::Error::other("fixture lacks horizontal tail geometry"))?;
    horizontal_tail.arm = None;
    let aircraft = resolve_aircraft(document)?;
    let fuselage = aircraft
        .geometry
        .fuselage
        .ok_or_else(|| std::io::Error::other("missing resolved fuselage"))?;

    assert_eq!(fuselage.length.value, 8.25);
    assert!(fuselage.length.provenance.explicitly_provided);
    assert!((fuselage.diameter.value - 16.17_f64.sqrt() / 3.0).abs() < 1.0e-12);
    assert!(!fuselage.diameter.provenance.explicitly_provided);
    assert_eq!(
        fuselage.diameter.provenance.correlation_id,
        Some("ace_conventional_geometry")
    );
    assert_eq!(fuselage.diameter.provenance.correlation_version, Some(1));
    let horizontal_tail = aircraft
        .geometry
        .horizontal_tail
        .ok_or_else(|| std::io::Error::other("missing resolved horizontal tail"))?;
    assert!((horizontal_tail.arm.value - 8.25 * 0.5).abs() < 1.0e-12);
    assert!(!horizontal_tail.arm.provenance.explicitly_provided);
    Ok(())
}

#[test]
fn geometry_without_matching_topology_component_is_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = c172_aircraft_document()?;
    document.aircraft.topology = None;
    document.aircraft.configuration = "tailless_blended_wing_body".to_owned();
    document.aircraft.geometry.fuselage = None;
    document.aircraft.geometry.vertical_tail = None;

    let error = resolve_aircraft(document).expect_err("unmatched tail geometry must fail");
    assert_eq!(error.detail().code, "GEOMETRY_COMPONENT_MISMATCH");
    assert_eq!(
        error.detail().path.as_deref(),
        Some("aircraft.geometry.horizontal_tail")
    );
    Ok(())
}

#[test]
fn tailless_topology_does_not_invent_conventional_geometry()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = c172_aircraft_document()?;
    document.aircraft.topology = None;
    document.aircraft.configuration = "tailless_blended_wing_body".to_owned();
    document.aircraft.geometry.fuselage = None;
    document.aircraft.geometry.horizontal_tail = None;
    document.aircraft.geometry.vertical_tail = None;
    document.aircraft.mass.components.clear();
    document.aircraft.mass.fuel_station = None;
    document.aircraft.mass.payload_station = None;
    let aircraft = resolve_aircraft(document.clone())?;
    let mut scaled_document = document;
    scaled_document.aircraft.geometry.wing.area = Some("32.34 m^2".to_owned());
    scaled_document.aircraft.geometry.wing.aspect_ratio = None;
    let scaled = resolve_aircraft(scaled_document)?;

    assert!(aircraft.geometry.fuselage.is_none());
    assert!(aircraft.geometry.horizontal_tail.is_none());
    assert!(aircraft.geometry.vertical_tail.is_none());
    assert!(aircraft.mass.statement.payload_station.value > 1.0);
    assert!(
        scaled.mass.statement.payload_station.value > aircraft.mass.statement.payload_station.value
    );
    Ok(())
}

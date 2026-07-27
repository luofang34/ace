use std::io;

use serde_json::json;
use serde_yaml::Value;

use crate::domain::schema::AssumptionEntry;
use crate::test_support::example_scenario;

use super::collect_assumptions;

fn entry<'a>(entries: &'a [AssumptionEntry], path: &str) -> Result<&'a AssumptionEntry, io::Error> {
    entries
        .iter()
        .find(|entry| entry.parameter_path == path)
        .ok_or_else(|| io::Error::other(format!("missing ledger entry {path}")))
}

#[test]
fn ledger_assigns_units_only_to_registered_quantity_paths() -> Result<(), Box<dyn std::error::Error>>
{
    let document: Value = serde_yaml::from_str(
        r#"
aircraft:
  id: C172 reference identity
  name: C172-Class Reference Aircraft
  topology:
    components:
      - id: primary wing
        roles: [fuel storage]
  metadata:
    purpose: 10 kt planning note
  mass:
    maximum_takeoff_mass: 1111 kg
  geometry:
    wing:
      area: 16.17 m^2
    fuselage:
      length: 8.25 m
      diameter: 1.34 m
    horizontal_tail:
      area: 3.2 m^2
      arm: 4.1 m
mission:
  payload:
    mass: 230 kg
  initial_state:
    altitude: 5000 ft
    true_airspeed: 100 kt
    fuel_fraction: 0.5
  segments:
    - id: cruise segment
      duration: 45 min
      altitude: 8000 ft
      true_airspeed: 115 kt
requirements:
  items:
    - id: landing stall
      metric: performance.stall_speed_landing
      operator: le
      value: 50 kt
      severity: hard
"#,
    )?;
    let mut entries = Vec::new();
    for root in ["aircraft", "mission", "requirements"] {
        collect_assumptions(root, document.get(root), false, &mut entries);
    }

    for (path, value) in [
        ("aircraft.id", json!("C172 reference identity")),
        ("aircraft.name", json!("C172-Class Reference Aircraft")),
        (
            "aircraft.topology.components.0.roles.0",
            json!("fuel storage"),
        ),
        ("aircraft.metadata.purpose", json!("10 kt planning note")),
        ("mission.initial_state.fuel_fraction", json!(0.5)),
        ("mission.segments.0.id", json!("cruise segment")),
        ("requirements.items.0.id", json!("landing stall")),
        (
            "requirements.items.0.metric",
            json!("performance.stall_speed_landing"),
        ),
        ("requirements.items.0.operator", json!("le")),
        ("requirements.items.0.severity", json!("hard")),
    ] {
        assert_eq!(entry(&entries, path)?.resolved_value, value);
        assert_eq!(entry(&entries, path)?.unit, None);
    }
    for (path, unit) in [
        ("aircraft.mass.maximum_takeoff_mass", "kg"),
        ("aircraft.geometry.wing.area", "m^2"),
        ("aircraft.geometry.fuselage.length", "m"),
        ("aircraft.geometry.fuselage.diameter", "m"),
        ("aircraft.geometry.horizontal_tail.area", "m^2"),
        ("aircraft.geometry.horizontal_tail.arm", "m"),
        ("mission.payload.mass", "kg"),
        ("mission.initial_state.altitude", "ft"),
        ("mission.initial_state.true_airspeed", "kt"),
        ("mission.segments.0.duration", "min"),
        ("mission.segments.0.altitude", "ft"),
        ("mission.segments.0.true_airspeed", "kt"),
        ("requirements.items.0.value", "kt"),
    ] {
        assert_eq!(entry(&entries, path)?.unit.as_deref(), Some(unit));
    }
    Ok(())
}

#[test]
fn shipped_c172_ledger_preserves_name_and_physical_units() -> Result<(), Box<dyn std::error::Error>>
{
    let scenario = example_scenario("c172")?;
    let name = entry(&scenario.assumptions, "aircraft.name")?;
    let mass = entry(&scenario.assumptions, "aircraft.mass.maximum_takeoff_mass")?;
    let serialized = serde_json::to_value(name)?;

    assert_eq!(name.resolved_value, json!("C172-Class Reference Aircraft"));
    assert_eq!(name.unit, None);
    assert_eq!(serialized["resolved_value"], name.resolved_value);
    assert_eq!(serialized["unit"], serde_json::Value::Null);
    assert_eq!(mass.unit.as_deref(), Some("kg"));
    Ok(())
}

#[test]
fn derived_geometry_ledger_retains_correlation_provenance() -> Result<(), Box<dyn std::error::Error>>
{
    let scenario = example_scenario("sr71")?;
    let length = entry(&scenario.assumptions, "aircraft.geometry.fuselage.length")?;
    let vertical_area = entry(
        &scenario.assumptions,
        "aircraft.geometry.vertical_tail.area",
    )?;

    for assumption in [length, vertical_area] {
        assert_eq!(
            assumption.provenance_kind,
            "statistical_correlation".to_owned()
        );
        assert_eq!(
            assumption.correlation_id.as_deref(),
            Some("ace_conventional_geometry")
        );
        assert_eq!(assumption.correlation_version, Some(1));
        assert!(!assumption.explicitly_provided);
        assert!(assumption.supplied_by_default);
    }
    assert_eq!(length.unit.as_deref(), Some("m"));
    assert_eq!(vertical_area.unit.as_deref(), Some("m^2"));
    assert!(
        entry(
            &scenario.assumptions,
            "aircraft.geometry.horizontal_tail.area"
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn mixed_geometry_assumptions_serialize_as_fixed_width_csv()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("sr71")?;
    let mut writer = csv::Writer::from_writer(Vec::new());
    for assumption in &scenario.assumptions {
        writer.serialize(assumption)?;
    }
    let csv = String::from_utf8(writer.into_inner()?)?;

    assert!(csv.lines().next().is_some_and(|header| {
        header.contains("correlation_id") && header.contains("correlation_version")
    }));
    assert!(csv.contains("ace_conventional_geometry"));
    Ok(())
}

#[test]
fn resolved_mass_statement_sources_are_recorded_in_the_ledger()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("c172")?;
    let fuselage = entry(
        &scenario.assumptions,
        "aircraft.mass.components.fuselage.mass",
    )?;
    let engine = entry(
        &scenario.assumptions,
        "aircraft.mass.components.powerplant.mass",
    )?;

    assert_eq!(fuselage.provenance_kind, "statistical");
    assert_eq!(
        fuselage.correlation_id.as_deref(),
        Some("mass.component_fraction")
    );
    assert_eq!(fuselage.correlation_version, Some(1));
    assert_eq!(engine.provenance_kind, "profile");
    assert!(engine.inherited_from_profile);
    assert_eq!(engine.unit.as_deref(), Some("kg"));
    Ok(())
}

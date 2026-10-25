use super::*;
use ace_engine::Registry;
use ace_model::{AnalysisResources, CatalogEntry, Source};
use std::path::PathBuf;

fn yaml(path: &str, key: &str) -> serde_json::Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let text = std::fs::read_to_string(root.join(path)).unwrap();
    let value: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
    let json = serde_json::to_value(value).unwrap();
    key.split('.').fold(json, |value, part| value[part].clone())
}

fn entry(kind: &str, document: serde_json::Value) -> CatalogEntry {
    CatalogEntry {
        kind: kind.into(),
        version: 1,
        source: Source {
            kind: "test".into(),
            reference: "legacy example".into(),
            revision: None,
        },
        document,
    }
}

/// Vehicle-port and catalog form of a legacy example, evaluated by the envelope plugin.
fn evaluate(name: &str) -> PluginOutput {
    let scenario = crate::test_support::example_scenario(name).unwrap();
    let aircraft = &scenario.aircraft;
    let mut catalog = BTreeMap::from([
        (
            "polars".to_owned(),
            entry(
                crate::services::synthesis::catalog::POLARS,
                yaml(&format!("{name}/aircraft.yaml"), "aircraft.aerodynamics"),
            ),
        ),
        (
            "limits".to_owned(),
            entry(
                crate::services::synthesis::catalog::LIMITS,
                yaml(&format!("{name}/aircraft.yaml"), "aircraft.limits"),
            ),
        ),
    ]);
    let engine = yaml(&format!("{name}/profiles/engine.yaml"), "profile");
    let kind = engine["type"].as_str().unwrap().to_owned();
    catalog.insert("engine".into(), entry(&kind, engine));
    if aircraft.propulsion.propeller_profile.is_some() {
        let propeller = yaml(&format!("{name}/profiles/propeller.yaml"), "profile");
        catalog.insert(
            "propeller".into(),
            entry(crate::services::synthesis::catalog::PROPELLER, propeller),
        );
    }
    let field = &scenario.mission.field;
    let mass = &aircraft.mass;
    let values = [
        aircraft.wing.area_m2,
        aircraft.wing.span_m,
        aircraft.wing.sweep_quarter_chord_rad.to_degrees(),
        mass.maximum_takeoff_mass_kg,
        mass.operating_empty_mass_kg,
        mass.maximum_payload_mass_kg,
        mass.maximum_fuel_mass_kg,
        field
            .takeoff_mass_kg
            .unwrap_or(mass.maximum_takeoff_mass_kg),
        field
            .landing_mass_kg
            .unwrap_or(mass.maximum_takeoff_mass_kg),
        f64::from(aircraft.propulsion.engine_count),
        aircraft.propulsion.sizing_factor,
        field.elevation_m,
        field.obstacle_height_m,
        field.braking_coefficient,
        field.runway_slope,
        field.headwind_m_s,
    ];
    let inputs: BTreeMap<String, Quantity> = INPUTS
        .iter()
        .zip(values)
        .map(|((port, unit), value)| ((*port).to_owned(), Quantity::new(value, *unit).unwrap()))
        .collect();
    let registry = Registry::default();
    let context = AnalysisContext {
        resources: AnalysisResources {
            catalog,
            ..AnalysisResources::default()
        },
        values: &inputs,
        registry: &registry,
    };
    PerformanceEnvelope::new()
        .evaluate(&inputs, &context)
        .unwrap()
}

#[test]
fn vehicle_envelope_reproduces_the_reference_aircraft_performance() {
    for name in ["c172", "b777"] {
        let scenario = crate::test_support::example_scenario(name).unwrap();
        let legacy = PointAnalyzer::new(scenario).summary(None).unwrap();
        let output = evaluate(name);
        let expected = [
            ("stall_speed_clean", legacy.stall_speed_clean_m_s),
            ("stall_speed_landing", legacy.stall_speed_landing_m_s),
            ("best_glide_speed", legacy.best_glide_speed_m_s),
            ("maximum_level_speed", legacy.maximum_level_speed_m_s),
            ("maximum_lift_to_drag", legacy.maximum_lift_to_drag_ratio),
            ("service_ceiling", legacy.service_ceiling_m.unwrap()),
            ("absolute_ceiling", legacy.absolute_ceiling_m.unwrap()),
            (
                "takeoff_field_length",
                legacy.takeoff_field_length_m.unwrap(),
            ),
            (
                "landing_field_length",
                legacy.landing_field_length_m.unwrap(),
            ),
        ];
        for (port, value) in expected {
            let actual = output.values[port].value;
            assert!(
                (actual - value).abs() <= 1e-9 * value.abs().max(1.0),
                "{name} {port}: {actual} != {value}"
            );
        }
    }
}

#[test]
fn reference_bands_hold_on_the_vehicle_envelope() {
    let b777 = evaluate("b777");
    let ceiling_ft = b777.values["service_ceiling"].value / 0.3048;
    assert!((39_000.0..=45_000.0).contains(&ceiling_ft), "{ceiling_ft}");
    assert!((16.0..=22.0).contains(&b777.values["maximum_lift_to_drag"].value));
    let c172 = evaluate("c172");
    let stall_kt = c172.values["stall_speed_landing"].value / 0.514_444;
    assert!((35.0..=60.0).contains(&stall_kt), "{stall_kt}");
}

#[test]
fn missing_or_ambiguous_catalog_data_fails_before_analysis() {
    let registry = Registry::default();
    let inputs = BTreeMap::new();
    let context = AnalysisContext {
        resources: AnalysisResources::default(),
        values: &inputs,
        registry: &registry,
    };
    assert!(
        PerformanceEnvelope::new()
            .evaluate(&inputs, &context)
            .is_err()
    );
}

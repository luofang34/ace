use super::*;
use ace_engine::{Session, ValueStatus};
use ace_model::{Source, VehicleModel};
use serde_json::{Value, json};

fn source() -> Source {
    Source {
        kind: "test".into(),
        reference: "resource dependency test".into(),
        revision: None,
    }
}

fn parameter(value: Option<f64>, unit: &str) -> Value {
    json!({"unit":unit,"value":value.map(|value| json!({"value":value,"unit":unit})),
        "source":source(),"owner":"test"})
}

fn model() -> VehicleModel {
    let mut parameters = serde_json::Map::new();
    for (id, value, unit) in [
        ("duration", 10.0, "s"),
        ("speed", 100.0, "m/s"),
        ("flow", 1.0, "kg/s"),
        ("rate", 2.0, "USD/s"),
        ("tank.mass", 20.0, "kg"),
    ] {
        parameters.insert(id.into(), parameter(Some(value), unit));
    }
    for (id, unit) in [
        ("phase.duration", "s"),
        ("phase.distance", "m"),
        ("phase.fuel", "kg"),
        ("phase.cost", "USD"),
        ("mission.duration", "s"),
        ("mission.distance", "m"),
        ("mission.block", "kg"),
        ("mission.reserve", "kg"),
        ("mission.remaining", "kg"),
        ("mission.mass", "kg"),
        ("mission.cost", "USD"),
        ("economics.trip", "USD"),
        ("economics.casm", "USD/nmi"),
        ("economics.network", "1"),
    ] {
        parameters.insert(id.into(), parameter(None, unit));
    }
    serde_json::from_value(json!({"schema_version":1,"id":"vehicle","name":"Resource test","domain":"vehicle",
        "parameters":parameters,"analyses":{
            "phase":{"plugin":"vehicle.constant_rate_phase","version":"1", "inputs":{
                "duration":"duration","speed":"speed","fuel_flow":"flow","cost_rate":"rate"},
                "outputs":{"duration":"phase.duration","distance":"phase.distance","fuel_burn":"phase.fuel","cost":"phase.cost"}},
            "mission":{"plugin":"vehicle.mission_sequence","version":"1","inputs":{},"resources":{"missions":["design"]},
                "outputs":{"duration":"mission.duration","distance":"mission.distance","block_fuel":"mission.block",
                "reserve_fuel":"mission.reserve","remaining_fuel":"mission.remaining","final_mass":"mission.mass","cost":"mission.cost"}},
            "economics":{"plugin":"vehicle.operations","version":"1","inputs":{},"resources":{"operations":true},
                "outputs":{"trip_cost":"economics.trip","casm":"economics.casm","network_compatible":"economics.network"}}},
        "mass_items":{
            "empty":{"kind":"empty","mass":{"value":100,"unit":"kg"},"cg":[{"value":0,"unit":"m"},{"value":0,"unit":"m"},{"value":0,"unit":"m"}],"source":source()},
            "tank":{"kind":"fuel","mass":"tank.mass","cg":[{"value":1,"unit":"m"},{"value":0,"unit":"m"},{"value":0,"unit":"m"}],"source":source()}},
        "missions":{"design":{"name":"Design mission","stages":[{"id":"cruise","kind":"cruise","analysis":"phase","fuel_sequence":["tank"]}]}},
        "operations":{"seats":{"value":10,"unit":"1"},"load_factor":{"value":0.8,"unit":"1"},"distance":"mission.distance",
            "costs":[{"id":"phase","category":"mission","amount":"mission.cost"}]}})).unwrap()
}

fn session() -> Session {
    let mut registry = Registry::default();
    register(&mut registry).unwrap();
    Session::new(model(), registry).unwrap()
}

#[test]
fn resource_only_dependency_edits_invalidate_mission_and_economics() {
    let mut session = session();
    let original = session.run().unwrap();
    assert!(original.complete());
    session
        .edit(
            &BTreeMap::from([(
                "tank.mass".into(),
                Quantity::new(15.0, Unit::Kilogram).unwrap(),
            )]),
            source(),
        )
        .unwrap();
    assert_eq!(
        session.snapshot().values["mission.remaining"].status,
        ValueStatus::Stale
    );
    let result = session.run().unwrap();
    assert_eq!(result.executed, ["analysis:mission", "analysis:economics"]);
    assert_eq!(
        result.values["mission.remaining"].quantity.unwrap().value,
        5.0
    );
    let run = result
        .runs
        .iter()
        .rev()
        .find(|run| run.node == "analysis:mission")
        .unwrap();
    assert_eq!(run.inputs["tank.mass"].quantity.unwrap().value, 15.0);
    assert_eq!(run.resources.missions["design"].stages[0].id, "cruise");
    assert!(run.resources.mass_items.contains_key("empty"));
    assert_eq!(run.details["remaining_fuel_kg"], 5.0);
}

#[test]
fn mission_failure_revokes_scalars_blocks_cost_and_retains_failure_evidence() {
    let mut session = session();
    session.run().unwrap();
    session
        .edit(
            &BTreeMap::from([(
                "flow".into(),
                Quantity::new(3.0, Unit::KilogramPerSecond).unwrap(),
            )]),
            source(),
        )
        .unwrap();
    let result = session.run().unwrap();
    assert!(!result.complete());
    assert_eq!(
        result.values["mission.remaining"].status,
        ValueStatus::Failed
    );
    assert!(result.values["mission.remaining"].quantity.is_none());
    assert_eq!(result.values["economics.casm"].status, ValueStatus::Blocked);
    let run = result
        .runs
        .iter()
        .rev()
        .find(|run| run.node == "analysis:mission")
        .unwrap();
    assert_eq!(run.details["failed_stage"], "cruise");
    assert!(run.outputs.is_empty());
    assert!(run.message.as_ref().unwrap().contains("shortfall 10 kg"));
}

#[test]
fn provisional_phase_quality_propagates_through_resource_edges() {
    let result = session().run().unwrap();
    assert_eq!(
        result.values["economics.casm"].status,
        ValueStatus::Approximate
    );
}

#[test]
fn unknown_resource_reference_is_rejected_before_execution() {
    let mut model = model();
    model
        .analyses
        .get_mut("mission")
        .unwrap()
        .resources
        .missions = vec!["missing".into()];
    assert!(
        model
            .validate()
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
}

#[test]
fn usable_volume_requires_explicit_measurements_instead_of_box_volume() {
    let mut resources = ace_model::AnalysisResources::default();
    resources.geometry.insert("tank".into(), serde_json::from_value(json!({"name":"Tank","role":"fuel_tank",
        "source":source(),"bounds":{"minimum":[{"value":0,"unit":"m"},{"value":0,"unit":"m"},{"value":0,"unit":"m"}],
        "maximum":[{"value":2,"unit":"m"},{"value":2,"unit":"m"},{"value":2,"unit":"m"}]}})).unwrap());
    let values = BTreeMap::new();
    let registry = Registry::default();
    let mut context = AnalysisContext {
        resources,
        values: &values,
        registry: &registry,
    };
    assert!(evaluation::evaluate(Kind::Volume, &values, &context).is_err());
    context.resources.geometry.get_mut("tank").unwrap().volume = Some(
        ace_model::Binding::Constant(Quantity::new(5.0, Unit::CubicMeter).unwrap()),
    );
    let output = evaluation::evaluate(Kind::Volume, &values, &context).unwrap();
    assert_eq!(output.values["volume"].value, 5.0);
    assert_eq!(output.details["tank"], 5.0);
}

use super::*;
use serde_json::json;

pub(super) fn fixture() -> (AnalysisResources, BTreeMap<String, Quantity>) {
    let mut resources = AnalysisResources::default();
    for (id, kind, mass, x) in [
        ("empty", "empty", 100.0, 0.0),
        ("front", "fuel", 10.0, -2.0),
        ("rear", "fuel", 20.0, 2.0),
        ("stores", "payload", 10.0, 4.0),
    ] {
        resources.mass_items.insert(id.into(), serde_json::from_value(json!({"kind":kind,
            "mass":{"value":mass,"unit":"kg"},"cg":[{"value":x,"unit":"m"},{"value":0,"unit":"m"},{"value":0,"unit":"m"}],
            "source":{"kind":"test","reference":"mission fixture"}})).unwrap());
    }
    resources.stage_analyses.insert("phase".into(), serde_json::from_value(json!({
        "plugin":"test.phase","version":"1","inputs":{},"outputs":{
            "duration":"phase.duration","distance":"phase.distance","fuel_burn":"phase.fuel","cost":"phase.cost"}})).unwrap());
    resources.missions.insert("sortie".into(), serde_json::from_value(json!({"name":"Test sortie","stages":[
        {"id":"combat","kind":"weapons_release","analysis":"phase","fuel_sequence":["front","rear"],
            "payload_release":{"stores":{"value":5,"unit":"kg"}}},
        {"id":"reserve","kind":"reserve","analysis":"phase","fuel_sequence":["rear"],"reserve":true}
    ]})).unwrap());
    let values = BTreeMap::from([
        (
            "phase.duration".into(),
            Quantity::new(60.0, Unit::Second).unwrap(),
        ),
        (
            "phase.distance".into(),
            Quantity::new(600.0, Unit::Meter).unwrap(),
        ),
        (
            "phase.fuel".into(),
            Quantity::new(12.0, Unit::Kilogram).unwrap(),
        ),
        (
            "phase.cost".into(),
            Quantity::new(100.0, Unit::Dollar).unwrap(),
        ),
    ]);
    (resources, values)
}

#[test]
fn phase_sequence_consumes_tanks_releases_payload_and_recomputes_cg() {
    let (resources, values) = fixture();
    let report = evaluate_mission("sortie", &resources, &values).unwrap();
    assert!(report.completed);
    assert_eq!(report.block_fuel_kg, 12.0);
    assert_eq!(report.reserve_fuel_kg, 12.0);
    assert_eq!(report.remaining_fuel_kg, 6.0);
    assert_eq!(report.final_state.mass.total_mass_kg, 111.0);
    assert_eq!(report.final_state.state.time.value, 120.0);
    assert_eq!(report.final_state.state.cost.value, 200.0);
    assert_eq!(report.stages[0].end.mass.items["front"].mass_kg, 0.0);
    assert_eq!(report.stages[0].end.mass.items["rear"].mass_kg, 18.0);
    assert!((report.final_state.mass.cg_m[0] - 32.0 / 111.0).abs() < 1e-12);
}

#[test]
fn failed_transition_retains_last_valid_state_without_partial_consumption() {
    let (resources, mut values) = fixture();
    values.insert(
        "phase.fuel".into(),
        Quantity::new(16.0, Unit::Kilogram).unwrap(),
    );
    let report = evaluate_mission("sortie", &resources, &values).unwrap();
    assert!(!report.completed);
    assert_eq!(report.failed_stage.as_deref(), Some("reserve"));
    assert_eq!(report.stages.len(), 1);
    assert_eq!(report.remaining_fuel_kg, 14.0);
    assert_eq!(report.final_state.state.time.value, 60.0);
    assert!(report.failure.unwrap().contains("shortfall 2 kg"));
}

#[test]
fn named_initial_loading_changes_fuel_feasibility() {
    let (mut resources, values) = fixture();
    resources.loading_cases.insert(
        "light".into(),
        serde_json::from_value(json!({"masses":{
        "front":{"value":0,"unit":"kg"},"rear":{"value":5,"unit":"kg"}}}))
        .unwrap(),
    );
    resources.missions.get_mut("sortie").unwrap().loading = Some("light".into());
    let report = evaluate_mission("sortie", &resources, &values).unwrap();
    assert!(!report.completed);
    assert!(report.stages.is_empty());
    assert_eq!(report.remaining_fuel_kg, 5.0);
}

#[test]
fn payload_cannot_be_released_twice_beyond_available_mass() {
    let (mut resources, values) = fixture();
    resources.missions.get_mut("sortie").unwrap().stages[1]
        .payload_release
        .insert(
            "stores".into(),
            ace_model::Binding::Constant(Quantity::new(6.0, Unit::Kilogram).unwrap()),
        );
    let report = evaluate_mission("sortie", &resources, &values).unwrap();
    assert!(!report.completed);
    assert_eq!(report.remaining_fuel_kg, 18.0);
    assert!(report.failure.unwrap().contains("exceeds stores payload"));
}

#[test]
fn decimal_tank_depletion_tolerates_only_floating_point_roundoff() {
    let (mut resources, mut values) = fixture();
    resources
        .missions
        .get_mut("sortie")
        .unwrap()
        .stages
        .truncate(1);
    for (id, mass) in [("front", 0.1), ("rear", 0.2)] {
        resources.mass_items.get_mut(id).unwrap().mass =
            ace_model::Binding::Constant(Quantity::new(mass, Unit::Kilogram).unwrap());
    }
    values.insert(
        "phase.fuel".into(),
        Quantity::new(0.1 + 0.2, Unit::Kilogram).unwrap(),
    );
    assert!(
        evaluate_mission("sortie", &resources, &values)
            .unwrap()
            .completed
    );
    values.insert(
        "phase.fuel".into(),
        Quantity::new(0.30000001, Unit::Kilogram).unwrap(),
    );
    assert!(
        !evaluate_mission("sortie", &resources, &values)
            .unwrap()
            .completed
    );
}

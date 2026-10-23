use super::*;
use ace_engine::{Registry, Session};
use serde_json::json;

pub(crate) fn input(value: f64, unit: &str) -> serde_json::Value {
    json!({"unit":unit,"value":{"value":value,"unit":unit},
        "source":{"kind":"estimated","reference":"illustrative capability fixture"},"owner":"test"})
}

pub(crate) fn map() -> CapabilityMap {
    CapabilityMap {
        unit_cost: "cost.unit".into(),
        endurance: "flight.endurance".into(),
        cruise_speed: "flight.speed".into(),
        operating_cost_per_hour: "cost.hourly".into(),
        loss_per_sortie: "reliability.loss".into(),
        turnaround: "ground.turnaround".into(),
    }
}

/// A vehicle whose endurance is computed from fuel and burn rate.
pub(crate) fn capability(
    id: &str,
    unit_cost: f64,
    endurance_h: f64,
    loss: f64,
) -> VehicleCapability {
    let model: VehicleModel = serde_json::from_value(json!({"schema_version":1,"id":id,"name":id,"domain":"vehicle",
        "parameters":{
            "cost.unit":input(unit_cost,"USD"),"flight.speed":input(50.0,"m/s"),"cost.hourly":input(100.0,"USD/h"),
            "reliability.loss":input(loss,"1"),"ground.turnaround":input(2.0,"h"),
            "fuel.usable":input(endurance_h * 10.0,"kg"),"fuel.flow":input(10.0 / 3600.0,"kg/s"),
            "flight.endurance":{"unit":"h","source":{"kind":"derived","reference":"fuel / flow"},"owner":"test",
                "expression":{"op":"divide","left":{"op":"reference","parameter":"fuel.usable"},"right":{"op":"reference","parameter":"fuel.flow"}}}}}))
    .unwrap();
    let mut session = Session::new(model, Registry::default()).unwrap();
    let snapshot = session.run().unwrap();
    VehicleCapability::from_snapshot(session.model(), &snapshot, &map()).unwrap()
}

#[test]
fn capability_reads_traceable_values_from_a_synthesis_snapshot() {
    let record = capability("uas", 50_000.0, 10.0, 0.01);
    assert!((record.values.endurance.value - 36_000.0).abs() < 1e-6);
    assert_eq!(record.values.turnaround.value, 7200.0);
    assert!(!record.approximate);
    assert!(record.evidence["endurance"].run_id.is_some());
    assert_eq!(record.evidence["unit_cost"].source.kind, "estimated");
    record.verify().unwrap();
    let mut tampered = record.clone();
    tampered.values.unit_cost.value = 1.0;
    assert!(tampered.verify().is_err());
}

#[test]
fn missing_stale_or_out_of_range_capabilities_are_rejected() {
    let reference = capability("uas", 50_000.0, 10.0, 0.01);
    let model: VehicleModel = serde_json::from_value(
        json!({"schema_version":1,"id":"uas","name":"uas","domain":"vehicle",
        "parameters":{"cost.unit":input(1.0,"USD")}}),
    )
    .unwrap();
    let mut session = Session::new(model, Registry::default()).unwrap();
    let snapshot = session.run().unwrap();
    assert!(VehicleCapability::from_snapshot(session.model(), &snapshot, &map()).is_err());
    let mut other = session.model().clone();
    other.revision = 9;
    assert!(VehicleCapability::from_snapshot(&other, &snapshot, &map()).is_err());
    let mut invalid = reference;
    invalid.values.loss_per_sortie.value = 1.5;
    assert!(invalid.verify().is_err());
}

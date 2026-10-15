use super::*;
use ace_model::{Binding, Expression, ModelEdit};

fn accounting_session() -> Session {
    let mut model: VehicleModel = serde_json::from_str(include_str!(
        "../../../../../examples/synthesis/trip-cost.json"
    ))
    .unwrap();
    let mut unrelated = model.parameters["cost"].clone();
    unrelated.value = None;
    unrelated.expression = Some(Expression::Constant {
        value: Quantity::new(7.0, Unit::Dollar).unwrap(),
    });
    model.parameters.insert("unrelated".into(), unrelated);
    let mut registry = Registry::default();
    register(&mut registry).unwrap();
    Session::new(model, registry).unwrap()
}

#[test]
fn block_time_and_invoice_edits_recompute_accounting_and_reuse_unrelated_runs() {
    let mut session = accounting_session();
    let before = session.run().unwrap();
    assert!(before.complete());
    assert_eq!(
        before.values["cost_per_hour"].quantity.unwrap().value,
        5000.0
    );
    session
        .edit(
            &BTreeMap::from([
                (
                    "duration".into(),
                    Quantity::new(14400.0, Unit::Second).unwrap(),
                ),
                ("cost".into(), Quantity::new(12000.0, Unit::Dollar).unwrap()),
            ]),
            source(),
        )
        .unwrap();
    let pending = session.snapshot();
    assert_eq!(pending.values["cost_per_hour"].status, ValueStatus::Stale);
    assert_eq!(pending.values["cost_per_hour"].quantity, None);
    let after = session.run().unwrap();
    assert_eq!(after.executed, ["analysis:accounting"]);
    assert_eq!(
        after.values["cost_per_hour"].quantity.unwrap().value,
        3000.0
    );
    assert_eq!(after.values["trip_cost"].quantity.unwrap().value, 12000.0);
    assert_eq!(
        after.values["unrelated"].run_id,
        before.values["unrelated"].run_id
    );
    assert_eq!(after.revision, before.revision.wrapping_add(1));
    let run = after.runs.last().unwrap();
    assert_eq!(
        run.inputs["duration"]
            .quantity
            .unwrap()
            .to(Unit::Hour)
            .unwrap()
            .value,
        4.0
    );
    assert_eq!(run.details["cost_per_block_hour_usd"], 3000.0);
    assert_eq!(run.details["seats"], 0);
    assert!(run.details["casm_usd_per_nmi"].is_null());
}

#[test]
fn missing_or_zero_duration_revokes_previous_cost_outputs() {
    let mut session = accounting_session();
    session.run().unwrap();
    session
        .edit(
            &BTreeMap::from([("duration".into(), Quantity::new(0.0, Unit::Hour).unwrap())]),
            source(),
        )
        .unwrap();
    let invalid = session.run().unwrap();
    assert!(!invalid.complete());
    assert_eq!(invalid.values["cost_per_hour"].status, ValueStatus::Failed);
    assert_eq!(invalid.values["cost_per_hour"].quantity, None);
    let mut operations = session.model().operations.clone().unwrap();
    operations.block_duration = None;
    session
        .transact(&[ModelEdit::Operations {
            value: Some(operations),
        }])
        .unwrap();
    let missing = session.run().unwrap();
    assert!(!missing.complete());
    assert!(
        missing
            .runs
            .last()
            .unwrap()
            .message
            .as_ref()
            .unwrap()
            .contains("requires a positive normal-trip duration")
    );
    assert_eq!(missing.values["trip_cost"].quantity, None);
}

#[test]
fn invalid_duration_binding_rolls_back_model_revision_and_results() {
    let mut session = accounting_session();
    session.run().unwrap();
    let before = serde_json::to_value(session.snapshot()).unwrap();
    let model = session.model().clone();
    for binding in ["cost", "missing"] {
        let mut operations = model.operations.clone().unwrap();
        operations.block_duration = Some(Binding::Parameter(binding.into()));
        assert!(
            session
                .transact(&[ModelEdit::Operations {
                    value: Some(operations)
                }])
                .is_err()
        );
        assert_eq!(*session.model(), model);
        assert_eq!(serde_json::to_value(session.snapshot()).unwrap(), before);
    }
}

#[test]
fn seat_normalization_plugin_rejects_zero_seats_without_publishing_fake_casm() {
    let mut session = accounting_session();
    session.run().unwrap();
    let mut analysis = session.model().analyses["accounting"].clone();
    analysis.plugin = "vehicle.operations".into();
    analysis.outputs.remove("cost_per_hour");
    analysis.outputs.insert("casm".into(), "casm".into());
    let mut parameter = session.model().parameters["cost_per_hour"].clone();
    parameter.unit = Unit::DollarPerNauticalMile;
    session
        .transact(&[
            ModelEdit::Parameter {
                id: "casm".into(),
                value: Some(parameter),
            },
            ModelEdit::Analysis {
                id: "accounting".into(),
                value: Some(analysis),
            },
            ModelEdit::Parameter {
                id: "cost_per_hour".into(),
                value: None,
            },
        ])
        .unwrap();
    let snapshot = session.run().unwrap();
    assert!(!snapshot.complete());
    assert_eq!(snapshot.values["casm"].quantity, None);
    assert_eq!(snapshot.values["casm"].status, ValueStatus::Failed);
    assert!(
        snapshot
            .runs
            .last()
            .unwrap()
            .message
            .as_ref()
            .unwrap()
            .contains("use vehicle.trip_cost")
    );
}

#[test]
fn zero_seat_accounting_keeps_airport_failure_as_a_hard_requirement() {
    let mut session = accounting_session();
    let mut operations = session.model().operations.clone().unwrap();
    operations.span = Some(Binding::Constant(Quantity::new(37.0, Unit::Meter).unwrap()));
    operations.required_runway = Some(Binding::Constant(
        Quantity::new(1000.0, Unit::Meter).unwrap(),
    ));
    operations.airports = serde_json::from_value(json!([{
        "id":"endpoint","maximum_span":{"value":36,"unit":"m"},
        "runway":{"value":2000,"unit":"m"},"movement_fee":{"value":0,"unit":"USD"}
    }]))
    .unwrap();
    let requirement = serde_json::from_value(json!({
        "name":"Required airport compatibility","metric":"network_compatible","relation":"ge",
        "target":{"value":1,"unit":"1"},"priority":"hard","source":source()
    }))
    .unwrap();
    session
        .transact(&[
            ModelEdit::Operations {
                value: Some(operations),
            },
            ModelEdit::Requirement {
                id: "airport".into(),
                value: Some(requirement),
            },
        ])
        .unwrap();
    let snapshot = session.run().unwrap();
    assert!(snapshot.complete());
    assert_eq!(
        snapshot.values["network_compatible"]
            .quantity
            .unwrap()
            .value,
        0.0
    );
    assert_eq!(
        snapshot.values["cost_per_hour"].quantity.unwrap().value,
        5000.0
    );
    let assessment = ace_engine::assess(session.model(), &snapshot).unwrap();
    assert_eq!(assessment.feasibility, ace_engine::Feasibility::Violated);
}

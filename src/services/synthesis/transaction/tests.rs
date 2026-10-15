use super::*;
use ace_model::{Binding, ModelEdit, Priority};

fn session() -> Session {
    let model = serde_json::from_str(include_str!(
        "../../../../examples/synthesis/transport.json"
    ))
    .unwrap();
    create_session_blocking(model, &BTreeMap::new()).unwrap()
}

fn number(snapshot: &Snapshot, id: &str) -> f64 {
    snapshot.values[id].quantity.unwrap().value
}

#[test]
fn operations_transaction_reuses_physics_and_requirement_edits_do_not_run_solvers() {
    let mut session = session();
    let baseline = session.run().unwrap();
    let mut operations = session.model().operations.clone().unwrap();
    operations.costs.push(ace_model::CostItem {
        id: "navigation".into(),
        category: "navigation".into(),
        amount: Binding::Constant(Quantity::new(1_000.0, Unit::Dollar).unwrap()),
        basis: None,
    });
    session
        .transact(&[ModelEdit::Operations {
            value: Some(operations),
        }])
        .unwrap();
    let result = session.run().unwrap();
    assert_eq!(result.executed, ["analysis:economics"]);
    assert!(
        (number(&result, "economics.trip_cost")
            - number(&baseline, "economics.trip_cost")
            - 1_000.0)
            .abs()
            < 1e-7
    );
    assert_eq!(
        result.values["weight.mtow"].run_id,
        baseline.values["weight.mtow"].run_id
    );
    let mut requirement = session.model().requirements["gate_span"].clone();
    requirement.target = Quantity::new(34.0, Unit::Meter).unwrap();
    session
        .transact(&[ModelEdit::Requirement {
            id: "gate_span".into(),
            value: Some(requirement.clone()),
        }])
        .unwrap();
    let result = session.run().unwrap();
    assert!(result.executed.is_empty());
    let assessment = ace_engine::assess(session.model(), &result).unwrap();
    assert_eq!(assessment.execution, ace_engine::ExecutionStatus::Complete);
    assert_eq!(assessment.feasibility, ace_engine::Feasibility::Violated);
    requirement.priority = Priority::Soft;
    session
        .transact(&[ModelEdit::Requirement {
            id: "gate_span".into(),
            value: Some(requirement),
        }])
        .unwrap();
    let result = session.run().unwrap();
    assert_eq!(
        ace_engine::assess(session.model(), &result)
            .unwrap()
            .feasibility,
        ace_engine::Feasibility::Provisional
    );
}

#[test]
fn mission_stage_changes_recompute_fuel_closure_and_invalid_removals_roll_back() {
    let mut session = session();
    let baseline = session.run().unwrap();
    let mut mission = session.model().missions["design"].clone();
    let mut extra = mission.stages[0].clone();
    extra.id = "additional_taxi".into();
    mission.stages.insert(0, extra);
    session
        .transact(&[ModelEdit::Mission {
            id: "design".into(),
            value: Some(mission.clone()),
        }])
        .unwrap();
    assert!(
        session.snapshot().values["fuel.required"]
            .quantity
            .is_none()
    );
    let result = session.run().unwrap();
    assert!(result.complete());
    assert!(number(&result, "fuel.required") > number(&baseline, "fuel.required"));
    assert_eq!(
        result.values["phase.taxi.fuel_burn"].run_id,
        baseline.values["phase.taxi.fuel_burn"].run_id
    );
    mission.stages.retain(|stage| stage.id != "cruise");
    let before = serde_json::to_value(session.snapshot()).unwrap();
    assert!(
        session
            .transact(&[ModelEdit::Mission {
                id: "design".into(),
                value: Some(mission)
            }])
            .is_err()
    );
    assert_eq!(before, serde_json::to_value(session.snapshot()).unwrap());
    assert!(
        session
            .transact(&[ModelEdit::MassItem {
                id: "fuel".into(),
                value: None
            }])
            .is_err()
    );
    assert!(
        session
            .transact(&[ModelEdit::Geometry {
                id: "tank".into(),
                value: None
            }])
            .is_err()
    );
    assert_eq!(before, serde_json::to_value(session.snapshot()).unwrap());
}

#[test]
fn structural_geometry_and_mass_edits_revoke_consumers_but_keep_independent_phase_runs() {
    let mut session = session();
    let baseline = session.run().unwrap();
    let mut geometry = session.model().geometry["tank"].clone();
    geometry.volume = Some(Binding::Constant(
        Quantity::new(30.0, Unit::CubicMeter).unwrap(),
    ));
    let mut mass = session.model().mass_items["payload"].clone();
    mass.cg[0] = Binding::Constant(Quantity::new(14.0, Unit::Meter).unwrap());
    session
        .transact(&[
            ModelEdit::Geometry {
                id: "tank".into(),
                value: Some(geometry),
            },
            ModelEdit::MassItem {
                id: "payload".into(),
                value: Some(mass),
            },
        ])
        .unwrap();
    assert!(session.snapshot().values["loading.cg_x"].quantity.is_none());
    let result = session.run().unwrap();
    assert!(result.complete());
    assert_eq!(number(&result, "fuel.capacity"), 24_000.0);
    assert!(number(&result, "loading.cg_x") > number(&baseline, "loading.cg_x"));
    assert_eq!(
        result.values["phase.taxi.fuel_burn"].run_id,
        baseline.values["phase.taxi.fuel_burn"].run_id
    );
}

#[test]
fn installing_a_missing_phase_adapter_retries_its_mission_and_sizing_consumers() {
    let model = session().model().clone();
    let mut registry = Registry::default();
    ace_physics::register(&mut registry).unwrap();
    registry
        .register(Arc::new(weight::WeightPlugin::new()))
        .unwrap();
    let mut session = Session::new(model, registry).unwrap();
    let unavailable = session.run().unwrap();
    assert!(!unavailable.complete());
    session
        .register(Arc::new(flight::FlightPhase::new()))
        .unwrap();
    let available = session.run().unwrap();
    assert!(available.complete());
    assert!(
        available
            .executed
            .contains(&"analysis:fuel_requirement".into())
    );
    assert_eq!(
        available.values["phase.taxi.fuel_burn"].run_id,
        unavailable.values["phase.taxi.fuel_burn"].run_id
    );
}

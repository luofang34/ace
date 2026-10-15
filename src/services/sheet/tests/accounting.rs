use super::*;

#[test]
fn hourly_cost_projection_uses_current_runs_and_preserves_trace_and_replay() {
    let model: ace_model::VehicleModel = serde_json::from_str(include_str!(
        "../../../../examples/synthesis/trip-cost.json"
    ))
    .unwrap();
    let mut session =
        super::super::super::synthesis::create_session_blocking(model, &BTreeMap::new()).unwrap();
    let before = session.run().unwrap();
    let sheet = ace_sheet::project(session.model(), &before).unwrap();
    let panel = &sheet.panels[0];
    assert_eq!(panel.plugin, "vehicle.trip_cost");
    assert!(!panel.historical);
    assert_eq!(panel.details["cost_per_block_hour_usd"], 5000.0);
    assert!(panel.details["casm_usd_per_nmi"].is_null());
    let original_run = panel.run_id.clone();
    session
        .edit(
            &BTreeMap::from([(
                "duration".into(),
                ace_model::Quantity::new(4.0, ace_model::Unit::Hour).unwrap(),
            )]),
            ace_model::Source {
                kind: "test".into(),
                reference: "Elapsed time correction".into(),
                revision: None,
            },
        )
        .unwrap();
    let pending = ace_sheet::project(session.model(), &session.snapshot()).unwrap();
    assert!(pending.panels[0].historical);
    assert_eq!(pending.snapshot.values["cost_per_hour"].quantity, None);
    let snapshot = session.run().unwrap();
    let current = ace_sheet::project(session.model(), &snapshot).unwrap();
    assert_eq!(current.revision, snapshot.revision.to_string());
    assert!(!current.panels[0].historical);
    assert_ne!(current.panels[0].run_id, original_run);
    assert_eq!(current.panels[0].details["cost_per_block_hour_usd"], 2500.0);
    assert!(
        current
            .snapshot
            .runs
            .iter()
            .any(|run| run.id == current.panels[0].run_id)
    );
    let serialized = serde_json::to_string(session.model()).unwrap();
    let restored = serde_json::from_str(&serialized).unwrap();
    let mut replay =
        super::super::super::synthesis::create_session_blocking(restored, &BTreeMap::new())
            .unwrap();
    let replay = replay.run().unwrap();
    assert_eq!(replay.model_digest, snapshot.model_digest);
    assert_eq!(
        replay.values["cost_per_hour"].quantity,
        snapshot.values["cost_per_hour"].quantity
    );
}

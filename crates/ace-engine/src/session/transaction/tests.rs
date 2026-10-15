use super::*;
use ace_model::{Expression, Quantity, Unit};

fn session() -> Session {
    let model = serde_json::from_value(serde_json::json!({
        "schema_version":1,"id":"transaction","name":"Transaction","domain":"test",
        "parameters":{
            "input":{"unit":"m","value":{"value":2,"unit":"m"},"owner":"test","source":{"kind":"test","reference":"fixture"}},
            "output":{"unit":"m","expression":{"op":"reference","parameter":"input"},"owner":"test","source":{"kind":"test","reference":"fixture"}},
            "other":{"unit":"m","expression":{"op":"constant","value":{"value":7,"unit":"m"}},"owner":"test","source":{"kind":"test","reference":"fixture"}}
        }})).unwrap();
    Session::new(model, crate::Registry::default()).unwrap()
}

#[test]
fn failed_recompile_preserves_model_values_history_and_revision() {
    let mut session = session();
    session.run().unwrap();
    let before = serde_json::to_value(session.snapshot()).unwrap();
    let mut parameter = session.model.parameters["input"].clone();
    parameter.expression = Some(Expression::Reference {
        parameter: "output".into(),
    });
    assert!(
        session
            .transact(&[ModelEdit::Parameter {
                id: "input".into(),
                value: Some(parameter)
            }])
            .is_err()
    );
    assert_eq!(session.model.revision, 0);
    assert_eq!(before, serde_json::to_value(session.snapshot()).unwrap());
    assert!(
        session
            .transact(&[ModelEdit::Parameter {
                id: "input".into(),
                value: None
            }])
            .is_err()
    );
    assert_eq!(before, serde_json::to_value(session.snapshot()).unwrap());
}

#[test]
fn successful_recompile_reuses_unaffected_runs_and_revokes_changed_producers() {
    let mut session = session();
    let before = session.run().unwrap();
    let mut parameter = session.model.parameters["output"].clone();
    parameter.expression = Some(Expression::Constant {
        value: Quantity::new(9.0, Unit::Meter).unwrap(),
    });
    session
        .transact(&[ModelEdit::Parameter {
            id: "output".into(),
            value: Some(parameter),
        }])
        .unwrap();
    assert_eq!(session.snapshot().values["output"].quantity, None);
    let after = session.run().unwrap();
    assert_eq!(after.executed, ["expression:output"]);
    assert_eq!(after.values["output"].quantity.unwrap().value, 9.0);
    assert_eq!(after.values["other"].run_id, before.values["other"].run_id);
    assert_eq!(after.revision, 1);
}

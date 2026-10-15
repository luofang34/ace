use super::*;
use ace_engine::{Registry, Session};
use ace_model::{Quantity, Source, Unit};
use serde_json::json;

fn model() -> VehicleModel {
    serde_json::from_value(json!({"schema_version":1,"id":"trace_test","name":"Trace fixture","domain":"vehicle",
        "parameters":{
            "span":{"unit":"m","value":{"value":3,"unit":"m"},"owner":"test","source":{"kind":"test","reference":"dimension"}},
            "doubled":{"unit":"m","expression":{"op":"multiply","left":{"op":"reference","parameter":"span"},"right":{"op":"constant","value":{"value":2,"unit":"1"}}},"owner":"test","source":{"kind":"expression","reference":"twice span"}}},
        "geometry":{"wing":{"role":"wing","name":"Wing","source":{"kind":"test","reference":"shape"},
            "bounds":{"minimum":[{"value":0,"unit":"m"},{"value":0,"unit":"m"},{"value":0,"unit":"m"}],"maximum":["span",{"value":1,"unit":"m"},{"value":1,"unit":"m"}]}}},
        "requirements":{"size":{"name":"Size constraint","metric":"doubled","relation":"le","target":{"value":7,"unit":"m"},"priority":"hard","source":{"kind":"user","reference":"SRD-1"},"geometry":["wing"]}}})).unwrap()
}

fn edit(session: &mut Session, span: f64) {
    session
        .edit(
            &BTreeMap::from([("span".into(), Quantity::new(span, Unit::Meter).unwrap())]),
            Source {
                kind: "test".into(),
                reference: "edit".into(),
                revision: None,
            },
        )
        .unwrap();
}

#[test]
fn provenance_links_requirements_to_inputs_geometry_runs_and_margins() {
    let mut session = Session::new(model(), Registry::default()).unwrap();
    let snapshot = session.run().unwrap();
    let sheet = project(session.model(), &snapshot).unwrap();
    for (from, to) in [
        ("requirement:size", "parameter:doubled"),
        ("parameter:span", "geometry:wing"),
        ("parameter:span", "expression:doubled"),
        ("parameter:doubled", "margin:size"),
    ] {
        assert!(
            sheet
                .trace
                .links
                .iter()
                .any(|link| link.from == from && link.to == to)
        );
    }
    assert!(
        sheet
            .trace
            .links
            .iter()
            .any(|link| link.from.starts_with("run:") && link.to == "margin:size")
    );
    assert!(sheet.editable.contains("span"));
    assert!(!sheet.editable.contains("doubled"));
    for link in &sheet.trace.links {
        assert!(sheet.trace.nodes.contains_key(&link.from), "{}", link.from);
        assert!(sheet.trace.nodes.contains_key(&link.to), "{}", link.to);
    }
}

#[test]
fn edits_revoke_current_margins_and_unrelated_history_is_not_sent() {
    let mut session = Session::new(model(), Registry::default()).unwrap();
    session.run().unwrap();
    edit(&mut session, 4.0);
    let pending = project(session.model(), &session.snapshot()).unwrap();
    assert_eq!(pending.margins["size"].status, MarginStatus::Unknown);
    assert!(pending.snapshot.values["doubled"].quantity.is_none());
    let snapshot = session.run().unwrap();
    let sheet = project(session.model(), &snapshot).unwrap();
    assert_eq!(sheet.margins["size"].status, MarginStatus::Fail);
    assert_eq!(sheet.snapshot.runs.len(), 1);
    assert_eq!(snapshot.runs.len(), 2);
}

#[test]
fn browser_revision_tokens_preserve_full_u64_range() {
    let mut model = model();
    model.revision = u64::MAX;
    let mut session = Session::new(model, Registry::default()).unwrap();
    let snapshot = session.run().unwrap();
    assert_eq!(
        project(session.model(), &snapshot).unwrap().revision,
        u64::MAX.to_string()
    );
    edit(&mut session, 4.0);
    assert_eq!(session.model().revision, 0);
}

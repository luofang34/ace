use crate::{AnalysisResources, ModelEdit, VehicleModel};
use serde_json::json;

fn model(selection: serde_json::Value) -> VehicleModel {
    let source = json!({"kind":"test","reference":"catalog fixture"});
    serde_json::from_value(json!({"schema_version":1,"id":"catalogued","name":"Catalog fixture","domain":"vehicle",
        "parameters":{"power":{"unit":"W","source":source,"owner":"test"}},
        "analyses":{"engine":{"mode":"scalar","plugin":"test.engine","version":"1","inputs":{},
            "outputs":{"power":"power"},"resources":{"catalog":selection}}},
        "catalog":{
            "piston":{"kind":"piston_engine","version":1,"source":source,"document":{"rated_power":"134 kW"}},
            "spare":{"kind":"propeller","version":1,"source":source,"document":{"diameter":"1.9 m"}}},
        "operations":null}))
    .unwrap()
}

#[test]
fn analyses_receive_only_the_catalog_entries_they_select() {
    let model = model(json!(["piston"]));
    model.validate().unwrap();
    let resources = AnalysisResources::collect(&model, &model.analyses["engine"]).unwrap();
    assert_eq!(resources.catalog.keys().collect::<Vec<_>>(), ["piston"]);
    assert_eq!(
        resources.catalog["piston"].document["rated_power"],
        "134 kW"
    );
}

#[test]
fn unknown_selections_and_malformed_entries_are_rejected() {
    assert!(model(json!(["missing"])).validate().is_err());
    let mut malformed = model(json!([]));
    malformed.catalog.get_mut("spare").unwrap().version = 0;
    assert!(malformed.validate().is_err());
    let mut scalar = model(json!([]));
    scalar.catalog.get_mut("spare").unwrap().document = json!("not an object");
    assert!(scalar.validate().is_err());
}

#[test]
fn catalog_edits_replace_and_remove_entries_and_empty_catalogs_are_omitted() {
    let mut model = model(json!([]));
    let mut entry = model.catalog["spare"].clone();
    entry.version = 2;
    ModelEdit::CatalogEntry {
        id: "spare".into(),
        value: Some(entry),
    }
    .apply(&mut model)
    .unwrap();
    assert_eq!(model.catalog["spare"].version, 2);
    for id in ["spare", "piston"] {
        ModelEdit::CatalogEntry {
            id: id.into(),
            value: None,
        }
        .apply(&mut model)
        .unwrap();
    }
    let encoded = serde_json::to_value(&model).unwrap();
    assert!(encoded.get("catalog").is_none());
    assert!(
        encoded["analyses"]["engine"]["resources"]
            .get("catalog")
            .is_none()
    );
}

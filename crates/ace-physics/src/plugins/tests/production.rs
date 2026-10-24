use ace_engine::{Registry, Session, ValueStatus};
use ace_model::{Quantity, Source, Unit, VehicleModel};
use serde_json::json;
use std::collections::BTreeMap;

fn source() -> Source {
    Source {
        kind: "test".into(),
        reference: "illustrative production fixture".into(),
        revision: None,
    }
}

fn input(value: f64, unit: &str) -> serde_json::Value {
    json!({"unit":unit,"value":{"value":value,"unit":unit},"source":source(),"owner":"manufacturing"})
}

fn output(unit: &str) -> serde_json::Value {
    json!({"unit":unit,"source":source(),"owner":"manufacturing"})
}

fn model(selected: bool) -> VehicleModel {
    serde_json::from_value(json!({"schema_version":1,"id":"airframe","name":"Production fixture","domain":"vehicle",
        "parameters":{"wing.mass":input(10.0,"kg"),"build.quantity":input(100.0,"1"),
            "cost.unit":output("USD"),"cost.recurring":output("USD"),"cost.first":output("USD"),
            "cost.hours":output("h"),"cost.structure":output("kg")},
        "mass_items":{"wing":{"kind":"empty","mass":"wing.mass","cg":[{"value":0,"unit":"m"},{"value":0,"unit":"m"},{"value":0,"unit":"m"}],"source":source()}},
        "production":{
            "materials":{"cfrp":{"cost_per_kg":{"value":40,"unit":"USD/kg"},"buy_to_fly":{"value":1.5,"unit":"1"}}},
            "processes":{"layup":{"hours_per_kg":{"value":2,"unit":"h/kg"},"tooling":{"value":10000,"unit":"USD"},"learning":{"value":0.85,"unit":"1"}}},
            "parts":{"wing":{"mass_item":"wing","material":"cfrp","process":"layup"}},
            "labor_rate":{"value":60,"unit":"USD/h"},"quantity":"build.quantity"},
        "analyses":{"cost":{"plugin":"vehicle.production_cost","version":"1","inputs":{},
            "resources":{"production":selected},
            "outputs":{"unit_cost":"cost.unit","recurring_unit_cost":"cost.recurring","first_unit_cost":"cost.first",
                "labor_hours":"cost.hours","structure_mass":"cost.structure"}}}}))
    .unwrap()
}

fn session(model: VehicleModel) -> Session {
    let mut registry = Registry::default();
    crate::register(&mut registry).unwrap();
    Session::new(model, registry).unwrap()
}

#[test]
fn production_cost_is_an_approximate_graph_output_that_tracks_mass_and_quantity() {
    let mut session = session(model(true));
    let first = session.run().unwrap();
    let unit = first.values["cost.unit"].clone();
    assert_eq!(unit.status, ValueStatus::Approximate);
    assert_eq!(first.values["cost.structure"].quantity.unwrap().value, 10.0);
    assert_eq!(
        first.values["cost.first"].quantity.unwrap().value,
        10.0 * 1.5 * 40.0 + 20.0 * 60.0
    );
    let edit = |name: &str, value: f64, unit: Unit| {
        BTreeMap::from([(name.to_owned(), Quantity::new(value, unit).unwrap())])
    };
    let affected = session
        .edit(&edit("wing.mass", 12.0, Unit::Kilogram), source())
        .unwrap();
    assert!(affected.iter().any(|node| node.contains("cost")));
    let heavier = session.run().unwrap();
    assert!(heavier.values["cost.unit"].quantity.unwrap().value > unit.quantity.unwrap().value);
    session
        .edit(&edit("build.quantity", 1000.0, Unit::Ratio), source())
        .unwrap();
    let larger = session.run().unwrap();
    assert!(
        larger.values["cost.unit"].quantity.unwrap().value
            < heavier.values["cost.unit"].quantity.unwrap().value
    );
    let run = larger
        .runs
        .iter()
        .rev()
        .find(|run| run.node.contains("cost"))
        .unwrap();
    assert!(run.resources.production.is_some());
    assert!(
        run.details["assumptions"]
            .as_str()
            .unwrap()
            .contains("Wright")
    );
}

#[test]
fn an_unselected_bill_leaves_cost_unavailable_instead_of_guessing() {
    let snapshot = session(model(false)).run().unwrap();
    assert!(!snapshot.values["cost.unit"].usable());
    let mut absent = model(true);
    absent.production = None;
    let mut registry = Registry::default();
    crate::register(&mut registry).unwrap();
    assert!(Session::new(absent, registry).is_err());
}

#[test]
fn production_bills_that_reference_unknown_entries_are_invalid() {
    let mut broken = model(true);
    if let Some(bill) = broken.production.as_mut() {
        bill.parts.get_mut("wing").unwrap().process = "milling".into();
    }
    assert!(broken.validate().is_err());
    let serialized = serde_json::to_value(model(true)).unwrap();
    assert!(
        serialized["analyses"]["cost"]["resources"]["production"]
            .as_bool()
            .unwrap()
    );
    let mut plain = model(true);
    plain.production = None;
    plain.analyses.clear();
    assert!(
        serde_json::to_value(plain)
            .unwrap()
            .get("production")
            .is_none()
    );
}

#[test]
fn catalog_choices_are_typed_transactions_that_reprice_the_vehicle() {
    use ace_model::{Material, ModelEdit, Part};
    let mut session = session(model(true));
    let glass = session.run().unwrap().values["cost.unit"].quantity.unwrap();
    let mut bill = session.model().production.clone().unwrap();
    bill.materials.insert(
        "carbon".into(),
        Material {
            cost_per_kg: ace_model::Binding::Constant(
                Quantity::new(120.0, Unit::DollarPerKilogram).unwrap(),
            ),
            buy_to_fly: ace_model::Binding::Constant(Quantity::new(1.5, Unit::Ratio).unwrap()),
        },
    );
    let carbon_wing = Part {
        mass_item: "wing".into(),
        material: "carbon".into(),
        process: "layup".into(),
        count: 1,
    };
    let affected = session
        .transact(&[
            ModelEdit::Production { value: Some(bill) },
            ModelEdit::ProductionPart {
                id: "wing".into(),
                value: Some(carbon_wing),
            },
        ])
        .unwrap();
    assert!(affected.iter().any(|node| node.contains("cost")));
    let carbon = session.run().unwrap().values["cost.unit"].quantity.unwrap();
    assert!((carbon.value - glass.value - 10.0 * 1.5 * 80.0).abs() < 1e-6);
    let unknown = Part {
        mass_item: "wing".into(),
        material: "titanium".into(),
        process: "layup".into(),
        count: 1,
    };
    let revision = session.model().revision;
    assert!(
        session
            .transact(&[ModelEdit::ProductionPart {
                id: "wing".into(),
                value: Some(unknown),
            }])
            .is_err()
    );
    assert_eq!(session.model().revision, revision);
    let mut bare = model(false);
    bare.production = None;
    assert!(
        ModelEdit::ProductionPart {
            id: "wing".into(),
            value: None,
        }
        .apply(&mut bare)
        .is_err()
    );
}

use super::*;
use ace_model::{AnalysisResources, Binding, ModelEdit, Unit, VehicleModel};
use serde_json::json;

fn fixture() -> VehicleModel {
    let q = |value| json!({"value":value,"unit":"m"});
    let source = json!({"kind":"test","reference":"rigid body"});
    let mut geometry = json!({"name":"Tank","role":"fuel_tank","source":source,
        "bounds":{"minimum":[q(0.0),q(0.0),q(0.0)],"maximum":[q(2.0),q(1.0),q(1.0)]},
        "volume":{"value":2,"unit":"m^3"}});
    geometry["placement"] = json!({"relative_to":"rack","translation":[q(2.0),q(0.0),q(0.0)],
        "rotation":[[1,0,0],[0,1,0],[0,0,1]]});
    serde_json::from_value(json!({"schema_version":1,"id":"frames","name":"Frames","domain":"test",
        "parameters":{"volume":{"unit":"m^3","owner":"test","source":source}},
        "geometry":{"tank":geometry,"rack":{"name":"Rack","role":"structure","source":source,
            "placement":{"translation":[q(10.0),q(0.0),q(0.0)],"rotation":[[0,-1,0],[1,0,0],[0,0,1]]}},
            "obstacle":{"name":"Obstacle","role":"structure","source":source,
            "bounds":{"minimum":[q(12.0),q(2.0),q(0.0)],"maximum":[q(13.0),q(4.0),q(1.0)]}}},
        "mass_items":{"load":{"kind":"empty","mass":{"value":10,"unit":"kg"},
            "cg":[q(1.0),q(0.5),q(0.5)],"frame":"tank","source":source,
            "inertia":{"xx":{"value":1,"unit":"kg*m^2"},"yy":{"value":2,"unit":"kg*m^2"},
                "zz":{"value":3,"unit":"kg*m^2"},"xy":{"value":0,"unit":"kg*m^2"},
                "xz":{"value":0,"unit":"kg*m^2"},"yz":{"value":0,"unit":"kg*m^2"}}}},
        "clearances":{"gap":{"first":"tank","second":"obstacle","minimum":q(0.0)}},
        "sections":{"station":{"axis":0,"station":q(9.5),"geometry":["tank"],"reason":"test station"}},
        "analyses":{"volume":{"plugin":"vehicle.usable_volume","version":"1","inputs":{},
            "outputs":{"volume":"volume"},"geometry":["tank"]}}
    })).unwrap()
}

fn resources(model: &VehicleModel) -> AnalysisResources {
    let mut analysis = model.analyses["volume"].clone();
    analysis.geometry.push("obstacle".into());
    analysis.resources.mass_items.push("load".into());
    analysis.resources.clearances.push("gap".into());
    analysis.resources.sections.push("station".into());
    AnalysisResources::collect(model, &analysis).unwrap()
}

#[test]
fn geometry_cg_tensor_clearance_and_sections_share_composed_frames() {
    let mut model = fixture();
    model.validate().unwrap();
    let selected = resources(&model);
    assert!(!selected.geometry.contains_key("rack"));
    assert!(selected.frames.contains_key("rack"));
    let report = crate::evaluate_geometry(&selected, &BTreeMap::new()).unwrap();
    let mass = crate::evaluate_mass(&selected, &BTreeMap::new()).unwrap();
    assert_eq!(report.envelopes["tank"].minimum_m, [9.0, 2.0, 0.0]);
    assert_eq!(report.envelopes["tank"].maximum_m, [10.0, 4.0, 1.0]);
    assert_eq!(report.clearances["gap"].clearance_m, 2.0);
    assert_eq!(mass.current.cg_m, [9.5, 3.0, 0.5]);
    assert_eq!(
        mass.current.inertia_kg_m2.unwrap(),
        [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 3.0]]
    );
    assert_eq!(
        report
            .sections
            .iter()
            .find(|section| section.id == "station")
            .unwrap()
            .slices
            .len(),
        1
    );
    model
        .geometry
        .get_mut("rack")
        .unwrap()
        .placement
        .translation[0] = Binding::Constant(Quantity::new(11.0, Unit::Meter).unwrap());
    let selected = resources(&model);
    let moved = crate::evaluate_geometry(&selected, &BTreeMap::new()).unwrap();
    assert_eq!(moved.clearances["gap"].clearance_m, 1.0);
    assert!(
        moved
            .sections
            .iter()
            .find(|section| section.id == "station")
            .unwrap()
            .slices
            .is_empty()
    );
    assert_eq!(
        crate::evaluate_mass(&selected, &BTreeMap::new())
            .unwrap()
            .current
            .cg_m,
        [10.5, 3.0, 0.5]
    );
}

#[test]
fn frame_dependencies_invalidate_without_counting_parent_volume_and_bad_frames_roll_back() {
    let mut registry = ace_engine::Registry::default();
    crate::register(&mut registry).unwrap();
    let mut session = ace_engine::Session::new(fixture(), registry).unwrap();
    let first = session.run().unwrap();
    assert_eq!(first.values["volume"].quantity.unwrap().value, 2.0);
    let mut rack = session.model().geometry["rack"].clone();
    rack.placement.translation[0] = Binding::Constant(Quantity::new(11.0, Unit::Meter).unwrap());
    session
        .transact(&[ModelEdit::Geometry {
            id: "rack".into(),
            value: Some(rack.clone()),
        }])
        .unwrap();
    let second = session.run().unwrap();
    assert_eq!(second.executed, ["analysis:volume"]);
    assert_ne!(
        second.values["volume"].run_id,
        first.values["volume"].run_id
    );
    rack.placement.relative_to = Some("tank".into());
    assert!(
        session
            .transact(&[ModelEdit::Geometry {
                id: "rack".into(),
                value: Some(rack.clone())
            }])
            .is_err()
    );
    rack.placement.relative_to = None;
    rack.placement.rotation[2][2] = -1.0;
    assert!(
        session
            .transact(&[ModelEdit::Geometry {
                id: "rack".into(),
                value: Some(rack)
            }])
            .is_err()
    );
    assert_eq!(session.snapshot().model_digest, second.model_digest);
}

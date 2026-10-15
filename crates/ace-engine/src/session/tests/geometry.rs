use super::*;
use ace_model::{Binding, Bounds, GeometryObject, GeometryRole};

#[test]
fn geometry_bindings_invalidate_consumers_and_record_exact_geometry() -> crate::EngineResult<()> {
    let mut model = plugin_model();
    model.parameters.insert(
        "tank.length".into(),
        parameter(Some(3.0), Unit::Meter, None),
    );
    let zero = Binding::Constant(q(0.0, Unit::Meter));
    model.geometry.insert(
        "tank".into(),
        GeometryObject {
            role: GeometryRole::FuelTank,
            name: "Tank envelope".into(),
            source: source(),
            revision: 4,
            parent: None,
            placement: Default::default(),
            shape: None,
            bounds: Some(Bounds {
                minimum: [zero.clone(), zero.clone(), zero],
                maximum: [
                    Binding::Parameter("tank.length".into()),
                    Binding::Constant(q(1.0, Unit::Meter)),
                    Binding::Constant(q(1.0, Unit::Meter)),
                ],
            }),
            volume: None,
            wetted_area: None,
            assets: vec!["sha256:tank-mesh".into()],
            artifacts: Vec::new(),
        },
    );
    model
        .analyses
        .get_mut("structure")
        .expect("fixture")
        .geometry = vec!["tank".into()];
    let mut session = Session::new(model, Registry::default())?;
    session.register(plugin(false, false))?;
    session.run()?;
    let changed = session.edit(
        &BTreeMap::from([("tank.length".into(), q(4.0, Unit::Meter))]),
        source(),
    )?;
    assert_eq!(changed.len(), 2);
    let result = session.run()?;
    let run = result
        .runs
        .iter()
        .rev()
        .find(|run| run.node == "analysis:structure")
        .expect("solver run");
    assert_eq!(
        run.inputs["tank.length"].quantity.expect("length").value,
        4.0
    );
    assert_eq!(run.resources.geometry["tank"].revision, 4);
    assert_eq!(run.resources.geometry["tank"].assets, ["sha256:tank-mesh"]);
    let mut geometry = session.model().geometry["tank"].clone();
    geometry.artifacts.push(ace_model::Artifact::from_text(
        "mesh bytes".into(),
        "text/plain",
        "fixture.mesh/1",
        plugin(false, false).spec().identity.generator(),
    ));
    session.transact(&[ace_model::ModelEdit::Geometry {
        id: "tank".into(),
        value: Some(geometry.clone()),
    }])?;
    assert_eq!(session.model().geometry["tank"].revision, 5);
    assert!(session.snapshot().values["structure"].quantity.is_none());
    assert_eq!(session.run()?.executed.len(), 2);
    geometry.artifacts[0].text = Some("changed without updated digest".into());
    let before = session.snapshot();
    assert!(
        session
            .transact(&[ace_model::ModelEdit::Geometry {
                id: "tank".into(),
                value: Some(geometry)
            }])
            .is_err()
    );
    assert_eq!(session.snapshot().model_digest, before.model_digest);
    Ok(())
}

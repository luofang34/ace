use super::*;

#[test]
fn installing_a_missing_solver_retries_the_entire_coupled_component() -> crate::EngineResult<()> {
    let mut model = plugin_model();
    let payload = model.parameters.get_mut("payload").expect("payload");
    payload.value = Some(q(100.0, Unit::Kilogram));
    payload.expression = Some(Expression::Add {
        left: Box::new(multiply("structure", 0.25)),
        right: Box::new(Expression::Constant {
            value: q(50.0, Unit::Kilogram),
        }),
    });
    model
        .parameters
        .get_mut("structure")
        .expect("structure")
        .value = Some(q(100.0, Unit::Kilogram));
    model
        .closures
        .insert("coupled".into(), closure(&["payload", "structure"], 100));
    let mut session = Session::new(model, Registry::default())?;
    assert!(!session.run()?.complete());
    session.register(plugin(false, false))?;
    let result = session.run()?;
    assert!(result.complete());
    assert!((value(&result, "structure") - 200.0).abs() < 1e-4);
    assert_eq!(result.executed.len(), 3);
    Ok(())
}

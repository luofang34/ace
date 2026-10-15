use crate::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec, Registry,
    Session, SolverError, ValueStatus,
};
use ace_model::{
    Analysis, Closure, Expression, Parameter, Quantity, Source, Tolerance, Unit, VehicleModel,
};
use std::collections::BTreeMap;
use std::sync::Arc;
mod coupled_solver;
mod geometry;
#[path = "preview/tests.rs"]
mod preview;

fn q(value: f64, unit: Unit) -> Quantity {
    Quantity::new(value, unit).expect("finite fixture")
}
fn source() -> Source {
    Source {
        kind: "user".into(),
        reference: "test fixture".into(),
        revision: None,
    }
}
fn reference(name: &str) -> Expression {
    Expression::Reference {
        parameter: name.into(),
    }
}
fn multiply(name: &str, factor: f64) -> Expression {
    Expression::Multiply {
        left: Box::new(reference(name)),
        right: Box::new(Expression::Constant {
            value: q(factor, Unit::Ratio),
        }),
    }
}
fn parameter(value: Option<f64>, unit: Unit, expression: Option<Expression>) -> Parameter {
    Parameter {
        unit,
        value: value.map(|value| q(value, unit)),
        expression,
        source: source(),
        owner: "design".into(),
        revision: 0,
        uncertainty: None,
        lower: None,
        upper: None,
    }
}
fn model(parameters: BTreeMap<String, Parameter>) -> VehicleModel {
    VehicleModel {
        schema_version: 1,
        id: "test".into(),
        name: "Test vehicle".into(),
        domain: "generic".into(),
        revision: 0,
        parameters,
        analyses: BTreeMap::new(),
        closures: BTreeMap::new(),
        requirements: BTreeMap::new(),
        geometry: BTreeMap::new(),
        clearances: BTreeMap::new(),
        sections: BTreeMap::new(),
        mass_items: BTreeMap::new(),
        loading_cases: BTreeMap::new(),
        missions: BTreeMap::new(),
        operations: None,
        objectives: BTreeMap::new(),
        design_variables: Vec::new(),
    }
}
fn closure(names: &[&str], iterations: u32) -> Closure {
    Closure {
        tolerances: names
            .iter()
            .map(|name| {
                (
                    (*name).into(),
                    Tolerance {
                        absolute: q(1e-6, Unit::Kilogram),
                        relative: 1e-8,
                    },
                )
            })
            .collect(),
        max_iterations: iterations,
        relaxation: 1.0,
    }
}
fn value(snapshot: &crate::Snapshot, name: &str) -> f64 {
    snapshot.values[name]
        .quantity
        .expect("computed value")
        .value
}

#[test]
fn span_edit_recomputes_only_affected_region() -> crate::EngineResult<()> {
    let ar = Expression::Divide {
        left: Box::new(Expression::Power {
            base: Box::new(reference("wing.span")),
            exponent: 2,
        }),
        right: Box::new(reference("wing.area")),
    };
    let model = model(BTreeMap::from([
        ("wing.span".into(), parameter(Some(35.0), Unit::Meter, None)),
        (
            "wing.area".into(),
            parameter(Some(100.0), Unit::SquareMeter, None),
        ),
        ("wing.ar".into(), parameter(None, Unit::Ratio, Some(ar))),
        (
            "payload".into(),
            parameter(Some(100.0), Unit::Kilogram, None),
        ),
        (
            "payload.double".into(),
            parameter(None, Unit::Kilogram, Some(multiply("payload", 2.0))),
        ),
    ]));
    let mut session = Session::new(model, Registry::default())?;
    let before = session.run()?;
    assert!(before.complete());
    assert_eq!(value(&before, "wing.ar"), 12.25);
    assert!(session.run()?.executed.is_empty());
    let affected = session.edit(
        &BTreeMap::from([("wing.span".into(), q(37.0, Unit::Meter))]),
        source(),
    )?;
    assert_eq!(affected, ["expression:wing.ar"]);
    assert_eq!(
        session.snapshot().values["wing.ar"].status,
        ValueStatus::Stale
    );
    assert!(session.snapshot().values["wing.ar"].quantity.is_none());
    let after = session.run()?;
    assert!((value(&after, "wing.ar") - 13.69).abs() < 1e-10);
    assert_eq!(
        after.values["payload.double"].run_id,
        before.values["payload.double"].run_id
    );
    assert_eq!(after.revision, 1);
    Ok(())
}

#[test]
fn invalid_edit_rolls_back_every_input() -> crate::EngineResult<()> {
    let mut session = Session::new(
        model(BTreeMap::from([
            ("a".into(), parameter(Some(1.0), Unit::Kilogram, None)),
            ("b".into(), parameter(Some(2.0), Unit::Kilogram, None)),
        ])),
        Registry::default(),
    )?;
    let before = session.snapshot();
    let updates = BTreeMap::from([
        ("a".into(), q(2.0, Unit::Kilogram)),
        ("b".into(), q(3.0, Unit::Meter)),
    ]);
    assert!(session.edit(&updates, source()).is_err());
    assert_eq!(session.snapshot().values, before.values);
    assert_eq!(session.model().revision, 0);
    Ok(())
}

#[test]
fn dimension_failure_blocks_downstream_and_does_not_publish_seeds() -> crate::EngineResult<()> {
    let model = model(BTreeMap::from([
        ("length".into(), parameter(Some(1.0), Unit::Meter, None)),
        (
            "mass".into(),
            parameter(Some(10.0), Unit::Kilogram, Some(multiply("length", 2.0))),
        ),
        (
            "total".into(),
            parameter(None, Unit::Kilogram, Some(multiply("mass", 2.0))),
        ),
    ]));
    let result = Session::new(model, Registry::default())?.run()?;
    assert_eq!(result.values["mass"].status, ValueStatus::Failed);
    assert!(result.values["mass"].quantity.is_none());
    assert_eq!(result.values["total"].status, ValueStatus::Blocked);
    assert!(!result.complete());
    Ok(())
}

#[test]
fn weight_cycle_converges_then_responds_to_payload_edit() -> crate::EngineResult<()> {
    let mut model = model(BTreeMap::from([
        (
            "payload".into(),
            parameter(Some(100.0), Unit::Kilogram, None),
        ),
        (
            "mtow".into(),
            parameter(
                Some(200.0),
                Unit::Kilogram,
                Some(Expression::Add {
                    left: Box::new(reference("payload")),
                    right: Box::new(reference("empty")),
                }),
            ),
        ),
        (
            "empty".into(),
            parameter(Some(100.0), Unit::Kilogram, Some(multiply("mtow", 0.5))),
        ),
    ]));
    model
        .closures
        .insert("weight".into(), closure(&["mtow", "empty"], 100));
    let mut session = Session::new(model, Registry::default())?;
    assert!(session.run()?.closures["weight"].converged);
    session.edit(
        &BTreeMap::from([("payload".into(), q(150.0, Unit::Kilogram))]),
        source(),
    )?;
    let result = session.run()?;
    assert!(result.complete());
    assert!((value(&result, "mtow") - 300.0).abs() < 1e-4);
    assert!(result.closures["weight"].iterations > 1);
    assert!(result.runs.iter().any(|run| run.node == "closure:weight"));
    Ok(())
}

#[test]
fn nonconvergence_cannot_pass_using_small_relaxation() -> crate::EngineResult<()> {
    let mut model = model(BTreeMap::from([
        (
            "x".into(),
            parameter(Some(1.0), Unit::Kilogram, Some(multiply("x", 2.0))),
        ),
        (
            "downstream".into(),
            parameter(None, Unit::Kilogram, Some(reference("x"))),
        ),
    ]));
    let mut settings = closure(&["x"], 3);
    settings.relaxation = 0.001;
    model.closures.insert("loop".into(), settings);
    let result = Session::new(model, Registry::default())?.run()?;
    assert_eq!(result.values["x"].status, ValueStatus::Nonconverged);
    assert!(result.values["x"].quantity.is_none());
    assert_eq!(result.values["downstream"].status, ValueStatus::Blocked);
    assert!(result.closures["loop"].residuals["x"].value > 1.0);
    Ok(())
}

struct Scale {
    spec: PluginSpec,
    approximate: bool,
    wrong_unit: bool,
}
impl AnalysisPlugin for Scale {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let value = inputs
            .get("input")
            .ok_or_else(|| SolverError::Failed("missing input".into()))?;
        let unit = if self.wrong_unit {
            Unit::Meter
        } else {
            Unit::Kilogram
        };
        Ok(PluginOutput {
            values: BTreeMap::from([("output".into(), q(value.value * 2.0, unit))]),
            approximate: self.approximate,
            artifacts: vec![ace_model::Artifact::from_text(
                "test mesh".into(),
                "text/plain",
                "test.mesh/1",
                self.spec.identity.generator(),
            )],
            details: serde_json::Value::Null,
            failure: None,
        })
    }
}
fn plugin(approximate: bool, wrong_unit: bool) -> Arc<dyn AnalysisPlugin> {
    Arc::new(Scale {
        spec: PluginSpec {
            identity: PluginIdentity {
                id: "scale".into(),
                version: "1".into(),
                fidelity: Fidelity::Conceptual,
                code_revision: "fixture-commit".into(),
            },
            inputs: BTreeMap::from([("input".into(), Unit::Kilogram)]),
            outputs: BTreeMap::from([("output".into(), Unit::Kilogram)]),
        },
        approximate,
        wrong_unit,
    })
}
fn plugin_model() -> VehicleModel {
    let mut model = model(BTreeMap::from([
        ("payload".into(), parameter(Some(1.0), Unit::Kilogram, None)),
        ("structure".into(), parameter(None, Unit::Kilogram, None)),
        (
            "total".into(),
            parameter(None, Unit::Kilogram, Some(multiply("structure", 2.0))),
        ),
    ]));
    model.analyses.insert(
        "structure".into(),
        Analysis {
            mode: Default::default(),
            plugin: "scale".into(),
            version: "1".into(),
            inputs: BTreeMap::from([("input".into(), "payload".into())]),
            outputs: BTreeMap::from([("output".into(), "structure".into())]),
            geometry: Vec::new(),
            requirements: Vec::new(),
            resources: Default::default(),
        },
    );
    model
}

#[test]
fn deferred_solver_installation_recovers_only_its_downstream_results() -> crate::EngineResult<()> {
    let mut session = Session::new(plugin_model(), Registry::default())?;
    let missing = session.run()?;
    assert_eq!(missing.values["structure"].status, ValueStatus::Unavailable);
    assert_eq!(missing.values["total"].status, ValueStatus::Blocked);
    session.register(plugin(false, false))?;
    let result = session.run()?;
    assert!(result.complete());
    assert_eq!(value(&result, "total"), 4.0);
    assert_eq!(result.executed.len(), 2);
    assert!(result.runs.iter().any(|run| {
        run.plugin
            .as_ref()
            .is_some_and(|plugin| plugin.code_revision == "fixture-commit")
    }));
    Ok(())
}

#[test]
fn approximate_results_propagate_and_solver_contract_errors_invalidate_outputs()
-> crate::EngineResult<()> {
    let mut session = Session::new(plugin_model(), Registry::default())?;
    session.register(plugin(true, false))?;
    assert_eq!(
        session.run()?.values["total"].status,
        ValueStatus::Approximate
    );
    let mut session = Session::new(plugin_model(), Registry::default())?;
    session.register(plugin(false, true))?;
    let failed = session.run()?;
    assert_eq!(failed.values["structure"].status, ValueStatus::Failed);
    assert_eq!(failed.values["total"].status, ValueStatus::Blocked);
    assert!(failed.values["structure"].quantity.is_none());
    Ok(())
}

#[test]
fn undeclared_cycles_and_duplicate_producers_are_rejected() {
    let cyclic = model(BTreeMap::from([(
        "x".into(),
        parameter(Some(1.0), Unit::Kilogram, Some(reference("x"))),
    )]));
    assert!(Session::new(cyclic, Registry::default()).is_err());
    let mut duplicate = plugin_model();
    duplicate
        .parameters
        .get_mut("structure")
        .expect("fixture")
        .expression = Some(reference("payload"));
    assert!(Session::new(duplicate, Registry::default()).is_err());
}

#[test]
fn historical_inputs_retain_sources_and_revisions() -> crate::EngineResult<()> {
    let mut session = Session::new(plugin_model(), Registry::default())?;
    session.register(plugin(false, false))?;
    session.run()?;
    let edited = Source {
        kind: "reference".into(),
        reference: "new payload specification".into(),
        revision: Some("2".into()),
    };
    session.edit(
        &BTreeMap::from([("payload".into(), q(2.0, Unit::Kilogram))]),
        edited,
    )?;
    let snapshot = session.run()?;
    let histories: Vec<_> = snapshot
        .runs
        .iter()
        .filter_map(|run| run.inputs.get("payload"))
        .collect();
    assert_eq!(histories.len(), 2);
    assert_eq!(histories[0].source.reference, "test fixture");
    assert_eq!(histories[1].revision, 1);
    assert_eq!(histories[1].source.reference, "new payload specification");
    Ok(())
}

#[test]
fn requirements_preserve_direction_conditions_and_missing_evidence() -> crate::EngineResult<()> {
    use crate::MarginStatus;
    use ace_model::{Priority, Relation, Requirement};
    let mut model = plugin_model();
    model.requirements.insert(
        "weight".into(),
        Requirement {
            name: "Structure mass limit".into(),
            metric: "structure".into(),
            relation: Relation::Le,
            target: q(3.0, Unit::Kilogram),
            tolerance: None,
            priority: Priority::Hard,
            source: source(),
            condition: BTreeMap::new(),
            geometry: Vec::new(),
        },
    );
    let mut session = Session::new(model, Registry::default())?;
    let missing = session.run()?;
    assert_eq!(
        crate::evaluate_requirements(session.model(), &missing)?["weight"].status,
        MarginStatus::Unknown
    );
    session.register(plugin(false, false))?;
    let result = session.run()?;
    let margins = crate::evaluate_requirements(session.model(), &result)?;
    assert_eq!(margins["weight"].status, MarginStatus::Pass);
    assert_eq!(margins["weight"].absolute.expect("margin").value, 1.0);
    session.edit(
        &BTreeMap::from([("payload".into(), q(2.0, Unit::Kilogram))]),
        source(),
    )?;
    assert!(crate::evaluate_requirements(session.model(), &result).is_err());
    let result = session.run()?;
    assert_eq!(
        crate::evaluate_requirements(session.model(), &result)?["weight"].status,
        MarginStatus::Fail
    );
    Ok(())
}

#[test]
fn deep_dependency_graph_does_not_recurse_through_graph_nodes() -> crate::EngineResult<()> {
    let mut parameters =
        BTreeMap::from([("p0".into(), parameter(Some(1.0), Unit::Kilogram, None))]);
    for index in 1..1100 {
        parameters.insert(
            format!("p{index}"),
            parameter(
                None,
                Unit::Kilogram,
                Some(reference(&format!("p{}", index - 1))),
            ),
        );
    }
    let result = Session::new(model(parameters), Registry::default())?.run()?;
    assert!(result.complete());
    assert_eq!(value(&result, "p1099"), 1.0);
    Ok(())
}

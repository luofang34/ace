use super::*;
use ace_model::{DomainBound, DomainVariable, ModelEdit, ScalarPort};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Probe {
    spec: PluginSpec,
    calls: Arc<AtomicUsize>,
}

impl AnalysisPlugin for Probe {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(PluginOutput {
            values: BTreeMap::from([(
                "output".into(),
                q(inputs["input"].value * 2.0, Unit::Kilogram),
            )]),
            approximate: false,
            artifacts: vec![],
            details: serde_json::Value::Null,
            failure: None,
        })
    }
}

fn probe(spec: PluginSpec) -> Arc<Probe> {
    Arc::new(Probe {
        spec,
        calls: Arc::new(AtomicUsize::new(0)),
    })
}

#[test]
fn invocation_and_dimension_mismatches_are_rejected_before_calling_solver() {
    for mode in [false, true] {
        let mut spec = plugin(false, false).spec().clone();
        let mut model = plugin_model();
        if mode {
            spec.mode = ace_model::AnalysisMode::Phase;
            spec.outputs.clear();
        } else {
            model.parameters.get_mut("payload").unwrap().unit = Unit::Meter;
            model.parameters.get_mut("payload").unwrap().value = Some(q(1.0, Unit::Meter));
        }
        let probe = probe(spec);
        let mut registry = Registry::default();
        registry.register(probe.clone()).unwrap();
        assert!(matches!(
            Session::new(model, registry),
            Err(crate::EngineError::Contract { .. })
        ));
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn domain_failure_revokes_downstream_values_without_invoking_solver() {
    let mut spec = plugin(false, false).spec().clone();
    spec.domain.bounds.push(DomainBound::positive(
        DomainVariable::Input("input".into()),
        Unit::Kilogram,
    ));
    let probe = probe(spec);
    let mut registry = Registry::default();
    registry.register(probe.clone()).unwrap();
    let mut session = Session::new(plugin_model(), registry).unwrap();
    let original = session.run().unwrap();
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert_eq!(original.runs[0].contract.as_ref(), Some(&probe.spec));
    session
        .edit(
            &BTreeMap::from([("payload".into(), q(0.0, Unit::Kilogram))]),
            source(),
        )
        .unwrap();
    let result = session.run().unwrap();
    assert_eq!(result.values["structure"].status, ValueStatus::Failed);
    assert_eq!(result.values["total"].status, ValueStatus::Blocked);
    assert!(result.values["total"].quantity.is_none());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert!(result.runs.iter().any(|run| {
        run.message
            .as_ref()
            .is_some_and(|text| text.contains("outside solver domain"))
    }));
}

fn connected() -> (VehicleModel, Registry, Arc<Probe>) {
    let mut model = plugin_model();
    model.parameters.get_mut("total").unwrap().expression = None;
    let mut consumer = model.analyses["structure"].clone();
    consumer.plugin = "consumer".into();
    consumer.inputs.insert("input".into(), "structure".into());
    consumer.outputs.insert("output".into(), "total".into());
    model.analyses.insert("consumer".into(), consumer);
    let meaning = ScalarPort::semantic(Unit::Kilogram, "mass/1", "total");
    let mut producer_spec = plugin(false, false).spec().clone();
    producer_spec
        .outputs
        .insert("output".into(), meaning.clone());
    let mut consumer_spec = plugin(false, false).spec().clone();
    consumer_spec.identity.id = "consumer".into();
    consumer_spec.inputs.insert("input".into(), meaning);
    let bad = ScalarPort::semantic(Unit::Kilogram, "mass/1", "fuel-only");
    let mut bad_spec = consumer_spec.clone();
    bad_spec.identity.id = "incompatible".into();
    bad_spec.inputs.insert("input".into(), bad);
    let bad_probe = probe(bad_spec);
    let mut registry = Registry::default();
    for plugin in [
        probe(producer_spec),
        probe(consumer_spec),
        bad_probe.clone(),
    ] {
        registry.register(plugin).unwrap();
    }
    (model, registry, bad_probe)
}

#[test]
fn incompatible_composition_transaction_rolls_back_model_values_runs_and_revision() {
    let (model, registry, bad) = connected();
    let mut session = Session::new(model, registry).unwrap();
    let before = serde_json::to_value(session.run().unwrap()).unwrap();
    let before_model = session.model().clone();
    let mut analysis = before_model.analyses["consumer"].clone();
    analysis.plugin = "incompatible".into();
    assert!(
        session
            .transact(&[ModelEdit::Analysis {
                id: "consumer".into(),
                value: Some(analysis)
            }])
            .is_err()
    );
    assert_eq!(session.model(), &before_model);
    let mut after = serde_json::to_value(session.snapshot()).unwrap();
    after["executed"] = before["executed"].clone();
    assert_eq!(after, before);
    assert!(session.run().unwrap().executed.is_empty());
    assert_eq!(bad.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn rejected_deferred_registration_does_not_publish_incompatible_plugin() {
    let mut session = Session::new(plugin_model(), Registry::default()).unwrap();
    let mut bad = plugin(false, false).spec().clone();
    bad.inputs.insert("input".into(), Unit::Meter.into());
    assert!(session.register(probe(bad)).is_err());
    assert!(session.contracts().is_empty());
    session.register(plugin(false, false)).unwrap();
    assert_eq!(value(&session.run().unwrap(), "total"), 4.0);
}

#[test]
fn dimensional_expressions_cannot_silently_claim_producer_semantics() {
    let (mut model, registry, bad) = connected();
    model.parameters.insert(
        "alias".into(),
        parameter(None, Unit::Kilogram, Some(reference("structure"))),
    );
    model
        .analyses
        .get_mut("consumer")
        .unwrap()
        .inputs
        .insert("input".into(), "alias".into());
    assert!(matches!(
        Session::new(model, registry),
        Err(crate::EngineError::Contract { .. })
    ));
    assert_eq!(bad.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn malformed_domains_never_enter_the_registry() {
    for (name, unit) in [("undeclared", Unit::Kilogram), ("input", Unit::Meter)] {
        let mut bad = plugin(false, false).spec().clone();
        bad.domain.bounds.push(DomainBound::positive(
            DomainVariable::Input(name.into()),
            unit,
        ));
        let mut registry = Registry::default();
        assert!(registry.register(probe(bad)).is_err());
        registry.register(plugin(false, false)).unwrap();
        assert!(
            Session::new(plugin_model(), registry)
                .unwrap()
                .run()
                .unwrap()
                .complete()
        );
    }
}

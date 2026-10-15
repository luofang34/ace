use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn branches_keep_edits_and_calculation_evidence_independent() -> crate::EngineResult<()> {
    let mut session = Session::new(plugin_model(), Registry::default())?;
    session.register(plugin(false, false))?;
    let original = session.run()?;
    let mut branch = session.branch()?;
    assert!(branch.snapshot().runs.is_empty());
    branch.edit(
        &BTreeMap::from([("payload".into(), q(3.0, Unit::Kilogram))]),
        source(),
    )?;
    assert_eq!(value(&branch.run()?, "total"), 12.0);
    assert_eq!(session.snapshot().values, original.values);
    assert_eq!(session.snapshot().runs.len(), original.runs.len());
    assert_eq!(session.model().revision, 0);
    Ok(())
}

#[test]
fn previews_use_only_fast_adapters_and_leave_excluded_outputs_unavailable()
-> crate::EngineResult<()> {
    for fidelity in [
        Fidelity::Conceptual,
        Fidelity::Surrogate,
        Fidelity::Handbook,
        Fidelity::HighFidelity,
    ] {
        let mut spec = plugin(false, false).spec().clone();
        spec.identity.fidelity = fidelity;
        let mut registry = Registry::default();
        registry.register(Arc::new(Scale {
            spec,
            approximate: false,
            wrong_unit: false,
        }))?;
        let mut session = Session::new(plugin_model(), registry)?;
        assert!(session.run()?.complete());
        let preview = session.preview_branch()?.run()?;
        if matches!(fidelity, Fidelity::Conceptual | Fidelity::Surrogate) {
            assert_eq!(value(&preview, "total"), 4.0);
        } else {
            assert_eq!(preview.values["structure"].status, ValueStatus::Unavailable);
            assert_eq!(preview.values["total"].status, ValueStatus::Blocked);
            assert!(preview.values["structure"].quantity.is_none());
        }
        assert!(session.snapshot().complete());
    }
    Ok(())
}

struct RecordedScale {
    spec: PluginSpec,
    factor: f64,
    called: Arc<AtomicBool>,
}

impl AnalysisPlugin for RecordedScale {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        self.called.store(true, Ordering::SeqCst);
        Ok(PluginOutput {
            values: BTreeMap::from([(
                "output".into(),
                q(inputs["input"].value * self.factor, Unit::Kilogram),
            )]),
            approximate: false,
            artifacts: vec![ace_model::Artifact::from_text(
                self.spec.identity.id.clone(),
                "text/plain",
                "test.preview/1",
                self.spec.identity.generator(),
            )],
            details: serde_json::json!({"factor":self.factor}),
            failure: None,
        })
    }
}

fn recorded(id: &str, fidelity: Fidelity, factor: f64) -> Arc<RecordedScale> {
    let mut spec = plugin(false, false).spec().clone();
    spec.identity.id = id.into();
    spec.identity.code_revision = format!("{id}-revision");
    spec.identity.fidelity = fidelity;
    Arc::new(RecordedScale {
        spec,
        factor,
        called: Arc::new(AtomicBool::new(false)),
    })
}

#[test]
fn explicit_preview_uses_fast_evidence_and_preserves_authoritative_execution()
-> crate::EngineResult<()> {
    let formal = recorded("scale", Fidelity::HighFidelity, 3.0);
    let fast = recorded("scale_preview", Fidelity::Conceptual, 2.0);
    let mut registry = Registry::default();
    registry.register(formal.clone())?;
    registry.register_preview("scale", "1", fast.clone())?;
    let mut session = Session::new(plugin_model(), registry)?;
    let mut preview = session.fork().preview_branch()?;
    preview.edit(
        &BTreeMap::from([("payload".into(), q(2.0, Unit::Kilogram))]),
        source(),
    )?;
    let result = preview.run()?;
    assert!(fast.called.load(Ordering::SeqCst));
    assert!(!formal.called.load(Ordering::SeqCst));
    assert_eq!(value(&result, "total"), 8.0);
    assert_eq!(result.values["total"].status, ValueStatus::Approximate);
    let run = result.runs.iter().find(|run| run.plugin.is_some()).unwrap();
    assert_eq!(run.plugin.as_ref(), Some(&fast.spec.identity));
    assert_eq!(run.calculation["plugin"], "scale");
    assert_eq!(run.artifacts[0].generator.id, "scale_preview");
    run.artifacts[0].verify(b"scale_preview").unwrap();
    assert_eq!(run.details["factor"], 2.0);
    assert!(session.snapshot().runs.is_empty());
    let authoritative = session.run()?;
    assert!(formal.called.load(Ordering::SeqCst));
    assert_eq!(value(&authoritative, "total"), 6.0);
    assert_eq!(authoritative.values["total"].status, ValueStatus::Computed);
    assert_eq!(session.model().revision, 0);
    assert_eq!(
        authoritative
            .runs
            .iter()
            .find_map(|run| run.plugin.as_ref()),
        Some(&formal.spec.identity)
    );
    Ok(())
}

#[test]
fn preview_registration_rejects_ambiguous_or_incompatible_contracts() -> crate::EngineResult<()> {
    let mut registry = Registry::default();
    let fast = recorded("scale_preview", Fidelity::Surrogate, 2.0);
    assert!(
        registry
            .register_preview("scale", "1", fast.clone())
            .is_err()
    );
    registry.register(recorded("scale", Fidelity::HighFidelity, 3.0))?;
    assert!(
        registry
            .register_preview("scale", "2", fast.clone())
            .is_err()
    );
    for fidelity in [Fidelity::Handbook, Fidelity::HighFidelity] {
        assert!(
            registry
                .register_preview("scale", "1", recorded("slow", fidelity, 1.0))
                .is_err()
        );
    }
    for (input, unit) in [(true, Unit::Meter), (false, Unit::Meter)] {
        let mut bad = recorded("bad", Fidelity::Surrogate, 1.0).spec.clone();
        if input {
            bad.inputs.insert("input".into(), unit);
        } else {
            bad.outputs.insert("output".into(), unit);
        }
        assert!(
            registry
                .register_preview(
                    "scale",
                    "1",
                    Arc::new(Scale {
                        spec: bad,
                        approximate: false,
                        wrong_unit: false,
                    })
                )
                .is_err()
        );
    }
    registry.register_preview("scale", "1", fast.clone())?;
    assert!(registry.register_preview("scale", "1", fast).is_err());
    Ok(())
}

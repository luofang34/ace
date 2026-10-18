use super::*;
use crate::{AnalysisPlugin, PluginOutput, PluginSpec};
use ace_model::{DomainBound, DomainVariable, Unit, ValidityDomain};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Step {
    spec: PluginSpec,
    calls: Arc<AtomicUsize>,
    target: f64,
    burn: Quantity,
}

impl AnalysisPlugin for Step {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        _: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        Err(SolverError::Failed("state required".into()))
    }
    fn evaluate_phase(
        &self,
        _: &BTreeMap<String, Quantity>,
        state: &VehicleState,
        _: &AnalysisContext<'_>,
    ) -> Result<PhaseOutput, SolverError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let start = state.conditions["altitude"].to(Unit::Meter)?.value;
        Ok(PhaseOutput {
            increments: vec![PhaseIncrement {
                duration: Quantity::new(1.0, Unit::Second)?,
                distance: Quantity::new(10.0, Unit::Meter)?,
                fuel_burn: self.burn,
                cost: Quantity::new(0.0, Unit::Dollar)?,
                conditions: BTreeMap::from([(
                    "altitude".into(),
                    Quantity::new(start + self.target, Unit::Meter)?,
                )]),
                details: serde_json::Value::Null,
            }],
            approximate: true,
            artifacts: vec![],
            details: serde_json::Value::Null,
        })
    }
}

fn fixture(target: f64) -> (Registry, Analysis, VehicleState, Arc<Step>) {
    let plugin = Arc::new(Step {
        spec: PluginSpec {
            identity: PluginIdentity {
                id: "step".into(),
                version: "1".into(),
                fidelity: Fidelity::Conceptual,
                code_revision: "fixture".into(),
            },
            mode: AnalysisMode::Phase,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            domain: ValidityDomain {
                bounds: vec![DomainBound::closed(
                    DomainVariable::Condition("altitude".into()),
                    Unit::Meter,
                    0.0,
                    100.0,
                )],
                assumptions: vec![],
            },
        },
        calls: Arc::new(AtomicUsize::new(0)),
        target,
        burn: Quantity::new(0.0, Unit::Kilogram).unwrap(),
    });
    let mut registry = Registry::default();
    registry.register(plugin.clone()).unwrap();
    let analysis = Analysis {
        mode: AnalysisMode::Phase,
        plugin: "step".into(),
        version: "1".into(),
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        geometry: vec![],
        requirements: vec![],
        resources: Default::default(),
    };
    let state = VehicleState {
        time: Quantity::new(0.0, Unit::Second).unwrap(),
        distance: Quantity::new(0.0, Unit::Meter).unwrap(),
        cost: Quantity::new(0.0, Unit::Dollar).unwrap(),
        masses: BTreeMap::new(),
        conditions: BTreeMap::from([(
            "altitude".into(),
            Quantity::new(10.0, Unit::Meter).unwrap(),
        )]),
    };
    (registry, analysis, state, plugin)
}

#[test]
fn phase_checks_actual_entry_state_and_every_returned_endpoint() {
    for (target, accepted) in [(90.0, true), (91.0, false)] {
        let (registry, analysis, mut state, plugin) = fixture(target);
        let values = BTreeMap::new();
        let context = AnalysisContext {
            resources: Default::default(),
            values: &values,
            registry: &registry,
        };
        let result = registry.evaluate_phase(&analysis, &state, &context);
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(plugin.calls.load(Ordering::SeqCst), 1);
        if let Ok(run) = result {
            assert_eq!(run.contract, Some(plugin.spec.clone()));
            assert_eq!(run.initial_state, state);
        }
        state
            .conditions
            .insert("altitude".into(), Quantity::new(-1.0, Unit::Meter).unwrap());
        assert!(matches!(
            registry.evaluate_phase(&analysis, &state, &context),
            Err(SolverError::Contract(
                ace_model::ContractError::Outside { .. }
            ))
        ));
        assert_eq!(plugin.calls.load(Ordering::SeqCst), 1);
        state.conditions.clear();
        assert!(matches!(
            registry.evaluate_phase(&analysis, &state, &context),
            Err(SolverError::Contract(
                ace_model::ContractError::Missing { .. }
            ))
        ));
        assert_eq!(plugin.calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn invalid_increment_units_or_signs_are_rejected_at_the_phase_boundary() {
    for burn in [
        Quantity::new(-1.0, Unit::Kilogram).unwrap(),
        Quantity::new(1.0, Unit::Meter).unwrap(),
    ] {
        let (_, analysis, state, base) = fixture(1.0);
        let mut registry = Registry::default();
        let plugin = Arc::new(Step {
            spec: base.spec.clone(),
            calls: Arc::new(AtomicUsize::new(0)),
            target: 1.0,
            burn,
        });
        registry.register(plugin.clone()).unwrap();
        let values = BTreeMap::new();
        let context = AnalysisContext {
            resources: Default::default(),
            values: &values,
            registry: &registry,
        };
        assert!(
            registry
                .evaluate_phase(&analysis, &state, &context)
                .is_err()
        );
        assert_eq!(plugin.calls.load(Ordering::SeqCst), 1);
    }
}

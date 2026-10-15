use super::*;
use crate::{Feasibility, StudyError};
use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PluginIdentity, PluginOutput, PluginSpec, Registry,
    SolverError,
};
use ace_model::{Quantity, Unit, VehicleModel};
use serde_json::json;
use std::sync::Arc;

fn q(value: f64, unit: Unit) -> Quantity {
    Quantity::new(value, unit).unwrap()
}
fn model() -> VehicleModel {
    serde_json::from_value(json!({"schema_version":1,"id":"study_fixture","name":"Generic test design","domain":"vehicle",
        "parameters": {
            "x":{"unit":"m","value":{"value":0,"unit":"m"},"source":{"kind":"test","reference":"input"},"owner":"test"},
            "mode":{"unit":"1","value":{"value":1,"unit":"1"},"source":{"kind":"test","reference":"condition"},"owner":"test"},
            "loss":{"unit":"m^2","source":{"kind":"test","reference":"quadratic"},"owner":"test","expression":{"op":"power","base":{"op":"subtract","left":{"op":"reference","parameter":"x"},"right":{"op":"constant","value":{"value":2,"unit":"m"}}},"exponent":2}},
            "independent":{"unit":"m","source":{"kind":"test","reference":"constant expression"},"owner":"test","expression":{"op":"constant","value":{"value":7,"unit":"m"}}}},
        "objectives":{"loss":{"metric":"loss","direction":"min"},"reach":{"metric":"x","direction":"max"}},
        "requirements":{"limit":{"name":"Length limit","metric":"x","relation":"le","target":{"value":3,"unit":"m"},"priority":"hard","source":{"kind":"test","reference":"SRD"}},
            "preference":{"name":"Prefer shorter","metric":"x","relation":"le","target":{"value":1,"unit":"m"},"priority":"soft","source":{"kind":"test","reference":"preference"}}},
        "design_variables":[{"parameter":"x","lower":{"value":0,"unit":"m"},"upper":{"value":4,"unit":"m"}}]})).unwrap()
}
fn grid(values: &[f64], budget: u32) -> StudyPlan {
    StudyPlan {
        schema_version: 1,
        max_evaluations: budget,
        method: SearchMethod::Grid {
            axes: BTreeMap::from([(
                "x".into(),
                values.iter().map(|value| q(*value, Unit::Meter)).collect(),
            )]),
        },
    }
}
fn session(model: VehicleModel) -> Session {
    Session::new(model, Registry::default()).unwrap()
}
fn spans(archive: &StudyArchive, ids: &[String]) -> Vec<f64> {
    let mut values: Vec<_> = archive
        .candidates
        .iter()
        .filter(|c| ids.contains(&c.id))
        .map(|c| c.updates["x"].value)
        .collect();
    values.sort_by(f64::total_cmp);
    values
}

#[test]
fn mixed_objectives_keep_tradeoffs_and_exclude_hard_but_not_soft_violations() {
    let archive = run(&session(model()), &grid(&[0.0, 1.0, 2.0, 3.0, 4.0], 5)).unwrap();
    assert_eq!(spans(&archive, &archive.pareto), vec![2.0, 3.0]);
    assert!(archive.provisional_pareto.is_empty());
    assert_eq!(archive.candidates[4].feasibility, Feasibility::Violated);
    assert_eq!(
        archive.candidates[2].margins["preference"].status,
        ace_engine::MarginStatus::Fail
    );
    assert_eq!(archive.stop_reason, StopReason::GridExhausted);
}

#[test]
fn study_forks_reuse_unaffected_runs_and_leave_the_live_model_untouched() {
    let mut baseline = session(model());
    let before = baseline.run().unwrap();
    let plan = grid(&[1.0, 2.0, 3.0], 3);
    let archive = run(&baseline, &plan).unwrap();
    for candidate in &archive.candidates {
        assert_eq!(
            candidate.model.parameters["x"].source.reference,
            format!("vehicle-study:{}", archive.request_id)
        );
        assert_eq!(
            candidate.state.values["independent"].run_id,
            before.values["independent"].run_id
        );
        assert_eq!(candidate.state.executed, vec!["expression:loss"]);
        let snapshot = archive.snapshot(&candidate.id).unwrap();
        assert!(snapshot.complete());
        assert!(ace_engine::evaluate_requirements(&candidate.model, &snapshot).is_ok());
    }
    assert_eq!(baseline.snapshot().values, before.values);
    assert_eq!(baseline.model().revision, 0);
    assert_eq!(run(&baseline, &plan).unwrap().id, archive.id);
    let decoded: StudyArchive =
        serde_json::from_slice(&serde_json::to_vec(&archive).unwrap()).unwrap();
    assert!(decoded.snapshot(&decoded.candidates[0].id).is_ok());
    decoded.verify().unwrap();
    let mut altered = decoded.clone();
    altered.baseline.name = "altered".into();
    assert!(altered.verify().is_err());
}

#[test]
fn equivalent_units_are_deduplicated_and_budget_does_not_claim_grid_exhaustion() {
    let plan = StudyPlan {
        schema_version: 1,
        max_evaluations: 2,
        method: SearchMethod::Grid {
            axes: BTreeMap::from([(
                "x".into(),
                vec![
                    q(1.0, Unit::Foot),
                    q(0.3048, Unit::Meter),
                    q(2.0, Unit::Foot),
                ],
            )]),
        },
    };
    let archive = run(&session(model()), &plan).unwrap();
    assert_eq!(archive.candidates.len(), 2);
    assert_eq!(archive.stop_reason, StopReason::GridExhausted);
    let limited = run(&session(model()), &grid(&[0.0, 1.0, 2.0], 2)).unwrap();
    assert_eq!(limited.candidates.len(), 2);
    assert_eq!(limited.stop_reason, StopReason::EvaluationBudget);
}

#[test]
fn refinement_improves_the_objective_and_respects_declared_constraints() {
    let mut model = model();
    model.objectives.remove("reach");
    let plan = StudyPlan {
        schema_version: 1,
        max_evaluations: 24,
        method: SearchMethod::Refine {
            levels: 5,
            variables: Vec::new(),
        },
    };
    let archive = run(&session(model), &plan).unwrap();
    assert_eq!(spans(&archive, &archive.pareto), vec![2.0]);
    assert_eq!(archive.stop_reason, StopReason::RefinementLimit);
    assert!(archive.candidates.len() <= 24);
    assert!(
        archive
            .candidates
            .iter()
            .all(|c| (0.0..=4.0).contains(&c.updates["x"].value))
    );
}

#[test]
fn refinement_can_restore_feasibility_from_an_infeasible_start() {
    let mut model = model();
    model.objectives.remove("loss");
    model.objectives.get_mut("reach").unwrap().direction = ace_model::Direction::Min;
    let constraint = model.requirements.get_mut("limit").unwrap();
    constraint.relation = ace_model::Relation::Ge;
    constraint.target = q(2.0, Unit::Meter);
    let plan = StudyPlan {
        schema_version: 1,
        max_evaluations: 20,
        method: SearchMethod::Refine {
            levels: 4,
            variables: Vec::new(),
        },
    };
    let archive = run(&session(model), &plan).unwrap();
    assert_eq!(archive.candidates[0].feasibility, Feasibility::Violated);
    assert_eq!(spans(&archive, &archive.pareto), vec![2.0]);
}

#[test]
fn leaving_a_required_context_cannot_make_a_candidate_feasible() {
    let mut model = model();
    model
        .requirements
        .get_mut("limit")
        .unwrap()
        .condition
        .insert("mode".into(), q(1.0, Unit::Ratio));
    model.design_variables.push(ace_model::DesignVariable {
        parameter: "mode".into(),
        lower: q(0.0, Unit::Ratio),
        upper: q(2.0, Unit::Ratio),
    });
    let plan = StudyPlan {
        schema_version: 1,
        max_evaluations: 2,
        method: SearchMethod::Grid {
            axes: BTreeMap::from([(
                "mode".into(),
                vec![q(0.0, Unit::Ratio), q(1.0, Unit::Ratio)],
            )]),
        },
    };
    let archive = run(&session(model), &plan).unwrap();
    assert_eq!(archive.candidates[0].feasibility, Feasibility::Unknown);
    assert_eq!(archive.candidates[1].feasibility, Feasibility::Satisfied);
    assert_eq!(archive.pareto, vec![archive.candidates[1].id.clone()]);
}

struct Quality {
    spec: PluginSpec,
}
impl AnalysisPlugin for Quality {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let x = inputs
            .get("x")
            .ok_or_else(|| SolverError::Failed("missing x".into()))?
            .value;
        Ok(PluginOutput {
            values: BTreeMap::from([("loss".into(), q((x - 2.0).powi(2), Unit::SquareMeter))]),
            approximate: x >= 2.0,
            artifacts: Vec::new(),
            details: json!({"input":x}),
            failure: None,
        })
    }
}
fn plugin_session(installed: bool) -> Session {
    let mut model = model();
    model.parameters.get_mut("loss").unwrap().expression = None;
    model.analyses.insert(
        "quality".into(),
        ace_model::Analysis {
            mode: Default::default(),
            plugin: "quality".into(),
            version: "1".into(),
            inputs: BTreeMap::from([("x".into(), "x".into())]),
            outputs: BTreeMap::from([("loss".into(), "loss".into())]),
            geometry: Vec::new(),
            requirements: Vec::new(),
            resources: Default::default(),
        },
    );
    let mut registry = Registry::default();
    if installed {
        registry
            .register(Arc::new(Quality {
                spec: PluginSpec {
                    identity: PluginIdentity {
                        id: "quality".into(),
                        version: "1".into(),
                        fidelity: Fidelity::Conceptual,
                        code_revision: "fixture".into(),
                    },
                    inputs: BTreeMap::from([("x".into(), Unit::Meter)]),
                    outputs: BTreeMap::from([("loss".into(), Unit::SquareMeter)]),
                },
            }))
            .unwrap();
    }
    Session::new(model, registry).unwrap()
}

#[test]
fn approximate_objectives_have_a_separate_front_and_missing_solvers_have_none() {
    let archive = run(&plugin_session(true), &grid(&[0.0, 1.0, 2.0, 3.0, 4.0], 5)).unwrap();
    assert_eq!(spans(&archive, &archive.pareto), vec![1.0]);
    assert_eq!(spans(&archive, &archive.provisional_pareto), vec![2.0, 3.0]);
    assert_eq!(archive.candidates[4].feasibility, Feasibility::Violated);
    let missing = run(&plugin_session(false), &grid(&[0.0, 1.0, 2.0], 3)).unwrap();
    assert!(missing.pareto.is_empty());
    assert!(missing.provisional_pareto.is_empty());
    assert!(
        missing
            .candidates
            .iter()
            .all(|c| c.feasibility == Feasibility::Unknown)
    );
}

#[test]
fn evidence_tampering_and_missing_calculations_are_rejected_on_snapshot_restoration() {
    let archive = run(&session(model()), &grid(&[1.0], 1)).unwrap();
    let id = archive.candidates[0].id.clone();
    let mut damaged = archive.clone();
    damaged.candidates[0].model.name = "changed".into();
    assert!(damaged.snapshot(&id).is_err());
    let mut damaged = archive.clone();
    damaged.evidence.clear();
    assert!(matches!(
        damaged.snapshot(&id),
        Err(StudyError::MissingEvidence { .. })
    ));
    let mut damaged = archive.clone();
    damaged.evidence.values_mut().next().unwrap().message = Some("changed".into());
    assert!(damaged.snapshot(&id).is_err());
}

#[test]
fn invalid_plans_reject_out_of_bounds_wrong_units_and_computed_variables() {
    let baseline = model();
    assert!(grid(&[5.0], 1).validate(&baseline).is_err());
    assert!(grid(&[1.0], 0).validate(&baseline).is_err());
    let wrong = StudyPlan {
        schema_version: 1,
        max_evaluations: 1,
        method: SearchMethod::Grid {
            axes: BTreeMap::from([("x".into(), vec![q(1.0, Unit::Kilogram)])]),
        },
    };
    assert!(wrong.validate(&baseline).is_err());
    let mut computed = baseline.clone();
    computed.design_variables.push(ace_model::DesignVariable {
        parameter: "loss".into(),
        lower: q(0.0, Unit::SquareMeter),
        upper: q(4.0, Unit::SquareMeter),
    });
    let plan = StudyPlan {
        schema_version: 1,
        max_evaluations: 1,
        method: SearchMethod::Refine {
            levels: 1,
            variables: vec!["loss".into()],
        },
    };
    assert!(plan.validate(&computed).is_err());
    assert!(serde_json::from_value::<StudyPlan>(json!({"schema_version":1,"max_evaluations":1,"method":{"kind":"refine","levels":1,"ignored":true}})).is_err());
}

#[test]
fn multi_axis_grid_evaluates_the_full_cartesian_product() {
    let mut model = model();
    model.design_variables.push(ace_model::DesignVariable {
        parameter: "mode".into(),
        lower: q(0.0, Unit::Ratio),
        upper: q(2.0, Unit::Ratio),
    });
    let plan = StudyPlan {
        schema_version: 1,
        max_evaluations: 6,
        method: SearchMethod::Grid {
            axes: BTreeMap::from([
                (
                    "x".into(),
                    vec![
                        q(0.0, Unit::Meter),
                        q(1.0, Unit::Meter),
                        q(2.0, Unit::Meter),
                    ],
                ),
                (
                    "mode".into(),
                    vec![q(0.0, Unit::Ratio), q(1.0, Unit::Ratio)],
                ),
            ]),
        },
    };
    let archive = run(&session(model), &plan).unwrap();
    let samples: Vec<_> = archive
        .candidates
        .iter()
        .map(|c| (c.updates["mode"].value, c.updates["x"].value))
        .collect();
    assert_eq!(
        samples,
        vec![
            (0.0, 0.0),
            (0.0, 1.0),
            (0.0, 2.0),
            (1.0, 0.0),
            (1.0, 1.0),
            (1.0, 2.0)
        ]
    );
    assert_eq!(archive.stop_reason, StopReason::GridExhausted);
}

#[test]
fn nonconverged_closures_are_unknown_and_cannot_enter_a_front() {
    let mut model = model();
    let loss = model.parameters.get_mut("loss").unwrap();
    loss.value = Some(q(1.0, Unit::SquareMeter));
    loss.expression = Some(ace_model::Expression::Multiply {
        left: Box::new(ace_model::Expression::Reference {
            parameter: "loss".into(),
        }),
        right: Box::new(ace_model::Expression::Constant {
            value: q(2.0, Unit::Ratio),
        }),
    });
    model.closures.insert(
        "divergent".into(),
        ace_model::Closure {
            tolerances: BTreeMap::from([(
                "loss".into(),
                ace_model::Tolerance {
                    absolute: q(1e-9, Unit::SquareMeter),
                    relative: 0.0,
                },
            )]),
            max_iterations: 2,
            relaxation: 1.0,
        },
    );
    let archive = run(&session(model), &grid(&[0.0, 1.0], 2)).unwrap();
    assert!(archive.pareto.is_empty());
    assert!(archive.provisional_pareto.is_empty());
    for candidate in &archive.candidates {
        assert_eq!(candidate.feasibility, Feasibility::Unknown);
        assert!(!candidate.state.closures["divergent"].converged);
        assert!(candidate.objectives["loss"].value.quantity.is_none());
    }
}

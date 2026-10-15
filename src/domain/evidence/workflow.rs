use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::CandidateDescriptor;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OptimizerCheckpoint {
    pub(crate) generation: u32,
    pub(crate) rng_state: u64,
    pub(crate) population: Vec<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct CandidateOutcome {
    pub(crate) candidate_id: String,
    pub(crate) evaluation_id: String,
    pub(crate) generation: u32,
    pub(crate) feasible: bool,
    pub(crate) normalized_constraint_violation: f64,
    pub(crate) objective_values: BTreeMap<String, f64>,
    pub(crate) rank_score: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct StudyArchiveWorkflow {
    pub(crate) checkpoint: OptimizerCheckpoint,
    pub(crate) outcomes: Vec<CandidateOutcome>,
    pub(crate) pareto_candidate_ids: Vec<String>,
}

impl StudyArchiveWorkflow {
    pub(crate) fn is_empty(&self) -> bool {
        self.checkpoint == OptimizerCheckpoint::default()
            && self.outcomes.is_empty()
            && self.pareto_candidate_ids.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StudyLoadResult {
    pub(crate) study_id: String,
    pub(crate) name: String,
    pub(crate) study_digest: String,
    pub(crate) baseline_digest: String,
    pub(crate) baseline_scenario_id: String,
    pub(crate) variable_count: usize,
    pub(crate) objective_count: usize,
    pub(crate) constraint_count: usize,
    pub(crate) embedded_baseline: bool,
    pub(crate) selected_candidates: Vec<CandidateDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CandidateSummary {
    pub(crate) candidate: CandidateDescriptor,
    pub(crate) generation: u32,
    pub(crate) feasible: bool,
    pub(crate) normalized_constraint_violation: f64,
    pub(crate) objective_values: BTreeMap<String, f64>,
    pub(crate) rank_score: f64,
    pub(crate) evidence_id: String,
    pub(crate) backend: String,
    pub(crate) fidelity_level: u8,
    pub(crate) failed_constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StudyRunResult {
    pub(crate) study_id: String,
    pub(crate) study_digest: String,
    pub(crate) archive_id: String,
    pub(crate) evaluated_candidates: usize,
    pub(crate) feasible_candidates: usize,
    pub(crate) reused_evaluations: usize,
    pub(crate) pareto_candidates: Vec<CandidateSummary>,
    pub(crate) selected_candidates: Vec<CandidateSummary>,
    pub(crate) archive_path: String,
    pub(crate) complete: bool,
    #[serde(skip)]
    pub(crate) trade_surface: Option<StudyTradeSurface>,
    #[serde(skip)]
    pub(crate) irregular_trade_space: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct StudyTradeSurface {
    pub(crate) x_path: String,
    pub(crate) x_unit: String,
    pub(crate) x_values: Vec<f64>,
    pub(crate) y_path: String,
    pub(crate) y_unit: String,
    pub(crate) y_values: Vec<f64>,
    pub(crate) objective_id: String,
    pub(crate) values: Vec<f64>,
    pub(crate) feasible_mask: Vec<bool>,
}

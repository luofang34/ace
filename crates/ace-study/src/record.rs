use crate::{StudyError, StudyPlan, StudyResult};
use ace_engine::{ClosureResult, Margin, RunRecord, Snapshot, ValueRecord};
use ace_model::{Direction, Quantity, VehicleModel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub use ace_engine::Feasibility;

/// Exact objective observation, retaining its independent direction and evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveValue {
    /// Preferred direction for this objective.
    pub direction: Direction,
    /// Current canonical observation, including units, validity, and producing run.
    pub value: ValueRecord,
}

/// Candidate projection with deduplicated calculation evidence stored in its archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateState {
    /// Execution build identity retained across archive restoration.
    pub build: ace_engine::BuildIdentity,
    /// Hash of the candidate's authoritative model declarations.
    pub model_digest: String,
    /// All current canonical observations.
    pub values: BTreeMap<String, ValueRecord>,
    /// Current coupled closure outcomes.
    pub closures: BTreeMap<String, ClosureResult>,
    /// Graph nodes recomputed for this candidate.
    pub executed: Vec<String>,
    /// Complete transitive calculation evidence referenced by this candidate.
    pub run_ids: Vec<String>,
}

/// One evaluated design, kept independently of the working model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// Content address of this candidate and its evidence references.
    pub id: String,
    /// Canonical sampled input values.
    pub updates: BTreeMap<String, Quantity>,
    /// Authoritative candidate model, directly usable by synthesis or the sheet.
    pub model: VehicleModel,
    /// Current projection with archive-local evidence references.
    pub state: CandidateState,
    /// Requirement margins, including soft and informational requirements.
    pub margins: BTreeMap<String, Margin>,
    /// Independent objective measurements; no artificial weighted sum is imposed.
    pub objectives: BTreeMap<String, ObjectiveValue>,
    /// Candidate feasibility at its actual evidence quality.
    pub feasibility: Feasibility,
}

/// Why a bounded study stopped; none of these imply a globally optimal design.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Every distinct combination in the declared grid was evaluated.
    GridExhausted,
    /// Every requested refinement level was explored.
    RefinementLimit,
    /// More distinct proposals remained than the evaluation budget allowed.
    EvaluationBudget,
}

/// Portable, content-addressed study result with two separate evidence-quality fronts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyArchive {
    /// Hash of this archive excluding this field.
    pub id: String,
    /// Content address of the baseline, plan, and search implementation.
    pub request_id: String,
    /// Immutable search implementation identity.
    pub algorithm_revision: String,
    /// Authoritative baseline; study execution never edits the caller's model.
    pub baseline: VehicleModel,
    /// Validated plan actually executed.
    pub plan: StudyPlan,
    /// Evaluations in deterministic proposal order.
    pub candidates: Vec<Candidate>,
    /// Deduplicated immutable calculations consumed by the candidate projections.
    pub evidence: BTreeMap<String, RunRecord>,
    /// Nondominated candidates with current, non-provisional comparison evidence.
    pub pareto: Vec<String>,
    /// Nondominated candidates whose comparison evidence is provisional.
    pub provisional_pareto: Vec<String>,
    /// Bounded termination reason.
    pub stop_reason: StopReason,
}

impl StudyArchive {
    /// Verify the archive and every candidate before importing study evidence.
    pub fn verify(&self) -> StudyResult<()> {
        let mut check = self.clone();
        check.id.clear();
        if digest(&check)? != self.id {
            return Err(StudyError::invalid(
                "archive",
                "content address does not match its data",
            ));
        }
        self.baseline.validate()?;
        self.plan.validate(&self.baseline)?;
        for candidate in &self.candidates {
            self.snapshot(&candidate.id)?;
        }
        Ok(())
    }
    /// Restore a candidate's full engine snapshot and verify its evidence content addresses.
    pub fn snapshot(&self, id: &str) -> StudyResult<Snapshot> {
        let candidate = self
            .candidates
            .iter()
            .find(|candidate| candidate.id == id)
            .ok_or_else(|| StudyError::invalid("candidate", format!("unknown candidate {id}")))?;
        let mut check = candidate.clone();
        check.id.clear();
        if digest(&check)? != id || digest(&candidate.model)? != candidate.state.model_digest {
            return Err(StudyError::invalid(
                id,
                "candidate content address does not match its data",
            ));
        }
        let mut runs = Vec::new();
        for run_id in &candidate.state.run_ids {
            let run = self
                .evidence
                .get(run_id)
                .ok_or_else(|| StudyError::MissingEvidence {
                    candidate: id.into(),
                    run: run_id.clone(),
                })?;
            let mut check = run.clone();
            check.id.clear();
            if run.id != *run_id || digest(&check)? != *run_id {
                return Err(StudyError::invalid(
                    run_id,
                    "calculation content address does not match its data",
                ));
            }
            runs.push(run.clone());
        }
        Ok(Snapshot {
            build: candidate.state.build.clone(),
            vehicle_id: candidate.model.id.clone(),
            revision: candidate.model.revision,
            model_digest: candidate.state.model_digest.clone(),
            values: candidate.state.values.clone(),
            closures: candidate.state.closures.clone(),
            executed: candidate.state.executed.clone(),
            runs,
        })
    }
}

pub(crate) fn digest(value: &impl Serialize) -> StudyResult<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

pub(crate) fn algorithm_revision() -> String {
    ace_engine::implementation_identity().digest
}

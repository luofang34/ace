use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::evidence::{
    CandidateDescriptor, CandidateOutcome, CandidateSummary, ConstraintStatus, EvidenceEnvelope,
    StudyArchive, StudyArchiveWorkflow, StudyRunResult,
};
use crate::domain::study::{ObjectiveDirection, StudyDefinition};
use crate::services::analysis::ApplicationService;

pub(super) fn rank(
    study: &StudyDefinition,
    workflow: &mut StudyArchiveWorkflow,
    selected_candidate_ids: &mut Vec<String>,
) {
    reset_scores(&mut workflow.outcomes);
    for objective in &study.objectives {
        score_objective(objective, &mut workflow.outcomes);
    }
    workflow.pareto_candidate_ids = pareto_ids(study, &workflow.outcomes);
    *selected_candidate_ids = selected_ids(study, workflow);
}

fn reset_scores(outcomes: &mut [CandidateOutcome]) {
    for outcome in outcomes {
        outcome.rank_score = outcome.normalized_constraint_violation * 1000.0
            + if outcome.feasible { 0.0 } else { 100.0 };
    }
}

fn score_objective(
    objective: &crate::domain::study::StudyObjective,
    outcomes: &mut [CandidateOutcome],
) {
    let values = outcomes
        .iter()
        .filter_map(|outcome| outcome.objective_values.get(&objective.id).copied())
        .collect::<Vec<_>>();
    let minimum = values.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = (maximum - minimum).abs();
    for outcome in outcomes {
        if let Some(value) = outcome.objective_values.get(&objective.id) {
            let normalized = if range <= 1.0e-12 {
                0.0
            } else if matches!(objective.direction, ObjectiveDirection::Minimize) {
                (*value - minimum) / range
            } else {
                (maximum - *value) / range
            };
            outcome.rank_score += objective.weight * normalized;
        }
    }
}

fn pareto_ids(study: &StudyDefinition, outcomes: &[CandidateOutcome]) -> Vec<String> {
    let feasible = outcomes
        .iter()
        .filter(|outcome| outcome.feasible)
        .collect::<Vec<_>>();
    let mut ids = feasible
        .iter()
        .filter(|candidate| {
            !feasible.iter().any(|other| {
                other.candidate_id != candidate.candidate_id && dominates(study, other, candidate)
            })
        })
        .map(|outcome| outcome.candidate_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn dominates(study: &StudyDefinition, left: &CandidateOutcome, right: &CandidateOutcome) -> bool {
    let mut strictly_better = false;
    for objective in &study.objectives {
        let (Some(left_value), Some(right_value)) = (
            left.objective_values.get(&objective.id),
            right.objective_values.get(&objective.id),
        ) else {
            return false;
        };
        let ordering = left_value.total_cmp(right_value);
        let no_worse = match objective.direction {
            ObjectiveDirection::Minimize => ordering != Ordering::Greater,
            ObjectiveDirection::Maximize => ordering != Ordering::Less,
        };
        if !no_worse {
            return false;
        }
        strictly_better |= ordering != Ordering::Equal;
    }
    strictly_better
}

fn selected_ids(study: &StudyDefinition, workflow: &StudyArchiveWorkflow) -> Vec<String> {
    let pareto = workflow
        .pareto_candidate_ids
        .iter()
        .collect::<BTreeSet<_>>();
    let mut candidates = workflow
        .outcomes
        .iter()
        .filter(|outcome| pareto.is_empty() || pareto.contains(&outcome.candidate_id))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| compare(left, right));
    let count =
        usize::try_from(study.analysis.refinement_candidate_limit.max(1)).unwrap_or(usize::MAX);
    candidates
        .into_iter()
        .take(count)
        .map(|outcome| outcome.candidate_id.clone())
        .collect()
}

pub(super) fn compare(left: &CandidateOutcome, right: &CandidateOutcome) -> Ordering {
    right
        .feasible
        .cmp(&left.feasible)
        .then_with(|| {
            left.normalized_constraint_violation
                .total_cmp(&right.normalized_constraint_violation)
        })
        .then_with(|| left.rank_score.total_cmp(&right.rank_score))
        .then_with(|| left.candidate_id.cmp(&right.candidate_id))
}

pub(super) fn run_result(
    service: &ApplicationService,
    archive: &StudyArchive,
    reused_evaluations: usize,
    limit: usize,
) -> AexResult<StudyRunResult> {
    let limit = limit.clamp(1, 50);
    let candidates = archive
        .candidates
        .iter()
        .map(|candidate| (candidate.candidate_id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let evidence = load_evidence(service, archive)?;
    let path = service.studies.archive_path(&archive.archive_id)?;
    Ok(StudyRunResult {
        study_id: archive.study_id.clone(),
        study_digest: archive.study_digest.clone(),
        archive_id: archive.archive_id.clone(),
        evaluated_candidates: archive.workflow.outcomes.len(),
        feasible_candidates: archive
            .workflow
            .outcomes
            .iter()
            .filter(|outcome| outcome.feasible)
            .count(),
        reused_evaluations,
        pareto_candidates: collect_by_ids(
            &archive.workflow,
            &archive.workflow.pareto_candidate_ids,
            &candidates,
            &evidence,
            limit,
        )?,
        selected_candidates: collect_by_ids(
            &archive.workflow,
            &archive.selected_candidate_ids,
            &candidates,
            &evidence,
            limit,
        )?,
        archive_path: path.display().to_string(),
        complete: archive.complete,
    })
}

fn load_evidence(
    service: &ApplicationService,
    archive: &StudyArchive,
) -> AexResult<BTreeMap<String, EvidenceEnvelope>> {
    archive
        .evaluation_ids
        .iter()
        .map(|id| {
            service
                .studies
                .load_evaluation_blocking(id)
                .map(|evidence| (id.clone(), evidence))
        })
        .collect()
}

fn collect_by_ids(
    workflow: &StudyArchiveWorkflow,
    ids: &[String],
    candidates: &BTreeMap<&str, &CandidateDescriptor>,
    evidence: &BTreeMap<String, EvidenceEnvelope>,
    limit: usize,
) -> AexResult<Vec<CandidateSummary>> {
    let ids = ids.iter().collect::<BTreeSet<_>>();
    let mut outcomes = workflow
        .outcomes
        .iter()
        .filter(|outcome| ids.contains(&outcome.candidate_id))
        .collect::<Vec<_>>();
    outcomes.sort_by(|left, right| compare(left, right));
    outcomes.truncate(limit);
    outcomes
        .into_iter()
        .map(|outcome| summary(outcome, candidates, evidence))
        .collect()
}

fn summary(
    outcome: &CandidateOutcome,
    candidates: &BTreeMap<&str, &CandidateDescriptor>,
    evidence: &BTreeMap<String, EvidenceEnvelope>,
) -> AexResult<CandidateSummary> {
    let candidate = candidates
        .get(outcome.candidate_id.as_str())
        .ok_or_else(|| missing_archive_member("candidate", &outcome.candidate_id))?;
    let evidence = evidence
        .get(&outcome.evaluation_id)
        .ok_or_else(|| missing_archive_member("evaluation", &outcome.evaluation_id))?;
    Ok(CandidateSummary {
        candidate: (*candidate).clone(),
        generation: outcome.generation,
        feasible: outcome.feasible,
        normalized_constraint_violation: outcome.normalized_constraint_violation,
        objective_values: outcome.objective_values.clone(),
        rank_score: outcome.rank_score,
        evidence_id: outcome.evaluation_id.clone(),
        backend: evidence.analysis.backend.clone(),
        fidelity_level: evidence.analysis.fidelity_level,
        failed_constraints: evidence
            .results
            .constraints
            .iter()
            .filter(|constraint| constraint.status != ConstraintStatus::Pass)
            .map(|constraint| constraint.id.clone())
            .collect(),
    })
}

fn missing_archive_member(kind: &str, id: &str) -> AexError {
    AexError::validation(
        "INVALID_STUDY_ARCHIVE",
        format!("archive.{kind}"),
        format!("workflow references missing {kind} {id}"),
    )
}

#[cfg(test)]
mod tests;

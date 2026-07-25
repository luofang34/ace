use std::collections::BTreeMap;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::evidence::{
    CandidateDescriptor, CandidateOutcome, ConstraintStatus, EvaluationStatus, EvidenceEnvelope,
    StudyArchive,
};
use crate::services::analysis::ApplicationService;

pub(super) fn load_all_blocking(
    service: &ApplicationService,
    archive: &StudyArchive,
) -> AexResult<BTreeMap<String, EvidenceEnvelope>> {
    archive
        .workflow
        .outcomes
        .iter()
        .map(|outcome| {
            load_for_outcome_blocking(service, archive, outcome)
                .map(|evidence| (outcome.evaluation_id.clone(), evidence))
        })
        .collect()
}

pub(super) fn load_for_outcome_blocking(
    service: &ApplicationService,
    archive: &StudyArchive,
    outcome: &CandidateOutcome,
) -> AexResult<EvidenceEnvelope> {
    let candidate = archive
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == outcome.candidate_id)
        .ok_or_else(|| invalid_link("outcome references a missing candidate"))?;
    let evidence = service
        .studies
        .load_evaluation_blocking(&outcome.evaluation_id)?;
    validate_link(archive, outcome, candidate, &evidence)?;
    Ok(evidence)
}

fn validate_link(
    archive: &StudyArchive,
    outcome: &CandidateOutcome,
    candidate: &CandidateDescriptor,
    evidence: &EvidenceEnvelope,
) -> AexResult<()> {
    let evidence_feasible = matches!(evidence.status, EvaluationStatus::Succeeded)
        && evidence.results.constraints.iter().all(|constraint| {
            constraint.severity != "hard" || constraint.status == ConstraintStatus::Pass
        });
    if evidence.evaluation_id == outcome.evaluation_id
        && evidence.candidate_id == outcome.candidate_id
        && candidate.candidate_id == outcome.candidate_id
        && evidence.study_id == archive.study_id
        && evidence_feasible == outcome.feasible
    {
        Ok(())
    } else {
        Err(invalid_link(
            "archive outcome, candidate, evidence, or feasibility does not match",
        ))
    }
}

fn invalid_link(message: &str) -> AexError {
    AexError::validation(
        "INVALID_STUDY_EVIDENCE_LINK",
        "archive.workflow.outcomes",
        message,
    )
}

#[cfg(test)]
mod tests;

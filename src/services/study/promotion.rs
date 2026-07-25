use std::path::Path;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::services::analysis::ApplicationService;
use crate::services::study::evaluation::evaluator_signature;
use crate::services::study::loading::PreparedStudy;
use crate::services::study::search::latest_archive_for_study;
use crate::storage::design_store::DesignRecord;

pub(super) fn promote_blocking(
    service: &ApplicationService,
    prepared: &PreparedStudy,
    candidate_id: &str,
    design_id: &str,
    display_name: &str,
    design_root: &Path,
) -> AexResult<DesignRecord> {
    let archive = latest_archive_for_study(service, &prepared.document.study.id)?;
    validate_archive_signature(prepared, &archive)?;
    let outcome = archive
        .workflow
        .outcomes
        .iter()
        .find(|outcome| outcome.candidate_id == candidate_id)
        .ok_or_else(|| {
            AexError::validation(
                "STUDY_CANDIDATE_NOT_FOUND",
                "candidate_id",
                format!("candidate {candidate_id} is not present in the study archive"),
            )
        })?;
    if !outcome.feasible {
        return Err(AexError::validation(
            "CANNOT_PROMOTE_INFEASIBLE_CANDIDATE",
            "candidate_id",
            "only candidates passing recorded hard constraints can be promoted",
        ));
    }
    let candidate = archive
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == candidate_id)
        .ok_or_else(|| {
            AexError::validation(
                "INVALID_STUDY_ARCHIVE",
                "archive.candidates",
                format!("outcome references missing candidate {candidate_id}"),
            )
        })?;
    service.create_design_blocking(
        design_id,
        display_name,
        design_root,
        None,
        Some(prepared.scenario_path()),
        &candidate.parameters,
    )
}

fn validate_archive_signature(
    prepared: &PreparedStudy,
    archive: &crate::domain::evidence::StudyArchive,
) -> AexResult<()> {
    if archive.study_digest == prepared.study_digest
        && archive.baseline_digest == prepared.baseline_digest
        && archive.evaluator_signature == evaluator_signature()
    {
        Ok(())
    } else {
        Err(AexError::validation(
            "STUDY_ARCHIVE_SIGNATURE_MISMATCH",
            "study.id",
            "the latest local archive does not match this study and evaluator",
        ))
    }
}

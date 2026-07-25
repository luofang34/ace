use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::evidence::StudyArchive;
use crate::services::analysis::ApplicationService;
use crate::services::study::evaluation::evaluator_signature;
use crate::services::study::loading::PreparedStudy;

pub(in crate::services::study) fn archive_for_query(
    service: &ApplicationService,
    study_id: &str,
    archive_id: Option<&str>,
) -> AexResult<StudyArchive> {
    if let Some(archive_id) = archive_id {
        let archive = service.studies.load_archive_blocking(archive_id)?;
        if archive.study_id == study_id {
            return Ok(archive);
        }
        return Err(AexError::validation(
            "STUDY_ARCHIVE_MISMATCH",
            "archive_id",
            format!("archive {archive_id} belongs to study {}", archive.study_id),
        ));
    }
    let archives = service
        .studies
        .list_archives_blocking()?
        .into_iter()
        .filter(|archive| archive.study_id == study_id)
        .collect::<Vec<_>>();
    if archives.is_empty() {
        return Err(archive_not_found(study_id));
    }
    let signatures = archives
        .iter()
        .map(|archive| {
            (
                archive.study_digest.as_str(),
                archive.baseline_digest.as_str(),
                archive.evaluator_signature.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    if signatures.len() > 1 {
        return Err(AexError::validation(
            "AMBIGUOUS_STUDY_ARCHIVE",
            "archive_id",
            format!("multiple revisions exist for {study_id}; provide archive_id"),
        ));
    }
    archives
        .into_iter()
        .max_by(compare_progress)
        .ok_or_else(|| archive_not_found(study_id))
}

pub(in crate::services::study) fn archive_for_prepared(
    service: &ApplicationService,
    prepared: &PreparedStudy,
) -> AexResult<StudyArchive> {
    matching_archive(service, prepared)?.ok_or_else(|| {
        AexError::validation(
            "STUDY_ARCHIVE_NOT_FOUND",
            "study.id",
            format!(
                "no matching local archive exists for {}",
                prepared.document.study.id
            ),
        )
    })
}

pub(super) fn matching_archive(
    service: &ApplicationService,
    prepared: &PreparedStudy,
) -> AexResult<Option<StudyArchive>> {
    let signature = evaluator_signature();
    Ok(service
        .studies
        .list_archives_blocking()?
        .into_iter()
        .filter(|archive| {
            archive.study_id == prepared.document.study.id
                && archive.study_digest == prepared.study_digest
                && archive.baseline_digest == prepared.baseline_digest
                && archive.evaluator_signature == signature
        })
        .max_by(compare_progress))
}

fn archive_not_found(study_id: &str) -> AexError {
    AexError::validation(
        "STUDY_ARCHIVE_NOT_FOUND",
        "study_id",
        format!("no local archive exists for {study_id}"),
    )
}

fn compare_progress(left: &StudyArchive, right: &StudyArchive) -> Ordering {
    left.complete
        .cmp(&right.complete)
        .then_with(|| {
            left.workflow
                .outcomes
                .len()
                .cmp(&right.workflow.outcomes.len())
        })
        .then_with(|| {
            left.workflow
                .checkpoint
                .generation
                .cmp(&right.workflow.checkpoint.generation)
        })
        .then_with(|| left.archive_id.cmp(&right.archive_id))
}

#[cfg(test)]
mod tests;

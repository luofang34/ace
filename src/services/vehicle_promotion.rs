use std::path::{Path, PathBuf};

use ace_engine::Feasibility;
use ace_study::StudyArchive;
use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::services::vehicle_document::create_blocking;

/// A verified study candidate written out as an editable vehicle file.
#[derive(Debug, Serialize)]
pub(crate) struct Promotion {
    pub path: PathBuf,
    pub archive_id: String,
    pub candidate_id: String,
    pub model_digest: String,
    pub feasibility: Feasibility,
    pub updates: std::collections::BTreeMap<String, ace_model::Quantity>,
}

/// Verify the archive and candidate evidence, then write the candidate's model to `save_as`.
pub(crate) fn promote_blocking(
    archive: &Path,
    candidate: &str,
    save_as: &Path,
) -> AexResult<Promotion> {
    let content = std::fs::read_to_string(archive).map_err(|source| AexError::Read {
        path: archive.into(),
        source,
    })?;
    let archive: StudyArchive =
        serde_json::from_str(&content).map_err(|source| AexError::Json { source })?;
    archive.verify()?;
    archive.snapshot(candidate)?;
    let selected = archive
        .candidates
        .iter()
        .find(|item| item.id == candidate)
        .ok_or_else(|| {
            AexError::validation("UNKNOWN_CANDIDATE", "candidate", "not in this archive")
        })?;
    if !matches!(
        selected.feasibility,
        Feasibility::Satisfied | Feasibility::Provisional
    ) {
        return Err(AexError::validation(
            "CANDIDATE_NOT_FEASIBLE",
            candidate.to_owned(),
            format!("candidate feasibility is {:?}", selected.feasibility),
        ));
    }
    create_blocking(save_as, &selected.model)?;
    Ok(Promotion {
        path: save_as.to_path_buf(),
        archive_id: archive.id.clone(),
        candidate_id: selected.id.clone(),
        model_digest: selected.state.model_digest.clone(),
        feasibility: selected.feasibility,
        updates: selected.updates.clone(),
    })
}

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::content_identity::{
    content_id, validate_content_id, validate_digest, validate_safe_id,
};
use crate::domain::diagnostic::{AexError, AexResult};

use super::{CandidateDescriptor, require_matching_id};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StudyArchiveDraft {
    pub(crate) study_id: String,
    pub(crate) study_digest: String,
    pub(crate) baseline_digest: String,
    pub(crate) evaluator_signature: String,
    pub(crate) complete: bool,
    pub(crate) candidates: Vec<CandidateDescriptor>,
    pub(crate) evaluation_ids: Vec<String>,
    pub(crate) selected_candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct StudyArchive {
    pub(crate) schema_version: u32,
    pub(crate) archive_id: String,
    pub(crate) study_id: String,
    pub(crate) study_digest: String,
    pub(crate) baseline_digest: String,
    pub(crate) evaluator_signature: String,
    pub(crate) complete: bool,
    pub(crate) candidates: Vec<CandidateDescriptor>,
    pub(crate) evaluation_ids: Vec<String>,
    pub(crate) selected_candidate_ids: Vec<String>,
}

impl StudyArchive {
    pub(crate) fn from_draft(draft: StudyArchiveDraft) -> AexResult<Self> {
        validate_archive_draft(&draft)?;
        let archive_id = content_id("archive_", &draft)?;
        Ok(Self {
            schema_version: 1,
            archive_id,
            study_id: draft.study_id,
            study_digest: draft.study_digest,
            baseline_digest: draft.baseline_digest,
            evaluator_signature: draft.evaluator_signature,
            complete: draft.complete,
            candidates: draft.candidates,
            evaluation_ids: draft.evaluation_ids,
            selected_candidate_ids: draft.selected_candidate_ids,
        })
    }

    pub(crate) fn validate(&self) -> AexResult<()> {
        if self.schema_version != 1 {
            return Err(AexError::validation(
                "UNSUPPORTED_SCHEMA_VERSION",
                "archive.schema_version",
                format!("expected 1, got {}", self.schema_version),
            ));
        }
        validate_content_id(&self.archive_id, "archive_", "archive.archive_id")?;
        let draft = self.as_draft();
        validate_archive_draft(&draft)?;
        let expected = content_id("archive_", &draft)?;
        require_matching_id(&self.archive_id, &expected, "archive.archive_id")
    }

    fn as_draft(&self) -> StudyArchiveDraft {
        StudyArchiveDraft {
            study_id: self.study_id.clone(),
            study_digest: self.study_digest.clone(),
            baseline_digest: self.baseline_digest.clone(),
            evaluator_signature: self.evaluator_signature.clone(),
            complete: self.complete,
            candidates: self.candidates.clone(),
            evaluation_ids: self.evaluation_ids.clone(),
            selected_candidate_ids: self.selected_candidate_ids.clone(),
        }
    }
}

fn validate_archive_draft(draft: &StudyArchiveDraft) -> AexResult<()> {
    validate_safe_id(&draft.study_id, "archive.study_id")?;
    validate_digest(&draft.study_digest, "archive.study_digest")?;
    validate_digest(&draft.baseline_digest, "archive.baseline_digest")?;
    if draft.evaluator_signature.trim().is_empty() {
        return Err(AexError::validation(
            "INVALID_EVALUATOR_SIGNATURE",
            "archive.evaluator_signature",
            "evaluator signature cannot be empty",
        ));
    }
    validate_candidates(draft)?;
    validate_evaluations(draft)?;
    validate_selection(draft)
}

fn validate_candidates(draft: &StudyArchiveDraft) -> AexResult<()> {
    let mut candidate_ids = BTreeSet::new();
    for candidate in &draft.candidates {
        candidate.validate()?;
        if candidate.baseline_digest != draft.baseline_digest
            || !candidate_ids.insert(candidate.candidate_id.as_str())
        {
            return Err(AexError::validation(
                "INVALID_ARCHIVE_CANDIDATE",
                "archive.candidates",
                "candidate ids must be unique and use the archive baseline digest",
            ));
        }
    }
    Ok(())
}

fn validate_evaluations(draft: &StudyArchiveDraft) -> AexResult<()> {
    let mut evaluation_ids = BTreeSet::new();
    for evaluation_id in &draft.evaluation_ids {
        validate_content_id(evaluation_id, "eval_", "archive.evaluation_ids")?;
        if !evaluation_ids.insert(evaluation_id.as_str()) {
            return Err(AexError::validation(
                "DUPLICATE_ARCHIVE_EVALUATION",
                "archive.evaluation_ids",
                "evaluation identifiers must be unique",
            ));
        }
    }
    Ok(())
}

fn validate_selection(draft: &StudyArchiveDraft) -> AexResult<()> {
    let candidate_ids = draft
        .candidates
        .iter()
        .map(|candidate| candidate.candidate_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut selected = BTreeSet::new();
    if draft.selected_candidate_ids.iter().any(|candidate_id| {
        !candidate_ids.contains(candidate_id.as_str()) || !selected.insert(candidate_id.as_str())
    }) {
        return Err(AexError::validation(
            "INVALID_ARCHIVE_SELECTION",
            "archive.selected_candidate_ids",
            "selected candidate ids must be unique members of the archive",
        ));
    }
    Ok(())
}

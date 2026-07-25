mod evaluation;
mod evidence_links;
mod loading;
mod promotion;
mod ranking;
mod search;

use std::path::Path;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::evidence::{EvidenceEnvelope, StudyLoadResult, StudyRunResult};
use crate::services::analysis::ApplicationService;
use crate::storage::design_store::DesignRecord;

impl ApplicationService {
    pub(crate) fn load_study_blocking(&self, study_path: &Path) -> AexResult<StudyLoadResult> {
        let prepared = loading::prepare_study_blocking(self, study_path)?;
        Ok(StudyLoadResult {
            study_id: prepared.document.study.id.clone(),
            name: prepared.document.study.name.clone(),
            study_digest: prepared.study_digest.clone(),
            baseline_digest: prepared.baseline_digest.clone(),
            baseline_scenario_id: prepared.scenario.id.clone(),
            variable_count: prepared.document.study.variables.len(),
            objective_count: prepared.document.study.objectives.len(),
            constraint_count: prepared.document.study.constraints.len(),
            embedded_baseline: prepared.embedded_baseline(),
            selected_candidates: prepared
                .document
                .study
                .selected_candidates
                .iter()
                .map(|selected| selected.candidate.clone())
                .collect(),
        })
    }

    pub(crate) fn run_study_blocking(&self, study_path: &Path) -> AexResult<StudyRunResult> {
        let prepared = loading::prepare_study_blocking(self, study_path)?;
        search::run_blocking(self, &prepared)
    }

    pub(crate) fn query_study_blocking(
        &self,
        study_id: &str,
        archive_id: Option<&str>,
        limit: usize,
    ) -> AexResult<StudyRunResult> {
        let archive = search::archive_for_query(self, study_id, archive_id)?;
        ranking::run_result(self, &archive, 0, limit)
    }

    pub(crate) fn get_study_evidence_blocking(
        &self,
        study_id: &str,
        archive_id: Option<&str>,
        candidate_id: &str,
    ) -> AexResult<EvidenceEnvelope> {
        let archive = search::archive_for_query(self, study_id, archive_id)?;
        let outcome = archive
            .workflow
            .outcomes
            .iter()
            .find(|outcome| outcome.candidate_id == candidate_id)
            .ok_or_else(|| {
                AexError::validation(
                    "STUDY_CANDIDATE_NOT_FOUND",
                    "candidate_id",
                    format!("candidate {candidate_id} is not present in {study_id}"),
                )
            })?;
        evidence_links::load_for_outcome_blocking(self, &archive, outcome)
    }

    pub(crate) fn promote_study_candidate_blocking(
        &self,
        study_path: &Path,
        candidate_id: &str,
        design_id: &str,
        display_name: &str,
        design_root: &Path,
    ) -> AexResult<DesignRecord> {
        let prepared = loading::prepare_study_blocking(self, study_path)?;
        promotion::promote_blocking(
            self,
            &prepared,
            candidate_id,
            design_id,
            display_name,
            design_root,
        )
    }
}

#[cfg(test)]
mod tests;

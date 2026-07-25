use std::path::PathBuf;

use crate::domain::evidence::StudyArchive;
use crate::domain::evidence::archive::StudyArchiveDraft;
use crate::services::analysis::ApplicationService;
use crate::services::study::evaluation::evaluator_signature;
use crate::services::study::loading;

use super::matching_archive;

#[test]
fn stale_evaluator_archives_are_not_reused() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let study = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/study.yaml");
    let prepared = loading::prepare_study_blocking(&service, &study)?;
    let stale = StudyArchive::from_draft(StudyArchiveDraft {
        study_id: prepared.document.study.id.clone(),
        study_digest: prepared.study_digest.clone(),
        baseline_digest: prepared.baseline_digest.clone(),
        evaluator_signature: format!("native-study-evidence-v5:{}", env!("CARGO_PKG_VERSION")),
        complete: true,
        candidates: Vec::new(),
        evaluation_ids: Vec::new(),
        selected_candidate_ids: Vec::new(),
        workflow: Default::default(),
    })?;
    service.studies.save_archive_blocking(&stale)?;

    assert!(evaluator_signature().starts_with("native-study-evidence-v6:"));
    assert!(matching_archive(&service, &prepared)?.is_none());
    Ok(())
}

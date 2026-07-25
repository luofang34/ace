use std::collections::BTreeMap;
use std::fs;

use tempfile::tempdir;

use crate::domain::diagnostic::AexError;
use crate::domain::evidence::{
    CandidateDescriptor, EvaluationStatus, EvidenceAnalysis, EvidenceDraft, EvidenceEnvelope,
    EvidenceProvenance, EvidenceResults, StudyArchive, StudyArchiveDraft,
};
use crate::domain::quantity::QuantityOutput;

use super::{FileStudyStore, StudyRepository};

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn records() -> Result<(EvidenceEnvelope, StudyArchive), Box<dyn std::error::Error>> {
    let candidate = CandidateDescriptor::new(
        digest('a'),
        BTreeMap::from([(
            "aircraft.geometry.wing.area".to_owned(),
            "16 m^2".to_owned(),
        )]),
    )?;
    let evidence = EvidenceEnvelope::from_draft(EvidenceDraft {
        study_id: "wing-trade".to_owned(),
        candidate_id: candidate.candidate_id.clone(),
        input_digest: digest('b'),
        status: EvaluationStatus::Succeeded,
        analysis: EvidenceAnalysis {
            discipline: "mission".to_owned(),
            operating_condition: "design_mission".to_owned(),
            method: "deterministic_integration".to_owned(),
            backend: "native".to_owned(),
            software_version: "ace-test".to_owned(),
            model_versions: BTreeMap::new(),
            fidelity_level: 1,
        },
        results: EvidenceResults {
            metrics: BTreeMap::from([(
                "mission.total_fuel".to_owned(),
                QuantityOutput::si(120.0, "kg"),
            )]),
            constraints: Vec::new(),
            diagnostics: Vec::new(),
        },
        provenance: EvidenceProvenance {
            assumptions: Vec::new(),
            validity_range: Vec::new(),
            confidence: None,
            dependencies: Vec::new(),
            artifacts: Vec::new(),
        },
    })?;
    let archive = StudyArchive::from_draft(StudyArchiveDraft {
        study_id: "wing-trade".to_owned(),
        study_digest: digest('c'),
        baseline_digest: digest('a'),
        evaluator_signature: "ace-test/native".to_owned(),
        complete: true,
        candidates: vec![candidate.clone()],
        evaluation_ids: vec![evidence.evaluation_id.clone()],
        selected_candidate_ids: vec![candidate.candidate_id],
    })?;
    Ok((evidence, archive))
}

#[test]
fn records_are_content_addressed_and_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = FileStudyStore::new(directory.path().to_path_buf());
    let (evidence, archive) = records()?;

    let evaluation_path = store.save_evaluation_blocking(&evidence)?;
    let archive_path = store.save_archive_blocking(&archive)?;
    assert_eq!(store.save_evaluation_blocking(&evidence)?, evaluation_path);
    assert_eq!(store.save_archive_blocking(&archive)?, archive_path);
    assert_eq!(
        store.load_evaluation_blocking(&evidence.evaluation_id)?,
        evidence
    );
    assert_eq!(store.load_archive_blocking(&archive.archive_id)?, archive);
    assert!(evaluation_path.starts_with(directory.path().join("evaluations")));
    assert!(archive_path.starts_with(directory.path().join("archives")));
    assert!(!directory.path().join("candidates").exists());
    Ok(())
}

#[test]
fn changed_evaluation_cannot_replace_stored_content() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = FileStudyStore::new(directory.path().to_path_buf());
    let (evidence, _) = records()?;
    let path = store.save_evaluation_blocking(&evidence)?;
    let original = fs::read(&path)?;
    let mut changed = evidence;
    changed.results.metrics.insert(
        "mission.total_fuel".to_owned(),
        QuantityOutput::si(121.0, "kg"),
    );

    assert!(matches!(
        store.save_evaluation_blocking(&changed),
        Err(AexError::Validation {
            code: "CONTENT_ID_MISMATCH",
            ..
        })
    ));
    assert_eq!(fs::read(path)?, original);
    Ok(())
}

#[test]
fn corrupt_json_and_content_ids_keep_path_context() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = FileStudyStore::new(directory.path().to_path_buf());
    let (_, archive) = records()?;
    let path = store.save_archive_blocking(&archive)?;
    fs::write(&path, b"{not json")?;
    assert!(matches!(
        store.load_archive_blocking(&archive.archive_id),
        Err(AexError::StoredJson {
            path: error_path,
            ..
        }) if error_path == path
    ));

    let mut corrupt = archive;
    corrupt.complete = false;
    fs::write(&path, serde_json::to_vec_pretty(&corrupt)?)?;
    assert!(matches!(
        store.load_archive_blocking(&corrupt.archive_id),
        Err(AexError::StoredRecord {
            path: error_path,
            source,
        }) if error_path == path
            && matches!(*source, AexError::Validation {
                code: "CONTENT_ID_MISMATCH",
                ..
            })
    ));
    Ok(())
}

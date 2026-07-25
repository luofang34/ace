use std::collections::BTreeMap;

use crate::domain::diagnostic::Diagnostic;
use crate::domain::quantity::QuantityOutput;

use super::archive::StudyArchiveDraft;
use super::{
    CandidateDescriptor, CandidateOutcome, ConstraintStatus, EvaluationStatus, EvidenceAnalysis,
    EvidenceConstraint, EvidenceDraft, EvidenceEnvelope, EvidenceProvenance, EvidenceResults,
    StudyArchive, StudyArchiveWorkflow,
};

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn candidate(area: &str) -> Result<CandidateDescriptor, Box<dyn std::error::Error>> {
    CandidateDescriptor::new(
        digest('a'),
        BTreeMap::from([("aircraft.geometry.wing.area".to_owned(), area.to_owned())]),
    )
    .map_err(Into::into)
}

fn evidence(
    candidate_id: String,
    metric_value: f64,
) -> Result<EvidenceEnvelope, Box<dyn std::error::Error>> {
    let draft = EvidenceDraft {
        study_id: "wing-trade".to_owned(),
        candidate_id,
        input_digest: digest('b'),
        status: EvaluationStatus::Succeeded,
        analysis: EvidenceAnalysis {
            discipline: "mission".to_owned(),
            operating_condition: "design_mission".to_owned(),
            method: "deterministic_integration".to_owned(),
            backend: "native".to_owned(),
            software_version: "ace-test".to_owned(),
            model_versions: BTreeMap::from([("mission".to_owned(), "1".to_owned())]),
            fidelity_level: 1,
        },
        results: EvidenceResults {
            metrics: BTreeMap::from([(
                "mission.total_fuel".to_owned(),
                QuantityOutput::si(metric_value, "kg"),
            )]),
            constraints: vec![EvidenceConstraint {
                id: "fuel-floor".to_owned(),
                metric: "mission.total_fuel".to_owned(),
                status: ConstraintStatus::Pass,
                severity: "hard".to_owned(),
                actual: Some(QuantityOutput::si(metric_value, "kg")),
                required: Some(QuantityOutput::si(100.0, "kg")),
                operator: "ge".to_owned(),
                normalized_violation: 0.0,
            }],
            diagnostics: vec![Diagnostic::warning(
                "TEST_WARNING",
                "fixture diagnostic",
                "mission",
            )],
        },
        provenance: EvidenceProvenance {
            assumptions: vec!["standard atmosphere".to_owned()],
            validity_range: vec!["altitude <= 3000 m".to_owned()],
            confidence: Some(0.8),
            dependencies: Vec::new(),
            artifacts: Vec::new(),
        },
    };
    EvidenceEnvelope::from_draft(draft).map_err(Into::into)
}

#[test]
fn candidate_id_is_order_independent_and_change_sensitive() -> Result<(), Box<dyn std::error::Error>>
{
    let first = CandidateDescriptor::new(
        digest('a'),
        BTreeMap::from([
            (
                "aircraft.geometry.wing.area".to_owned(),
                "16 m^2".to_owned(),
            ),
            (
                "aircraft.geometry.wing.aspect_ratio".to_owned(),
                "7.5".to_owned(),
            ),
        ]),
    )?;
    let second = CandidateDescriptor::new(
        digest('a'),
        BTreeMap::from([
            (
                "aircraft.geometry.wing.aspect_ratio".to_owned(),
                "7.5".to_owned(),
            ),
            (
                "aircraft.geometry.wing.area".to_owned(),
                "16 m^2".to_owned(),
            ),
        ]),
    )?;
    let changed = candidate("17 m^2")?;
    let equivalent_units = candidate("16 m2")?;

    assert_eq!(first.candidate_id, second.candidate_id);
    assert_eq!(
        candidate("16 m^2")?.candidate_id,
        equivalent_units.candidate_id
    );
    assert_eq!(
        equivalent_units.parameters["aircraft.geometry.wing.area"],
        "16 m^2"
    );
    assert_ne!(first.candidate_id, changed.candidate_id);
    first.validate()?;
    Ok(())
}

#[test]
fn evidence_id_is_reproducible_and_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let selected_candidate = candidate("16 m^2")?;
    let first = evidence(selected_candidate.candidate_id.clone(), 120.0)?;
    let equivalent = evidence(selected_candidate.candidate_id, 120.0)?;
    let changed = evidence(candidate("16 m^2")?.candidate_id, 121.0)?;

    assert_eq!(first.evaluation_id, equivalent.evaluation_id);
    assert_ne!(first.evaluation_id, changed.evaluation_id);
    let round_trip: EvidenceEnvelope = serde_json::from_slice(&serde_json::to_vec(&first)?)?;
    round_trip.validate()?;
    assert_eq!(first, round_trip);
    Ok(())
}

#[test]
fn archive_id_covers_completion_and_selection() -> Result<(), Box<dyn std::error::Error>> {
    let candidate = candidate("16 m^2")?;
    let evidence = evidence(candidate.candidate_id.clone(), 120.0)?;
    let draft = StudyArchiveDraft {
        study_id: "wing-trade".to_owned(),
        study_digest: digest('c'),
        baseline_digest: digest('a'),
        evaluator_signature: "ace-test/native".to_owned(),
        complete: true,
        candidates: vec![candidate.clone()],
        evaluation_ids: vec![evidence.evaluation_id],
        selected_candidate_ids: vec![candidate.candidate_id.clone()],
        workflow: Default::default(),
    };
    let first = StudyArchive::from_draft(draft)?;
    let changed = StudyArchive::from_draft(StudyArchiveDraft {
        study_id: first.study_id.clone(),
        study_digest: first.study_digest.clone(),
        baseline_digest: first.baseline_digest.clone(),
        evaluator_signature: first.evaluator_signature.clone(),
        complete: false,
        candidates: first.candidates.clone(),
        evaluation_ids: first.evaluation_ids.clone(),
        selected_candidate_ids: first.selected_candidate_ids.clone(),
        workflow: first.workflow.clone(),
    })?;

    assert_ne!(first.archive_id, changed.archive_id);
    assert!(serde_json::to_value(&first)?.get("workflow").is_none());
    first.validate()?;
    Ok(())
}

#[test]
fn archive_workflow_round_trips_and_covers_archive_members()
-> Result<(), Box<dyn std::error::Error>> {
    let evaluated_candidate = candidate("16 m^2")?;
    let unevaluated_candidate = candidate("17 m^2")?;
    let evidence = evidence(evaluated_candidate.candidate_id.clone(), 120.0)?;
    let workflow = StudyArchiveWorkflow {
        outcomes: vec![CandidateOutcome {
            candidate_id: evaluated_candidate.candidate_id.clone(),
            evaluation_id: evidence.evaluation_id.clone(),
            generation: 2,
            feasible: true,
            normalized_constraint_violation: 0.0,
            objective_values: BTreeMap::from([("fuel".to_owned(), 120.0)]),
            rank_score: 0.25,
        }],
        pareto_candidate_ids: vec![evaluated_candidate.candidate_id.clone()],
        ..StudyArchiveWorkflow::default()
    };
    let archive = StudyArchive::from_draft(StudyArchiveDraft {
        study_id: "wing-trade".to_owned(),
        study_digest: digest('c'),
        baseline_digest: digest('a'),
        evaluator_signature: "ace-test/native".to_owned(),
        complete: true,
        candidates: vec![evaluated_candidate.clone()],
        evaluation_ids: vec![evidence.evaluation_id.clone()],
        selected_candidate_ids: vec![evaluated_candidate.candidate_id.clone()],
        workflow: workflow.clone(),
    })?;
    let round_trip: StudyArchive = serde_json::from_slice(&serde_json::to_vec(&archive)?)?;
    round_trip.validate()?;
    assert_eq!(round_trip, archive);

    let invalid = StudyArchive::from_draft(StudyArchiveDraft {
        study_id: "wing-trade".to_owned(),
        study_digest: digest('c'),
        baseline_digest: digest('a'),
        evaluator_signature: "ace-test/native".to_owned(),
        complete: false,
        candidates: vec![evaluated_candidate, unevaluated_candidate.clone()],
        evaluation_ids: vec![evidence.evaluation_id],
        selected_candidate_ids: vec![unevaluated_candidate.candidate_id],
        workflow,
    });
    assert!(matches!(
        invalid,
        Err(crate::domain::diagnostic::AexError::Validation {
            code: "INVALID_ARCHIVE_OUTCOME",
            ..
        })
    ));
    Ok(())
}

use std::collections::BTreeMap;

use crate::domain::evidence::archive::StudyArchiveDraft;
use crate::domain::evidence::{
    CandidateDescriptor, CandidateOutcome, EvaluationStatus, EvidenceAnalysis, EvidenceDraft,
    EvidenceEnvelope, EvidenceProvenance, EvidenceResults, StudyArchive, StudyArchiveWorkflow,
};

use super::validate_link;

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn candidate(value: &str) -> Result<CandidateDescriptor, Box<dyn std::error::Error>> {
    CandidateDescriptor::new(
        digest('a'),
        BTreeMap::from([("aircraft.geometry.wing.area".to_owned(), value.to_owned())]),
    )
    .map_err(Into::into)
}

fn evidence(candidate_id: String) -> Result<EvidenceEnvelope, Box<dyn std::error::Error>> {
    EvidenceEnvelope::from_draft(EvidenceDraft {
        study_id: "evidence-links".to_owned(),
        candidate_id,
        input_digest: digest('b'),
        status: EvaluationStatus::Succeeded,
        analysis: EvidenceAnalysis {
            discipline: "study".to_owned(),
            operating_condition: "mission".to_owned(),
            method: "native".to_owned(),
            backend: "native".to_owned(),
            software_version: "test".to_owned(),
            model_versions: BTreeMap::new(),
            fidelity_level: 1,
        },
        results: EvidenceResults {
            metrics: BTreeMap::new(),
            metric_validity: BTreeMap::new(),
            constraints: Vec::new(),
            diagnostics: Vec::new(),
        },
        provenance: EvidenceProvenance {
            assumptions: Vec::new(),
            validity_range: Vec::new(),
            validity_domains: Vec::new(),
            confidence: None,
            dependencies: Vec::new(),
            artifacts: Vec::new(),
        },
    })
    .map_err(Into::into)
}

#[test]
fn candidate_and_feasibility_links_must_match_evidence() -> Result<(), Box<dyn std::error::Error>> {
    let first = candidate("16 m^2")?;
    let second = candidate("17 m^2")?;
    let first_evidence = evidence(first.candidate_id.clone())?;
    let second_evidence = evidence(second.candidate_id.clone())?;
    let outcome = CandidateOutcome {
        candidate_id: first.candidate_id.clone(),
        evaluation_id: first_evidence.evaluation_id.clone(),
        generation: 0,
        feasible: true,
        normalized_constraint_violation: 0.0,
        objective_values: BTreeMap::new(),
        rank_score: 0.0,
    };
    let archive = StudyArchive::from_draft(StudyArchiveDraft {
        study_id: "evidence-links".to_owned(),
        study_digest: digest('c'),
        baseline_digest: digest('a'),
        evaluator_signature: "native-test".to_owned(),
        complete: true,
        candidates: vec![first.clone()],
        evaluation_ids: vec![first_evidence.evaluation_id.clone()],
        selected_candidate_ids: vec![first.candidate_id.clone()],
        workflow: StudyArchiveWorkflow {
            outcomes: vec![outcome.clone()],
            pareto_candidate_ids: vec![first.candidate_id.clone()],
            ..StudyArchiveWorkflow::default()
        },
    })?;

    validate_link(&archive, &outcome, &first, &first_evidence)?;
    assert!(validate_link(&archive, &outcome, &first, &second_evidence).is_err());
    let infeasible = CandidateOutcome {
        feasible: false,
        ..outcome
    };
    assert!(validate_link(&archive, &infeasible, &first, &first_evidence).is_err());
    Ok(())
}

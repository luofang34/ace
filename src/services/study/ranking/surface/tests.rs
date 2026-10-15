#![allow(clippy::expect_used)]

use std::collections::BTreeMap;

use crate::domain::evidence::{
    CandidateDescriptor, CandidateOutcome, OptimizerCheckpoint, StudyArchive, StudyArchiveWorkflow,
};

use super::from_archive;

fn archive(generation: u32) -> StudyArchive {
    let points = [
        ("c1", "1 m", "10", 110.0, true),
        ("c2", "1 m", "20", 120.0, true),
        ("c3", "2 m", "10", 210.0, false),
        ("c4", "2 m", "20", 220.0, true),
    ];
    StudyArchive {
        schema_version: 1,
        archive_id: "archive_surface".to_owned(),
        study_id: "surface".to_owned(),
        study_digest: "a".repeat(64),
        baseline_digest: "b".repeat(64),
        evaluator_signature: "test".to_owned(),
        complete: true,
        candidates: points
            .iter()
            .map(|(id, x, y, _, _)| CandidateDescriptor {
                candidate_id: (*id).to_owned(),
                baseline_digest: "b".repeat(64),
                parameters: BTreeMap::from([
                    ("design.x".to_owned(), (*x).to_owned()),
                    ("design.y".to_owned(), (*y).to_owned()),
                ]),
            })
            .collect(),
        evaluation_ids: points
            .iter()
            .map(|(id, _, _, _, _)| format!("eval_{id}"))
            .collect(),
        selected_candidate_ids: Vec::new(),
        workflow: StudyArchiveWorkflow {
            checkpoint: OptimizerCheckpoint {
                generation,
                rng_state: 0,
                population: Vec::new(),
            },
            outcomes: points
                .iter()
                .map(|(id, _, _, value, feasible)| CandidateOutcome {
                    candidate_id: (*id).to_owned(),
                    evaluation_id: format!("eval_{id}"),
                    generation: 0,
                    feasible: *feasible,
                    normalized_constraint_violation: if *feasible { 0.0 } else { 1.0 },
                    objective_values: BTreeMap::from([("fuel".to_owned(), *value)]),
                    rank_score: *value,
                })
                .collect(),
            pareto_candidate_ids: Vec::new(),
        },
    }
}

#[test]
fn complete_grid_archive_becomes_row_major_surface() {
    let surface = from_archive(&archive(0)).expect("complete grid");

    assert_eq!(surface.x_values, [1.0, 2.0]);
    assert_eq!(surface.y_values, [10.0, 20.0]);
    assert_eq!(surface.values, [110.0, 210.0, 120.0, 220.0]);
    assert_eq!(surface.feasible_mask, [true, false, true, true]);
}

#[test]
fn evolutionary_archive_never_fabricates_a_surface() {
    assert!(from_archive(&archive(1)).is_none());
}

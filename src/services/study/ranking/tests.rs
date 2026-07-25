use std::collections::BTreeMap;

use crate::domain::evidence::{CandidateOutcome, StudyArchiveWorkflow};
use crate::domain::study::{
    ObjectiveDirection, StudyAnalysisPolicy, StudyBaseline, StudyDefinition, StudyObjective,
    StudySearchPolicy,
};

use super::rank;

fn study() -> StudyDefinition {
    StudyDefinition {
        id: "feasible-first".to_owned(),
        name: "Feasible first".to_owned(),
        baseline: StudyBaseline {
            scenario_path: Some("scenario.yaml".into()),
            embedded: None,
        },
        variables: Vec::new(),
        derived_parameters: Vec::new(),
        objectives: vec![StudyObjective {
            id: "fuel".to_owned(),
            metric: "mission.total_fuel".to_owned(),
            direction: ObjectiveDirection::Minimize,
            weight: 1.0,
        }],
        constraints: Vec::new(),
        selected_candidates: Vec::new(),
        analysis: StudyAnalysisPolicy {
            refinement_candidate_limit: 2,
            ..StudyAnalysisPolicy::default()
        },
        search: StudySearchPolicy::default(),
    }
}

fn outcome(id: &str, feasible: bool, violation: f64, fuel: f64) -> CandidateOutcome {
    CandidateOutcome {
        candidate_id: id.to_owned(),
        evaluation_id: format!("eval_{id}"),
        generation: 0,
        feasible,
        normalized_constraint_violation: violation,
        objective_values: BTreeMap::from([("fuel".to_owned(), fuel)]),
        rank_score: 0.0,
    }
}

#[test]
fn infeasible_candidate_never_outranks_feasible_candidate() {
    let mut workflow = StudyArchiveWorkflow {
        outcomes: vec![
            outcome("infeasible", false, 0.001, 1.0),
            outcome("feasible", true, 0.0, 100.0),
        ],
        ..StudyArchiveWorkflow::default()
    };
    let mut selected = Vec::new();

    rank(&study(), &mut workflow, &mut selected);

    assert_eq!(selected, ["feasible"]);
    assert_eq!(workflow.pareto_candidate_ids, ["feasible"]);
}

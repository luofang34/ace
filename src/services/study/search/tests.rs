use std::collections::BTreeMap;

use crate::domain::study::{
    StudyAnalysisPolicy, StudyBaseline, StudyDefinition, StudySearchPolicy, StudyVariable,
    StudyVariableKind,
};

use super::{DeterministicRng, grid_genes};

fn study() -> StudyDefinition {
    StudyDefinition {
        id: "deterministic-grid".to_owned(),
        name: "Deterministic grid".to_owned(),
        baseline: StudyBaseline {
            scenario_path: Some("scenario.yaml".into()),
            embedded: None,
        },
        variables: vec![
            StudyVariable {
                id: "area".to_owned(),
                path: "aircraft.geometry.wing.area".to_owned(),
                kind: StudyVariableKind::Continuous,
                values: vec!["10 m^2".to_owned(), "20 m^2".to_owned()],
                active_when: BTreeMap::new(),
            },
            StudyVariable {
                id: "engines".to_owned(),
                path: "aircraft.propulsion.engine_count".to_owned(),
                kind: StudyVariableKind::Integer,
                values: vec!["1".to_owned(), "2".to_owned()],
                active_when: BTreeMap::new(),
            },
        ],
        derived_parameters: Vec::new(),
        objectives: Vec::new(),
        constraints: Vec::new(),
        selected_candidates: Vec::new(),
        analysis: StudyAnalysisPolicy::default(),
        search: StudySearchPolicy::default(),
    }
}

#[test]
fn grid_order_and_limit_are_deterministic() {
    let first = grid_genes(&study(), 3);
    let second = grid_genes(&study(), 3);

    assert_eq!(first, second);
    assert_eq!(first.len(), 3);
    assert_eq!(first[0]["area"], "10 m^2");
    assert_eq!(first[0]["engines"], "1");
    assert_eq!(first[1]["engines"], "2");
    assert_eq!(first[2]["area"], "20 m^2");
}

#[test]
fn seeded_rng_repeats_and_handles_empty_choice() {
    let mut first = DeterministicRng::new(17);
    let mut second = DeterministicRng::new(17);
    let first_values = (0..8).map(|_| first.index(11)).collect::<Vec<_>>();
    let second_values = (0..8).map(|_| second.index(11)).collect::<Vec<_>>();

    assert_eq!(first_values, second_values);
    assert_eq!(first.index(0), 0);
}

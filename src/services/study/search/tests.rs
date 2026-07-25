use std::collections::{BTreeMap, BTreeSet};

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
    let first = grid_genes(&study()).take(3).collect::<Vec<_>>();
    let second = grid_genes(&study()).take(3).collect::<Vec<_>>();

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

#[test]
fn conditional_duplicates_do_not_truncate_later_grid_branches()
-> Result<(), Box<dyn std::error::Error>> {
    let mut conditional = study();
    conditional.variables[1].active_when =
        BTreeMap::from([("area".to_owned(), "20 m^2".to_owned())]);
    let mut unique = BTreeSet::new();
    let mut raw_rows = 0_usize;

    for genes in grid_genes(&conditional) {
        raw_rows = raw_rows.wrapping_add(1);
        let active = conditional
            .variables
            .iter()
            .filter(|variable| {
                variable
                    .active_when
                    .iter()
                    .all(|(id, value)| genes.get(id) == Some(value))
            })
            .filter_map(|variable| {
                genes
                    .get(&variable.id)
                    .map(|value| (variable.id.clone(), value.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        unique.insert(serde_json::to_string(&active)?);
        if unique.len() == 3 {
            break;
        }
    }

    assert_eq!(unique.len(), 3);
    assert_eq!(raw_rows, 4);
    Ok(())
}

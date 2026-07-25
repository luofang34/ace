use std::collections::BTreeMap;

use crate::domain::quantity::QuantityOutput;
use crate::domain::result::RequirementEvaluation;
use crate::domain::schema::Requirement;

use super::insert_metric_aliases;

fn declared_requirement(id: &str, severity: &str) -> Requirement {
    Requirement {
        id: id.to_owned(),
        metric: "mission.completed_distance".to_owned(),
        operator: "ge".to_owned(),
        required: 5.0,
        unit: "m".to_owned(),
        severity: severity.to_owned(),
        weight: None,
    }
}

fn evaluated_requirement(id: &str, severity: &str, passed: bool) -> RequirementEvaluation {
    RequirementEvaluation {
        id: id.to_owned(),
        metric: "mission.completed_distance".to_owned(),
        actual: QuantityOutput::si(10.0, "m"),
        required: QuantityOutput::si(5.0, "m"),
        operator: "ge".to_owned(),
        passed,
        absolute_margin: 5.0,
        percentage_margin: Some(100.0),
        severity: severity.to_owned(),
        warning_state: false,
    }
}

#[test]
fn study_headline_metric_is_gated_by_mission_completion() {
    let declared = [
        declared_requirement("range", "hard"),
        declared_requirement("mission_completion", "soft"),
    ];
    let evaluations = [
        evaluated_requirement("range", "hard", true),
        evaluated_requirement("mission_completion", "soft", false),
    ];
    let mut incomplete = BTreeMap::from([(
        "mission.completed".to_owned(),
        QuantityOutput::si(0.0, "bool"),
    )]);
    insert_metric_aliases(&mut incomplete, &declared, &evaluations);
    assert_eq!(incomplete["feasibility.hard_constraints_passed"].value, 0.0);

    let mut completed = BTreeMap::from([(
        "mission.completed".to_owned(),
        QuantityOutput::si(1.0, "bool"),
    )]);
    insert_metric_aliases(&mut completed, &declared, &evaluations);
    assert_eq!(completed["feasibility.hard_constraints_passed"].value, 1.0);
}

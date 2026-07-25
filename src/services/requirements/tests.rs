use crate::domain::quantity::QuantityOutput;
use crate::domain::result::RequirementEvaluation;
use crate::domain::schema::Requirement;

use super::{failed_hard_requirement_ids, hard_requirement_counts, hard_requirements_passed};

fn declared(id: &str, severity: &str) -> Requirement {
    Requirement {
        id: id.to_owned(),
        metric: "mission.completed_distance".to_owned(),
        operator: "ge".to_owned(),
        required: 1.0,
        unit: "m".to_owned(),
        severity: severity.to_owned(),
        weight: None,
    }
}

fn evaluated(id: &str, severity: &str, passed: bool) -> RequirementEvaluation {
    RequirementEvaluation {
        id: id.to_owned(),
        metric: "mission.completed_distance".to_owned(),
        actual: QuantityOutput::si(1.0, "m"),
        required: QuantityOutput::si(1.0, "m"),
        operator: "ge".to_owned(),
        passed,
        absolute_margin: 0.0,
        percentage_margin: Some(0.0),
        severity: severity.to_owned(),
        warning_state: false,
    }
}

#[test]
fn headline_verdict_requires_completion_and_every_hard_requirement() {
    let declared = [declared("range", "hard"), declared("ceiling", "soft")];
    let passing = [
        evaluated("range", "hard", true),
        evaluated("ceiling", "soft", false),
    ];
    assert!(hard_requirements_passed(true, &declared, &passing));
    assert!(!hard_requirements_passed(false, &declared, &passing));

    let failing = [evaluated("range", "hard", false)];
    assert!(!hard_requirements_passed(true, &declared, &failing));
    assert!(!hard_requirements_passed(true, &declared, &[]));
    assert_eq!(hard_requirement_counts(&declared, &passing), (1, 1));
    assert_eq!(
        failed_hard_requirement_ids(&declared, &[]),
        ["range".to_owned()]
    );
}

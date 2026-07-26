use std::collections::BTreeMap;

use crate::domain::evidence::{ConstraintStatus, EvidenceConstraint};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{RequirementEvaluation, RequirementStatus};
use crate::domain::schema::{Requirement, RequirementProvenance};
use crate::domain::validity::MetricValidity;

use super::{candidate_is_feasible, insert_metric_aliases};

fn declared_requirement(id: &str, severity: &str) -> Requirement {
    Requirement {
        id: id.to_owned(),
        metric: "mission.completed_distance".to_owned(),
        operator: "ge".to_owned(),
        required: 5.0,
        unit: "m".to_owned(),
        severity: severity.to_owned(),
        weight: None,
        provenance: RequirementProvenance {
            kind: "test".to_owned(),
            source: "test fixture".to_owned(),
            citation: None,
            non_regulatory: true,
            template_id: None,
            template_version: None,
        },
    }
}

fn evaluated_requirement(id: &str, severity: &str, passed: bool) -> RequirementEvaluation {
    RequirementEvaluation {
        id: id.to_owned(),
        metric: "mission.completed_distance".to_owned(),
        actual: QuantityOutput::si(10.0, "m"),
        required: QuantityOutput::si(5.0, "m"),
        operator: "ge".to_owned(),
        status: Some(if passed {
            RequirementStatus::Pass
        } else {
            RequirementStatus::Fail
        }),
        passed: Some(passed),
        validity: MetricValidity::default(),
        absolute_margin: 5.0,
        percentage_margin: Some(100.0),
        severity: severity.to_owned(),
        warning_state: false,
        provenance: None,
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

#[test]
fn hard_indeterminate_requirement_cannot_be_feasible() {
    let declared = [declared_requirement("ceiling", "hard")];
    let mut evaluation = evaluated_requirement("ceiling", "hard", true);
    evaluation.status = Some(RequirementStatus::Indeterminate);
    evaluation.passed = None;
    let constraint = EvidenceConstraint {
        id: "ceiling".to_owned(),
        metric: "performance.service_ceiling".to_owned(),
        status: ConstraintStatus::Indeterminate,
        severity: "hard".to_owned(),
        actual: Some(QuantityOutput::si(19_900.0, "m")),
        required: Some(QuantityOutput::si(24_384.0, "m")),
        operator: "ge".to_owned(),
        normalized_violation: 1.0,
    };

    assert!(!candidate_is_feasible(
        &declared,
        &[evaluation],
        true,
        true,
        &[constraint],
        true,
    ));
}

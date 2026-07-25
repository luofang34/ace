use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{RequirementEvaluation, RequirementStatus};
use crate::domain::schema::Requirement;
use crate::domain::validity::{MetricValidity, ValidityStatus};
use crate::models::performance::PointAnalyzer;
use crate::test_support::example_scenario;

use super::{
    MetricInput, evaluate_one, failed_hard_requirement_ids, hard_requirement_counts,
    hard_requirements_passed,
};

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
        status: Some(if passed {
            RequirementStatus::Pass
        } else {
            RequirementStatus::Fail
        }),
        passed: Some(passed),
        validity: MetricValidity::default(),
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

#[test]
fn sr71_boundary_ceiling_requirement_is_indeterminate_and_nullable()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("sr71")?;
    let performance = PointAnalyzer::new(scenario.clone()).summary()?;
    let requirement = scenario
        .requirements
        .items
        .iter()
        .find(|item| item.metric == "performance.service_ceiling")
        .ok_or("missing SR-71 service-ceiling requirement")?;
    let evaluation = evaluate_one(
        requirement,
        MetricInput {
            actual: performance.service_ceiling_m,
            validity: performance.validity_for("performance.service_ceiling"),
        },
    );

    assert_eq!(
        evaluation.resolved_status(),
        RequirementStatus::Indeterminate
    );
    assert_eq!(evaluation.passed, None);
    assert_eq!(evaluation.validity.status, ValidityStatus::BoundaryLimited);
    let stored = serde_json::to_value(&evaluation)?;
    assert_eq!(stored["status"], "indeterminate");
    assert!(stored["passed"].is_null());
    assert!(!hard_requirements_passed(
        true,
        &scenario.requirements.items,
        &[evaluation]
    ));
    Ok(())
}

#[test]
fn legacy_boolean_requirement_result_remains_readable() -> Result<(), Box<dyn std::error::Error>> {
    let mut stored = serde_json::to_value(evaluated("range", "hard", true))?;
    let object = stored
        .as_object_mut()
        .ok_or("requirement result must be an object")?;
    object.remove("status");
    object.remove("validity");
    let restored: RequirementEvaluation = serde_json::from_value(stored)?;

    assert_eq!(restored.passed, Some(true));
    assert_eq!(restored.resolved_status(), RequirementStatus::Pass);
    assert_eq!(restored.validity, MetricValidity::default());
    Ok(())
}

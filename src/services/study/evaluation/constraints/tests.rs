use std::collections::BTreeMap;

use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{RequirementEvaluation, RequirementStatus};
use crate::domain::study::StudyConstraint;
use crate::domain::validity::{MetricValidity, ValidityStatus};

use super::{collect, constraint_margin, constraint_passed};

#[test]
fn strict_inequality_operators_use_their_declared_direction()
-> Result<(), Box<dyn std::error::Error>> {
    assert!(constraint_margin(2.0, 1.0, "gt")? > 0.0);
    assert!(constraint_margin(1.0, 2.0, "lt")? > 0.0);
    assert!(constraint_margin(1.0, 2.0, "gt")? < 0.0);
    assert!(constraint_margin(2.0, 1.0, "lt")? < 0.0);
    assert!(!constraint_passed(1.0, 1.0, "gt", 0.0));
    assert!(!constraint_passed(1.0, 1.0, "lt", 0.0));
    Ok(())
}

#[test]
fn study_constraint_replaces_matching_baseline_requirement()
-> Result<(), Box<dyn std::error::Error>> {
    let requirement = RequirementEvaluation {
        id: "field_length".to_owned(),
        metric: "performance.takeoff_field_length".to_owned(),
        actual: QuantityOutput::si(15.0, "m"),
        required: QuantityOutput::si(25.0, "m"),
        operator: "le".to_owned(),
        status: Some(RequirementStatus::Pass),
        passed: Some(true),
        validity: MetricValidity::default(),
        absolute_margin: 10.0,
        percentage_margin: Some(40.0),
        severity: "soft".to_owned(),
        warning_state: false,
    };
    let study = StudyConstraint {
        id: "field_length".to_owned(),
        metric: "performance.takeoff_field_length".to_owned(),
        operator: "le".to_owned(),
        value: "20 m".to_owned(),
        severity: "hard".to_owned(),
        weight: 1.0,
    };
    let metrics = BTreeMap::from([(
        "performance.takeoff_field_length".to_owned(),
        QuantityOutput::si(15.0, "m"),
    )]);

    let constraints = collect(&[requirement], &[], &[study], &metrics, &BTreeMap::new())?;

    assert_eq!(constraints.len(), 1);
    assert_eq!(constraints[0].severity, "hard");
    assert_eq!(
        constraints[0].required.as_ref().map(|value| value.value),
        Some(20.0)
    );
    Ok(())
}

#[test]
fn unitless_constraint_accepts_exactly_one() -> Result<(), Box<dyn std::error::Error>> {
    let study = StudyConstraint {
        id: "load_factor".to_owned(),
        metric: "performance.load_factor".to_owned(),
        operator: "ge".to_owned(),
        value: "1".to_owned(),
        severity: "hard".to_owned(),
        weight: 1.0,
    };
    let metrics = BTreeMap::from([(
        "performance.load_factor".to_owned(),
        QuantityOutput::si(1.0, "1"),
    )]);

    let constraints = collect(&[], &[], &[study], &metrics, &BTreeMap::new())?;

    assert_eq!(
        constraints[0].status,
        crate::domain::evidence::ConstraintStatus::Pass
    );
    Ok(())
}

#[test]
fn fuel_failure_kinds_remain_distinct_in_study_evidence() -> Result<(), Box<dyn std::error::Error>>
{
    let constraints = collect(
        &[],
        &["fuel_exhausted".to_owned(), "fuel_capacity".to_owned()],
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )?;

    assert_eq!(constraints.len(), 2);
    assert!(constraints.iter().any(|constraint| {
        constraint.metric == "fuel_exhausted"
            && constraint.status == crate::domain::evidence::ConstraintStatus::Fail
            && constraint.severity == "hard"
    }));
    assert!(
        constraints
            .iter()
            .any(|constraint| constraint.metric == "fuel_capacity")
    );
    assert_ne!(constraints[0].id, constraints[1].id);
    Ok(())
}

#[test]
fn boundary_limited_study_constraint_is_indeterminate_with_violation()
-> Result<(), Box<dyn std::error::Error>> {
    let study = StudyConstraint {
        id: "ceiling".to_owned(),
        metric: "performance.service_ceiling".to_owned(),
        operator: "ge".to_owned(),
        value: "24000 m".to_owned(),
        severity: "hard".to_owned(),
        weight: 2.0,
    };
    let metrics = BTreeMap::from([(study.metric.clone(), QuantityOutput::si(19_900.0, "m"))]);
    let validity = BTreeMap::from([(
        study.metric.clone(),
        MetricValidity::boundary_limited("atmosphere.isa1976.maximum_altitude"),
    )]);
    let constraints = collect(&[], &[], &[study], &metrics, &validity)?;

    assert_eq!(
        constraints[0].status,
        crate::domain::evidence::ConstraintStatus::Indeterminate
    );
    assert_eq!(constraints[0].normalized_violation, 2.0);
    assert_eq!(
        validity["performance.service_ceiling"].status,
        ValidityStatus::BoundaryLimited
    );
    Ok(())
}

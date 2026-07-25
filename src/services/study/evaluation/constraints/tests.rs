use std::collections::BTreeMap;

use crate::domain::quantity::QuantityOutput;
use crate::domain::result::RequirementEvaluation;
use crate::domain::study::StudyConstraint;

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
        passed: true,
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

    let constraints = collect(&[requirement], &[], &[study], &metrics)?;

    assert_eq!(constraints.len(), 1);
    assert_eq!(constraints[0].severity, "hard");
    assert_eq!(
        constraints[0].required.as_ref().map(|value| value.value),
        Some(20.0)
    );
    Ok(())
}

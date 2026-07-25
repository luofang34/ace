use std::collections::{BTreeMap, BTreeSet};

use crate::domain::content_identity::digest_serializable;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::evidence::{ConstraintStatus, EvidenceConstraint};
use crate::domain::quantity::{Dimension, QuantityOutput, parse_quantity};
use crate::domain::result::{RequirementEvaluation, RequirementStatus};
use crate::domain::study::StudyConstraint;
use crate::domain::validity::{MetricValidity, ValidityStatus};

pub(super) fn collect(
    requirements: &[RequirementEvaluation],
    failed_model_constraints: &[String],
    study_constraints: &[StudyConstraint],
    metrics: &BTreeMap<String, QuantityOutput>,
    metric_validity: &BTreeMap<String, MetricValidity>,
) -> AexResult<Vec<EvidenceConstraint>> {
    let mut constraints = requirement_constraints(requirements);
    constraints.extend(model_constraints(failed_model_constraints, &constraints)?);
    for additional in additional_constraints(study_constraints, metrics, metric_validity)? {
        if let Some(index) = constraints
            .iter()
            .position(|constraint| constraint.id == additional.id)
        {
            constraints[index] = additional;
        } else {
            constraints.push(additional);
        }
    }
    Ok(constraints)
}

fn requirement_constraints(requirements: &[RequirementEvaluation]) -> Vec<EvidenceConstraint> {
    requirements
        .iter()
        .map(|requirement| EvidenceConstraint {
            id: requirement.id.clone(),
            metric: requirement.metric.clone(),
            status: match requirement.resolved_status() {
                RequirementStatus::Pass => ConstraintStatus::Pass,
                RequirementStatus::Fail => ConstraintStatus::Fail,
                RequirementStatus::Indeterminate => ConstraintStatus::Indeterminate,
            },
            severity: requirement.severity.clone(),
            actual: Some(requirement.actual.clone()),
            required: Some(requirement.required.clone()),
            operator: requirement.operator.clone(),
            normalized_violation: match requirement.resolved_status() {
                RequirementStatus::Pass => 0.0,
                RequirementStatus::Fail => {
                    -requirement.absolute_margin / requirement.required.value.abs().max(1.0e-12)
                }
                RequirementStatus::Indeterminate => 1.0,
            },
        })
        .collect()
}

fn model_constraints(
    failed: &[String],
    existing: &[EvidenceConstraint],
) -> AexResult<Vec<EvidenceConstraint>> {
    let existing_ids = existing
        .iter()
        .map(|constraint| constraint.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = existing_ids
        .iter()
        .map(|id| (*id).to_owned())
        .collect::<BTreeSet<_>>();
    let mut constraints = Vec::new();
    for id in failed {
        if existing_ids.contains(id.as_str()) {
            continue;
        }
        let evidence_id = model_constraint_id(id)?;
        if seen.insert(evidence_id.clone()) {
            constraints.push(EvidenceConstraint {
                id: evidence_id,
                metric: id.clone(),
                status: ConstraintStatus::Fail,
                severity: "hard".to_owned(),
                actual: None,
                required: None,
                operator: "model_gate".to_owned(),
                normalized_violation: 1.0,
            });
        }
    }
    Ok(constraints)
}

fn model_constraint_id(id: &str) -> AexResult<String> {
    let readable = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .take(58)
        .collect::<String>();
    let digest = digest_serializable(id)?;
    Ok(format!("model-{readable}-{}", &digest[..12]))
}

fn additional_constraints(
    constraints: &[StudyConstraint],
    metrics: &BTreeMap<String, QuantityOutput>,
    metric_validity: &BTreeMap<String, MetricValidity>,
) -> AexResult<Vec<EvidenceConstraint>> {
    constraints
        .iter()
        .map(|constraint| additional_constraint(constraint, metrics, metric_validity))
        .collect()
}

fn additional_constraint(
    constraint: &StudyConstraint,
    metrics: &BTreeMap<String, QuantityOutput>,
    metric_validity: &BTreeMap<String, MetricValidity>,
) -> AexResult<EvidenceConstraint> {
    let Some(actual) = metrics.get(&constraint.metric) else {
        return Ok(EvidenceConstraint {
            id: constraint.id.clone(),
            metric: constraint.metric.clone(),
            status: ConstraintStatus::Indeterminate,
            severity: constraint.severity.clone(),
            actual: None,
            required: None,
            operator: constraint.operator.clone(),
            normalized_violation: constraint.weight,
        });
    };
    let required_value = parse_required(&constraint.value, &actual.unit)?;
    if metric_validity
        .get(&constraint.metric)
        .is_some_and(|validity| validity.status == ValidityStatus::BoundaryLimited)
    {
        return Ok(EvidenceConstraint {
            id: constraint.id.clone(),
            metric: constraint.metric.clone(),
            status: ConstraintStatus::Indeterminate,
            severity: constraint.severity.clone(),
            actual: Some(actual.clone()),
            required: Some(QuantityOutput::si(required_value, &actual.unit)),
            operator: constraint.operator.clone(),
            normalized_violation: constraint.weight,
        });
    }
    let margin = constraint_margin(actual.value, required_value, &constraint.operator)?;
    let passed = constraint_passed(actual.value, required_value, &constraint.operator, margin);
    Ok(EvidenceConstraint {
        id: constraint.id.clone(),
        metric: constraint.metric.clone(),
        status: if passed {
            ConstraintStatus::Pass
        } else {
            ConstraintStatus::Fail
        },
        severity: constraint.severity.clone(),
        actual: Some(actual.clone()),
        required: Some(QuantityOutput::si(required_value, &actual.unit)),
        operator: constraint.operator.clone(),
        normalized_violation: if passed {
            0.0
        } else {
            constraint.weight * -margin / required_value.abs().max(1.0e-12)
        },
    })
}

fn parse_required(raw: &str, unit: &str) -> AexResult<f64> {
    let dimension = match unit {
        "kg" => Some(Dimension::Mass),
        "m" => Some(Dimension::Length),
        "m^2" => Some(Dimension::Area),
        "m/s" => Some(Dimension::Speed),
        "s" => Some(Dimension::Time),
        "W" => Some(Dimension::Power),
        "N" => Some(Dimension::Force),
        "deg" => Some(Dimension::Angle),
        _ => None,
    };
    if let Some(dimension) = dimension {
        return parse_quantity(raw, dimension);
    }
    if unit == "bool" {
        return match raw {
            "true" => Ok(1.0),
            "false" => Ok(0.0),
            _ => Err(AexError::validation(
                "INVALID_STUDY_CONSTRAINT_VALUE",
                "study.constraints.value",
                "boolean constraint values must be true or false",
            )),
        };
    }
    let number = if unit == "1" {
        raw.trim()
    } else {
        raw.strip_suffix(unit).unwrap_or(raw).trim()
    };
    number.parse::<f64>().map_err(|source| {
        AexError::validation(
            "INVALID_STUDY_CONSTRAINT_VALUE",
            "study.constraints.value",
            source.to_string(),
        )
    })
}

fn constraint_passed(actual: f64, required: f64, operator: &str, margin: f64) -> bool {
    match operator {
        "gt" => actual > required,
        "lt" => actual < required,
        _ => margin >= -1.0e-9,
    }
}

fn constraint_margin(actual: f64, required: f64, operator: &str) -> AexResult<f64> {
    match operator {
        "ge" => Ok(actual - required),
        "gt" => Ok(actual - required),
        "le" => Ok(required - actual),
        "lt" => Ok(required - actual),
        "eq" => Ok(-(actual - required).abs()),
        _ => Err(AexError::validation(
            "INVALID_STUDY_CONSTRAINT_OPERATOR",
            "study.constraints.operator",
            format!("expected ge, gt, le, lt, or eq; got {operator}"),
        )),
    }
}

#[cfg(test)]
mod tests;

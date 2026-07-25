use std::collections::BTreeSet;

use serde_yaml::Value;

use crate::domain::capabilities::{
    REQUIREMENT_OPERATORS, REQUIREMENT_SEVERITIES, RequirementMetricCapability,
    RequirementValueKind, requirement_metric,
};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::parse_quantity;
use crate::domain::schema::{RawRequirement, Requirement, Requirements, RequirementsDocument};

pub(crate) fn resolve_requirements(document: RequirementsDocument) -> AexResult<Requirements> {
    let raw = document.requirements;
    ensure_unique_requirement_ids(&raw.items)?;
    let items = raw
        .items
        .into_iter()
        .map(resolve_requirement)
        .collect::<AexResult<Vec<_>>>()?;
    Ok(Requirements { id: raw.id, items })
}

fn ensure_unique_requirement_ids(items: &[RawRequirement]) -> AexResult<()> {
    let mut ids = BTreeSet::new();
    for (index, requirement) in items.iter().enumerate() {
        if !ids.insert(requirement.id.as_str()) {
            return Err(AexError::validation(
                "DUPLICATE_REQUIREMENT_ID",
                format!("requirements.items.{index}.id"),
                format!(
                    "requirement id {} is declared more than once",
                    requirement.id
                ),
            ));
        }
    }
    Ok(())
}

fn resolve_requirement(raw: RawRequirement) -> AexResult<Requirement> {
    let capability = requirement_metric(&raw.metric).ok_or_else(|| {
        AexError::validation(
            "UNSUPPORTED_REQUIREMENT_METRIC",
            &raw.metric,
            "metric is not implemented",
        )
    })?;
    if !capability.bindable {
        let replacement = capability.replacement.unwrap_or("an achieved metric");
        return Err(AexError::validation(
            "DECLARED_METRIC_NOT_BINDABLE",
            format!("requirements.items.{}.metric", raw.id),
            format!(
                "{} is a declared input; bind the requirement to {replacement}",
                raw.metric
            ),
        ));
    }
    let (required, unit) = requirement_value(capability, &raw.value)?;
    if !REQUIREMENT_OPERATORS.contains(&raw.operator.as_str()) {
        return Err(AexError::validation(
            "INVALID_REQUIREMENT_OPERATOR",
            format!("requirements.items.{}.operator", raw.id),
            format!("unsupported operator {}", raw.operator),
        ));
    }
    if !REQUIREMENT_SEVERITIES.contains(&raw.severity.as_str()) {
        return Err(AexError::validation(
            "INVALID_REQUIREMENT_SEVERITY",
            format!("requirements.items.{}.severity", raw.id),
            format!("unsupported severity {}", raw.severity),
        ));
    }
    Ok(Requirement {
        id: raw.id,
        metric: raw.metric,
        operator: raw.operator,
        required,
        unit,
        severity: raw.severity,
        weight: raw.weight,
    })
}

fn requirement_value(
    capability: RequirementMetricCapability,
    value: &Value,
) -> AexResult<(f64, String)> {
    match capability.value_kind.dimension() {
        Some(dimension) => {
            let raw = value.as_str().ok_or_else(|| {
                AexError::validation(
                    "AMBIGUOUS_UNITLESS_VALUE",
                    capability.id,
                    "physical requirements require a unit",
                )
            })?;
            Ok((
                parse_quantity(raw, dimension)?,
                capability.canonical_unit.to_owned(),
            ))
        }
        None if capability.value_kind == RequirementValueKind::Scalar => value
            .as_f64()
            .map(|number| (number, capability.canonical_unit.to_owned()))
            .ok_or_else(|| {
                AexError::validation(
                    "INVALID_REQUIREMENT_VALUE",
                    capability.id,
                    "expected number",
                )
            }),
        None => Err(AexError::validation(
            "INVALID_REQUIREMENT_VALUE_KIND",
            capability.id,
            "physical requirement metric has no quantity dimension",
        )),
    }
}

#[cfg(test)]
mod tests;

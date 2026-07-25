use std::collections::BTreeSet;

use serde_yaml::Value;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{Dimension, parse_quantity};
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
    if let Some(replacement) = declared_metric_replacement(&raw.metric) {
        return Err(AexError::validation(
            "DECLARED_METRIC_NOT_BINDABLE",
            format!("requirements.items.{}.metric", raw.id),
            format!(
                "{} is a declared input; bind the requirement to {replacement}",
                raw.metric
            ),
        ));
    }
    let (required, unit) = requirement_value(&raw.metric, &raw.value)?;
    if !matches!(raw.operator.as_str(), "ge" | "le" | "eq") {
        return Err(AexError::validation(
            "INVALID_REQUIREMENT_OPERATOR",
            format!("requirements.items.{}.operator", raw.id),
            format!("unsupported operator {}", raw.operator),
        ));
    }
    if !matches!(raw.severity.as_str(), "hard" | "soft" | "report_only") {
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

fn requirement_value(metric: &str, value: &Value) -> AexResult<(f64, String)> {
    let dimension = match metric {
        "mission.payload_mass" => Some((Dimension::Mass, "kg")),
        "performance.achieved_cruise_true_airspeed" | "performance.stall_speed_landing" => {
            Some((Dimension::Speed, "m/s"))
        }
        "performance.minimum_cruise_excess_power" => Some((Dimension::Power, "W")),
        "mission.completed_distance"
        | "performance.full_payload_range"
        | "performance.zero_payload_ferry_range" => Some((Dimension::Length, "m")),
        "performance.service_ceiling" | "performance.takeoff_field_length" => {
            Some((Dimension::Length, "m"))
        }
        "performance.achieved_cruise_mach" | "performance.cruise_feasible" => None,
        _ => {
            return Err(AexError::validation(
                "UNSUPPORTED_REQUIREMENT_METRIC",
                metric,
                "metric is not implemented",
            ));
        }
    };
    match dimension {
        Some((kind, unit)) => {
            let raw = value.as_str().ok_or_else(|| {
                AexError::validation(
                    "AMBIGUOUS_UNITLESS_VALUE",
                    metric,
                    "physical requirements require a unit",
                )
            })?;
            Ok((parse_quantity(raw, kind)?, unit.to_owned()))
        }
        None => value
            .as_f64()
            .map(|number| (number, "1".to_owned()))
            .ok_or_else(|| {
                AexError::validation("INVALID_REQUIREMENT_VALUE", metric, "expected number")
            }),
    }
}

fn declared_metric_replacement(metric: &str) -> Option<&'static str> {
    match metric {
        "performance.cruise_mach" => Some("performance.achieved_cruise_mach"),
        "performance.cruise_true_airspeed" => Some("performance.achieved_cruise_true_airspeed"),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

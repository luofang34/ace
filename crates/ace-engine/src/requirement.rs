use crate::{EngineResult, Snapshot, ValueStatus};
use ace_model::{Quantity, Relation, Requirement, VehicleModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Requirement disposition, including explicitly missing or inapplicable evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarginStatus {
    /// Feasible with a positive margin.
    Pass,
    /// Active or provisional result requiring attention.
    Warn,
    /// Current calculation violates the requirement.
    Fail,
    /// Required evidence is unavailable.
    Unknown,
    /// Snapshot does not match the declared operating context.
    NotApplicable,
}

/// Computed signed margin linked to the exact output evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Margin {
    /// Requirement identifier.
    pub requirement: String,
    /// Canonical output identifier.
    pub parameter: String,
    /// Disposition.
    pub status: MarginStatus,
    /// Whether the metric comes from an approximate computation.
    #[serde(default)]
    pub provisional: bool,
    /// Achieved quantity, when available.
    pub actual: Option<Quantity>,
    /// Signed absolute slack; positive is feasible.
    pub absolute: Option<Quantity>,
    /// Slack divided by absolute target; absent for a zero target.
    pub fraction: Option<f64>,
    /// Producing calculation for provenance navigation.
    pub run_id: Option<String>,
    /// Reason for an unknown or inapplicable result.
    pub message: Option<String>,
}

/// Evaluate only current values and explicitly match all requirement conditions.
pub fn evaluate_requirements(
    model: &VehicleModel,
    snapshot: &Snapshot,
) -> EngineResult<BTreeMap<String, Margin>> {
    if crate::record::digest(model)? != snapshot.model_digest {
        return Err(ace_model::ModelError::invalid(
            "snapshot",
            "snapshot does not belong to this model revision",
        )
        .into());
    }
    model
        .requirements
        .iter()
        .map(|(id, requirement)| Ok((id.clone(), evaluate(id, requirement, snapshot)?)))
        .collect()
}

fn evaluate(id: &str, requirement: &Requirement, snapshot: &Snapshot) -> EngineResult<Margin> {
    let mut margin = Margin {
        requirement: id.into(),
        parameter: requirement.metric.clone(),
        status: MarginStatus::Unknown,
        provisional: false,
        actual: None,
        absolute: None,
        fraction: None,
        run_id: None,
        message: None,
    };
    for (name, expected) in &requirement.condition {
        let value = snapshot
            .values
            .get(name)
            .filter(|value| value.usable() && value.status != ValueStatus::Approximate);
        let Some(quantity) = value.and_then(|record| record.quantity) else {
            margin.message = Some(format!("condition {name} is unavailable or provisional"));
            return Ok(margin);
        };
        if (quantity.to(expected.unit)?.value - expected.value).abs()
            > 1e-9 * expected.value.abs().max(1.0)
        {
            margin.status = MarginStatus::NotApplicable;
            margin.message = Some(format!("condition {name} does not match"));
            return Ok(margin);
        }
    }
    let Some(record) = snapshot
        .values
        .get(&requirement.metric)
        .filter(|record| record.usable())
    else {
        margin.message = Some("current metric is unavailable".into());
        return Ok(margin);
    };
    let Some(actual) = record.quantity else {
        return Ok(margin);
    };
    let actual = actual.to(requirement.target.unit)?;
    let tolerance = requirement
        .tolerance
        .map(|value| value.to(actual.unit))
        .transpose()?
        .map_or(0.0, |q| q.value);
    let slack = match requirement.relation {
        Relation::Ge => actual.value - requirement.target.value + tolerance,
        Relation::Le => requirement.target.value - actual.value + tolerance,
        Relation::Eq => tolerance - (actual.value - requirement.target.value).abs(),
    };
    margin.actual = Some(actual);
    margin.absolute = Some(Quantity::new(slack, actual.unit)?);
    margin.fraction = if requirement.target.value == 0.0 {
        None
    } else {
        Some(slack / requirement.target.value.abs())
    };
    margin.run_id = record.run_id.clone();
    margin.provisional = record.status == ValueStatus::Approximate;
    margin.status = if slack < 0.0 {
        MarginStatus::Fail
    } else if margin.provisional
        || slack == 0.0
        || margin.fraction.is_some_and(|value| value <= 0.05)
    {
        MarginStatus::Warn
    } else {
        MarginStatus::Pass
    };
    Ok(margin)
}

use crate::{EngineResult, MarginStatus, Snapshot, ValueStatus, evaluate_requirements};
use ace_model::{Priority, VehicleModel};
use serde::{Deserialize, Serialize};

/// Whether current calculations support the mandatory design constraints.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Feasibility {
    /// Current computations satisfy mandatory constraints; this is not calibration evidence.
    Satisfied,
    /// Objectives or mandatory constraints depend on approximate evidence.
    Provisional,
    /// At least one mandatory constraint is violated, including approximate violations.
    Violated,
    /// Required context, calculation, objective or closure evidence is unavailable.
    Unknown,
}

/// Execution outcome independent of constraint satisfaction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Every declared value is available and every closure converged.
    Complete,
    /// At least one declared value or closure is unavailable.
    Incomplete,
}

/// Quality of calculation evidence; computed values do not imply experimental validation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceQuality {
    /// At least one required result is unavailable.
    Incomplete,
    /// At least one result uses approximate physics or inputs.
    Approximate,
    /// Executed model calculations without a calibration or validation claim.
    ComputedUnvalidated,
}

/// Shared synthesis, study and sheet assessment for one exact model revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment {
    /// Whether all computations produced usable results.
    pub execution: ExecutionStatus,
    /// Mandatory feasibility, independent of soft and report-only requirements.
    pub feasibility: Feasibility,
    /// Quality of the available numerical evidence.
    pub evidence: EvidenceQuality,
    /// Violated hard requirements.
    pub violations: Vec<String>,
    /// Missing or inapplicable hard requirement contexts.
    pub unknown: Vec<String>,
}

/// Assess current evidence without treating completion or contextual absence as feasibility.
pub fn assess(model: &VehicleModel, snapshot: &Snapshot) -> EngineResult<Assessment> {
    let margins = evaluate_requirements(model, snapshot)?;
    let mut violations = Vec::new();
    let mut unknown = Vec::new();
    let mut provisional = false;
    for (id, requirement) in &model.requirements {
        if requirement.priority != Priority::Hard {
            continue;
        }
        match margins.get(id) {
            Some(margin) if margin.status == MarginStatus::Fail => violations.push(id.clone()),
            Some(margin) if matches!(margin.status, MarginStatus::Pass | MarginStatus::Warn) => {
                provisional |= margin.provisional;
            }
            _ => unknown.push(id.clone()),
        }
    }
    let complete = snapshot.complete();
    let mut objectives_available = true;
    for objective in model.objectives.values() {
        match snapshot
            .values
            .get(&objective.metric)
            .filter(|value| value.usable())
        {
            Some(value) => provisional |= value.status == ValueStatus::Approximate,
            None => objectives_available = false,
        }
    }
    let feasibility = if !violations.is_empty() {
        Feasibility::Violated
    } else if !complete || !objectives_available || !unknown.is_empty() {
        Feasibility::Unknown
    } else if provisional {
        Feasibility::Provisional
    } else {
        Feasibility::Satisfied
    };
    let evidence = if !complete {
        EvidenceQuality::Incomplete
    } else if snapshot
        .values
        .values()
        .any(|value| value.status == ValueStatus::Approximate)
    {
        EvidenceQuality::Approximate
    } else {
        EvidenceQuality::ComputedUnvalidated
    };
    Ok(Assessment {
        execution: if complete {
            ExecutionStatus::Complete
        } else {
            ExecutionStatus::Incomplete
        },
        feasibility,
        evidence,
        violations,
        unknown,
    })
}

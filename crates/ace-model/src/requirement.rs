use crate::{Quantity, Source};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Constraint comparison convention; positive signed margin is feasible.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// Actual must be greater than or equal to target.
    Ge,
    /// Actual must be less than or equal to target.
    Le,
    /// Absolute deviation must be within an explicit tolerance.
    Eq,
}

/// Requirement enforcement policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Mandatory feasibility condition.
    Hard,
    /// Preference reported with its margin.
    Soft,
    /// Informational metric.
    ReportOnly,
}

/// Machine-computable requirement with traceable origin and applicable context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    /// Human-readable requirement.
    pub name: String,
    /// Canonical output being constrained.
    pub metric: String,
    /// Comparison direction.
    pub relation: Relation,
    /// Target in physical units.
    pub target: Quantity,
    /// Equality tolerance or allowed numerical deviation.
    pub tolerance: Option<Quantity>,
    /// Enforcement policy.
    pub priority: Priority,
    /// Source requirement document/identifier.
    pub source: Source,
    /// Required parameter values for this requirement's operating context.
    #[serde(default)]
    pub condition: BTreeMap<String, Quantity>,
    /// Semantic objects associated with the requirement.
    #[serde(default)]
    pub geometry: Vec<String>,
}

/// Direction of a separate optimization objective.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Minimize.
    Min,
    /// Maximize.
    Max,
}

/// Independent objective retained for Pareto studies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    /// Canonical scalar to optimize.
    pub metric: String,
    /// Preferred direction.
    pub direction: Direction,
}

/// Bounded authoritative input usable by sweeps and optimizers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignVariable {
    /// Parameter identifier.
    pub parameter: String,
    /// Lower bound.
    pub lower: Quantity,
    /// Upper bound.
    pub upper: Quantity,
}

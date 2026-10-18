use crate::{EngineResult, PluginIdentity};
use ace_model::{AnalysisResources, Quantity, Source};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Freshness and validity of a canonical value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueStatus {
    /// Authoritative input.
    Input,
    /// Current result from the selected model.
    Computed,
    /// Provisional result or a result consuming provisional inputs.
    Approximate,
    /// Input edit invalidated this output.
    Stale,
    /// Input or solver has not been supplied.
    Unavailable,
    /// Selected solver or its port contract failed.
    Failed,
    /// A required upstream value is unavailable.
    Blocked,
    /// Bounded coupled iteration did not converge.
    Nonconverged,
}

/// A current or historically consumed canonical parameter value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueRecord {
    /// Canonical identifier.
    pub parameter: String,
    /// Usability and freshness.
    pub status: ValueStatus,
    /// Missing for every unusable value.
    pub quantity: Option<Quantity>,
    /// Producing calculation content hash.
    pub run_id: Option<String>,
    /// Parameter declaration revision at consumption.
    pub revision: u64,
    /// Source retained even when the current parameter is subsequently edited.
    pub source: Source,
    /// Diagnostic with parameter/solver context.
    pub message: Option<String>,
}

impl ValueRecord {
    /// Whether the value is available for a downstream computation.
    pub fn usable(&self) -> bool {
        self.quantity.is_some()
            && matches!(
                self.status,
                ValueStatus::Input | ValueStatus::Computed | ValueStatus::Approximate
            )
    }
}

/// Reproducible calculation evidence with exact consumed values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    /// Content hash of all evidence except this field.
    pub id: String,
    /// Graph node or closure identifier.
    pub node: String,
    /// Hash of the complete authoritative model.
    pub model_digest: String,
    /// Executable identity; expressions and closure relaxation have no plugin.
    pub plugin: Option<PluginIdentity>,
    /// Exact scalar contract used by this invocation, including executable limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<crate::PluginSpec>,
    /// Serialized expression or closure settings.
    pub calculation: serde_json::Value,
    /// Consumed canonical value records.
    pub inputs: BTreeMap<String, ValueRecord>,
    /// Successfully produced quantities.
    pub outputs: BTreeMap<String, Quantity>,
    /// Calculation disposition.
    pub status: ValueStatus,
    /// Exact semantic resources consumed by this calculation.
    pub resources: AnalysisResources,
    /// Structured domain evidence from the selected solver.
    pub details: serde_json::Value,
    /// Solver artifacts.
    pub artifacts: Vec<ace_model::Artifact>,
    /// Contextual failure or validity diagnostic.
    pub message: Option<String>,
    /// Closure iteration, when applicable.
    pub iteration: Option<u32>,
}

/// Outcome of a declared coupled component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosureResult {
    /// Closure identifier.
    pub id: String,
    /// Whether every unrelaxed residual satisfied its tolerance.
    pub converged: bool,
    /// Number of complete iterations attempted.
    pub iterations: u32,
    /// Last residual per output.
    pub residuals: BTreeMap<String, Quantity>,
    /// Failure explanation.
    pub message: Option<String>,
}

/// Current design projection and immutable evidence history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Exact implementation and dependency identity that produced this execution session.
    pub build: crate::BuildIdentity,
    /// Authoritative vehicle identifier.
    pub vehicle_id: String,
    /// Revision of the design being projected.
    pub revision: u64,
    /// Hash of all model declarations.
    pub model_digest: String,
    /// Current canonical values.
    pub values: BTreeMap<String, ValueRecord>,
    /// Evidence for current and historically consumed values.
    pub runs: Vec<RunRecord>,
    /// Current closure outcomes.
    pub closures: BTreeMap<String, ClosureResult>,
    /// Nodes actually executed by this update.
    pub executed: Vec<String>,
}

impl Snapshot {
    /// Whether all declared canonical outputs and closure components are available.
    pub fn complete(&self) -> bool {
        self.values.values().all(ValueRecord::usable)
            && self.closures.values().all(|item| item.converged)
    }
}

pub(crate) fn digest(value: &impl Serialize) -> EngineResult<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

pub(crate) fn finalize(mut run: RunRecord) -> EngineResult<RunRecord> {
    run.id = digest(&run)?;
    Ok(run)
}

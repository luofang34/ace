use crate::SolverError;
use ace_model::{AnalysisResources, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod phase;
mod registry;
pub use phase::{PhaseIncrement, PhaseOutput, PhaseRun};
pub use registry::Registry;

/// Fidelity of an executable analysis model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    /// Preliminary conceptual correlation.
    Conceptual,
    /// Handbook engineering analysis.
    Handbook,
    /// Provisional surrogate result.
    Surrogate,
    /// High-fidelity solver adapter.
    HighFidelity,
}

/// Executable identity retained in every result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginIdentity {
    /// Registered solver identifier.
    pub id: String,
    /// Exact solver version.
    pub version: String,
    /// Model fidelity.
    pub fidelity: Fidelity,
    /// Code commit, package digest, or immutable build identifier.
    pub code_revision: String,
}

impl PluginIdentity {
    /// Artifact generator identity for this exact solver implementation.
    pub fn generator(&self) -> ace_model::GeneratorIdentity {
        ace_model::GeneratorIdentity {
            id: self.id.clone(),
            version: self.version.clone(),
            implementation: self.code_revision.clone(),
        }
    }
}

/// Input/output declaration used to reject contract mismatches before execution.
#[derive(Debug, Clone)]
pub struct PluginSpec {
    /// Versioned solver identity.
    pub identity: PluginIdentity,
    /// Named inputs and physical units.
    pub inputs: BTreeMap<String, Unit>,
    /// Named outputs and physical units.
    pub outputs: BTreeMap<String, Unit>,
}

/// Only the semantic objects and parameter values declared by an analysis node.
pub struct AnalysisContext<'a> {
    /// Selected semantic geometry, mass, mission and operating definitions.
    pub resources: AnalysisResources,
    /// Parameter values consumed by resources and input ports.
    pub values: &'a BTreeMap<String, Quantity>,
    /// Application-selected adapters, including isolated preview replacements.
    pub registry: &'a Registry,
}

/// Atomic output of one installed solver.
#[derive(Debug, Clone)]
pub struct PluginOutput {
    /// Every declared output port must appear exactly once.
    pub values: BTreeMap<String, Quantity>,
    /// Explicit provisional/approximate quality.
    pub approximate: bool,
    /// Content-addressed solver, mesh, or load-case artifacts.
    pub artifacts: Vec<ace_model::Artifact>,
    /// Structured geometry, loading, state, or cost evidence.
    pub details: serde_json::Value,
    /// Invalid physical result; evidence is retained but scalar outputs are revoked.
    pub failure: Option<String>,
}

/// Adapter boundary independent of vehicle domain, UI, and persistence.
pub trait AnalysisPlugin: Send + Sync {
    /// Immutable port and identity contract.
    fn spec(&self) -> &PluginSpec;
    /// Evaluate only declared inputs and resources.
    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        geometry: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError>;

    /// Consume a mission entry state and return ordered physical increments.
    /// Scalar-only solvers reject this contract instead of ignoring the state.
    fn evaluate_phase(
        &self,
        _inputs: &BTreeMap<String, Quantity>,
        _state: &ace_model::VehicleState,
        _context: &AnalysisContext<'_>,
    ) -> Result<PhaseOutput, SolverError> {
        Err(SolverError::Failed(format!(
            "{} does not implement state-driven phases",
            self.spec().identity.id
        )))
    }
}

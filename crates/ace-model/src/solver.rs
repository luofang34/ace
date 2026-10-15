use crate::Quantity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Whether a solver runs as a scalar graph node or consumes a mission entry state.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMode {
    /// Ordinary named scalar inputs and outputs.
    #[default]
    Scalar,
    /// Invoked by a mission sequencer with the preceding vehicle state.
    Phase,
}

/// Solver declaration mapping typed ports to the canonical graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Analysis {
    /// Execution contract; phases publish trajectory evidence inside their mission run.
    #[serde(default)]
    pub mode: AnalysisMode,
    /// Explicitly registered solver identifier.
    pub plugin: String,
    /// Exact solver version.
    pub version: String,
    /// Input port to parameter mapping.
    pub inputs: BTreeMap<String, String>,
    /// Output port to parameter mapping.
    pub outputs: BTreeMap<String, String>,
    /// Semantic geometry consumed by the solver.
    #[serde(default)]
    pub geometry: Vec<String>,
    /// Requirements motivating the calculation.
    #[serde(default)]
    pub requirements: Vec<String>,
    /// Additional semantic objects consumed by this calculation.
    #[serde(default)]
    pub resources: crate::ResourceSelection,
}

/// Dimensional convergence threshold for one coupled output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tolerance {
    /// Positive absolute residual tolerance.
    pub absolute: Quantity,
    /// Nonnegative relative residual tolerance.
    pub relative: f64,
}

/// Declared strongly connected component solved by bounded fixed-point iteration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Closure {
    /// All outputs in exactly one dependency cycle, with convergence tolerances.
    pub tolerances: BTreeMap<String, Tolerance>,
    /// Maximum complete iterations.
    pub max_iterations: u32,
    /// Relaxation in (0, 1]; convergence uses the unrelaxed residual.
    pub relaxation: f64,
}

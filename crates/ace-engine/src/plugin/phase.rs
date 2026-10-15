use super::{AnalysisContext, Fidelity, PluginIdentity, Registry};
use crate::SolverError;
use ace_model::{Analysis, AnalysisMode, Quantity, VehicleState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One integration interval; mass changes are applied by the mission sequencer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseIncrement {
    /// Nonnegative elapsed time during this interval.
    pub duration: Quantity,
    /// Nonnegative distance traveled during this interval.
    pub distance: Quantity,
    /// Nonnegative fuel consumption during this interval.
    pub fuel_burn: Quantity,
    /// Nonnegative operating expense during this interval.
    pub cost: Quantity,
    /// Conditions at the interval endpoint.
    pub conditions: BTreeMap<String, Quantity>,
    /// Aerodynamic, propulsion or other solver evidence evaluated for the interval.
    pub details: serde_json::Value,
}

/// State-dependent phase solution with its native resolution and fidelity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseOutput {
    /// Ordered, nonempty integration intervals.
    pub increments: Vec<PhaseIncrement>,
    /// Whether this solver or its assumptions are approximate.
    pub approximate: bool,
    /// Content-addressed solver artifacts.
    pub artifacts: Vec<ace_model::Artifact>,
    /// Method, validity domain and diagnostics.
    pub details: serde_json::Value,
}

/// Replayable phase invocation nested in its authoritative mission calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseRun {
    /// Actual adapter identity, including explicit preview substitutions.
    pub plugin: PluginIdentity,
    /// Exact named scalar inputs after unit conversion.
    pub inputs: BTreeMap<String, Quantity>,
    /// Entry state with the phase controls already applied.
    pub initial_state: VehicleState,
    /// Integration result.
    pub output: PhaseOutput,
}

impl Registry {
    /// Execute a registered phase using only its declared scalar inputs and resources.
    pub fn evaluate_phase(
        &self,
        analysis: &Analysis,
        state: &VehicleState,
        context: &AnalysisContext<'_>,
    ) -> Result<PhaseRun, SolverError> {
        if analysis.mode != AnalysisMode::Phase {
            return Err(SolverError::Failed(
                "analysis is not a state-driven phase".into(),
            ));
        }
        let plugin = self
            .get(&analysis.plugin, &analysis.version)
            .ok_or_else(|| {
                SolverError::Failed(format!(
                    "phase solver {}@{} is unavailable",
                    analysis.plugin, analysis.version
                ))
            })?;
        let spec = plugin.spec();
        if !spec.inputs.keys().eq(analysis.inputs.keys()) || !spec.outputs.is_empty() {
            return Err(SolverError::Failed(format!(
                "phase port contract mismatch: {}",
                analysis.plugin
            )));
        }
        let mut inputs = BTreeMap::new();
        for ((port, parameter), unit) in analysis.inputs.iter().zip(spec.inputs.values()) {
            let value = context.values.get(parameter).ok_or_else(|| {
                SolverError::Failed(format!(
                    "phase {} missing input {parameter}",
                    analysis.plugin
                ))
            })?;
            inputs.insert(port.clone(), value.to(*unit)?);
        }
        let resources = context.resources.for_phase(analysis)?;
        let mut dependencies = resources.dependencies();
        dependencies.extend(analysis.inputs.values().cloned());
        let values = context
            .values
            .iter()
            .filter(|(id, _)| dependencies.contains(*id))
            .map(|(id, value)| (id.clone(), *value))
            .collect();
        let scoped = AnalysisContext {
            resources,
            values: &values,
            registry: self,
        };
        self.control().checkpoint()?;
        let mut output = plugin.evaluate_phase(&inputs, state, &scoped)?;
        self.control().checkpoint()?;
        for artifact in &output.artifacts {
            artifact.validate()?;
        }
        if output.increments.is_empty() || output.increments.len() > 100_000 {
            return Err(SolverError::Failed(format!(
                "phase {} requires 1..=100000 integration intervals",
                analysis.plugin
            )));
        }
        output.approximate |= spec.identity.fidelity == Fidelity::Surrogate;
        Ok(PhaseRun {
            plugin: spec.identity.clone(),
            inputs,
            initial_state: state.clone(),
            output,
        })
    }
}

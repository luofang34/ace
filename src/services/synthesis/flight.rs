use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PhaseOutput, PluginIdentity, PluginOutput,
    PluginSpec, SolverError,
};
use ace_model::{Quantity, Unit, VehicleState};
use std::collections::BTreeMap;

mod integration;
mod parameters;

pub(super) struct FlightPhase {
    spec: PluginSpec,
}

impl FlightPhase {
    pub fn new() -> Self {
        use Unit::*;
        let ports = [
            ("area", SquareMeter),
            ("span", Meter),
            ("cd0", Ratio),
            ("oswald", Ratio),
            ("cl_max", Ratio),
            ("tsfc", KilogramPerNewtonSecond),
            ("available_thrust", Newton),
            ("fuel_price", DollarPerKilogram),
            ("maximum_step", Second),
        ];
        Self {
            spec: PluginSpec {
                identity: PluginIdentity {
                    id: "aircraft.level_flight_phase".into(),
                    version: "1".into(),
                    fidelity: Fidelity::Conceptual,
                    code_revision: revision(),
                },
                inputs: ports
                    .into_iter()
                    .map(|(name, unit)| (name.into(), unit))
                    .collect(),
                outputs: BTreeMap::new(),
            },
        }
    }
}

impl AnalysisPlugin for FlightPhase {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        _: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        Err(SolverError::Failed(
            "level flight requires a mission entry state".into(),
        ))
    }
    fn evaluate_phase(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        state: &VehicleState,
        context: &AnalysisContext<'_>,
    ) -> Result<PhaseOutput, SolverError> {
        let model = parameters::FlightModel::new(inputs)?;
        integration::evaluate(&model, state, context.registry.control())
    }
}

fn revision() -> String {
    ace_engine::implementation_identity().digest
}

fn external(source: crate::domain::diagnostic::AexError) -> SolverError {
    SolverError::External {
        solver: "aircraft.level_flight_phase".into(),
        source: Box::new(source),
    }
}

#[cfg(test)]
mod tests;

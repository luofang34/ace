use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PhaseOutput, PluginIdentity, PluginOutput,
    PluginSpec, SolverError,
};
use ace_model::{Quantity, Unit, VehicleState};
use std::collections::BTreeMap;

mod ground;
mod integration;
mod parameters;
mod path;

#[derive(Clone, Copy)]
pub(super) enum PhaseKind {
    Level,
    RouteCruise,
    EnergyPath,
    GroundEstimate,
}

pub(super) struct FlightPhase {
    spec: PluginSpec,
    kind: PhaseKind,
}

impl FlightPhase {
    pub fn new() -> Self {
        Self::with_kind(PhaseKind::Level)
    }

    pub fn with_kind(kind: PhaseKind) -> Self {
        use Unit::*;
        let mut ports = vec![
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
        let id = match kind {
            PhaseKind::Level => "aircraft.level_flight_phase",
            PhaseKind::RouteCruise => "aircraft.route_cruise_phase",
            PhaseKind::EnergyPath => "aircraft.energy_path_phase",
            PhaseKind::GroundEstimate => {
                ports = vec![
                    ("fuel_flow", KilogramPerSecond),
                    ("fuel_price", DollarPerKilogram),
                ];
                "aircraft.ground_estimate_phase"
            }
        };
        Self {
            kind,
            spec: PluginSpec {
                identity: PluginIdentity {
                    id: id.into(),
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
            "flight phase requires a mission entry state".into(),
        ))
    }
    fn evaluate_phase(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        state: &VehicleState,
        context: &AnalysisContext<'_>,
    ) -> Result<PhaseOutput, SolverError> {
        if matches!(self.kind, PhaseKind::GroundEstimate) {
            return ground::evaluate(inputs, state);
        }
        let model = parameters::FlightModel::new(inputs)?;
        match self.kind {
            PhaseKind::EnergyPath => path::evaluate(&model, state, context.registry.control()),
            PhaseKind::RouteCruise => {
                let mut entry = state.clone();
                let distance =
                    parameters::positive(&state.conditions, "route_distance", Unit::Meter)?
                        - state.distance.to(Unit::Meter)?.value
                        - parameters::positive(&state.conditions, "arrival_distance", Unit::Meter)?;
                let speed = parameters::positive(&state.conditions, "speed", Unit::MeterPerSecond)?;
                entry.conditions.insert(
                    "duration".into(),
                    Quantity::new(distance / speed, Unit::Second)?,
                );
                integration::evaluate(&model, &entry, context.registry.control())
            }
            _ => integration::evaluate(&model, state, context.registry.control()),
        }
    }
}

fn revision() -> String {
    ace_engine::implementation_identity().digest
}

fn external(source: crate::domain::diagnostic::AexError) -> SolverError {
    SolverError::External {
        solver: "aircraft.flight_phase".into(),
        source: Box::new(source),
    }
}

#[cfg(test)]
mod tests;

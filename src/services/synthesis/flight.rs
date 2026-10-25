use ace_engine::{
    AnalysisContext, AnalysisPlugin, Fidelity, PhaseOutput, PluginIdentity, PluginOutput,
    PluginSpec, SolverError,
};
use ace_model::{Quantity, Unit, VehicleState};
use std::collections::BTreeMap;

pub(super) mod contract;
mod ground;
mod integration;
mod parameters;
mod path;
mod polar;
mod powerplant;

#[derive(Clone, Copy)]
pub(super) enum PhaseKind {
    Level,
    RouteCruise,
    EnergyPath,
    GroundEstimate,
}

/// Installed-propulsion port family of one flight-phase variant.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Propulsion {
    /// Constant `available_thrust` and `tsfc` ports.
    Thrust,
    /// Shaft power, propeller efficiency, BSFC and lapse ports.
    Propeller,
    /// Engine count and sizing ports over the selected catalog turbofan deck.
    Deck,
}

pub(super) struct FlightPhase {
    spec: PluginSpec,
    kind: PhaseKind,
    propulsion: Propulsion,
    vspaero: bool,
}

impl FlightPhase {
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
            propulsion: Propulsion::Thrust,
            vspaero: false,
            spec: PluginSpec {
                mode: ace_model::AnalysisMode::Phase,
                domain: contract::domain(kind, Propulsion::Thrust, false),
                identity: PluginIdentity {
                    id: id.into(),
                    version: "1".into(),
                    fidelity: Fidelity::Conceptual,
                    code_revision: revision(),
                },
                inputs: ports
                    .into_iter()
                    .map(|(name, unit)| (name.into(), unit.into()))
                    .collect(),
                outputs: BTreeMap::new(),
            },
        }
    }

    pub fn with_vspaero(kind: PhaseKind) -> Self {
        Self::with_kind(kind).vspaero()
    }

    /// Replaces the thrust/TSFC ports with shaft power, propeller efficiency and BSFC.
    pub fn propeller(self) -> Self {
        self.with_propulsion(
            Propulsion::Propeller,
            "propeller_",
            &[
                ("shaft_power", Unit::Watt),
                ("propeller_efficiency", Unit::Ratio),
                ("bsfc", Unit::KilogramPerKilowattHour),
                ("power_lapse_exponent", Unit::Ratio),
            ],
        )
    }

    /// Replaces the thrust/TSFC ports with a catalog turbofan deck, so thrust and
    /// fuel flow follow mode, altitude and Mach.
    pub fn deck(self) -> Self {
        self.with_propulsion(
            Propulsion::Deck,
            "deck_",
            &[
                ("engine_count", Unit::Ratio),
                ("sizing_factor", Unit::Ratio),
            ],
        )
    }

    fn with_propulsion(
        mut self,
        propulsion: Propulsion,
        prefix: &str,
        ports: &[(&str, Unit)],
    ) -> Self {
        self.propulsion = propulsion;
        self.spec.identity.id = self
            .spec
            .identity
            .id
            .replace("aircraft.", &format!("aircraft.{prefix}"));
        for port in ["tsfc", "available_thrust"] {
            self.spec.inputs.remove(port);
        }
        for (port, unit) in ports {
            self.spec.inputs.insert((*port).into(), (*unit).into());
        }
        self.spec.domain = contract::domain(self.kind, propulsion, self.vspaero);
        self
    }

    pub fn vspaero(self) -> Self {
        let mut phase = self;
        phase.vspaero = true;
        phase.spec.identity.id = phase
            .spec
            .identity
            .id
            .replace("aircraft.", "aircraft.vspaero_");
        phase.spec.inputs.remove("oswald");
        for port in crate::backends::openvsp::polar::PORTS {
            phase
                .spec
                .inputs
                .insert(port.into(), contract::polar_port(port));
        }
        phase.spec.domain = contract::domain(phase.kind, phase.propulsion, true);
        phase
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
        let powerplant = match self.propulsion {
            Propulsion::Deck => powerplant::Powerplant::deck(inputs, &context.resources)?,
            Propulsion::Thrust | Propulsion::Propeller => powerplant::Powerplant::new(inputs)?,
        };
        let model = if self.vspaero {
            polar::model(inputs, powerplant)?
        } else {
            parameters::FlightModel::with_powerplant(inputs, powerplant)?
        };
        match self.kind {
            PhaseKind::EnergyPath => path::evaluate(&model, state, context.registry.control()),
            PhaseKind::RouteCruise => {
                let mut entry = state.clone();
                let distance =
                    parameters::positive(&state.conditions, "route_distance", Unit::Meter)?
                        - state.distance.to(Unit::Meter)?.value
                        - parameters::positive(&state.conditions, "arrival_distance", Unit::Meter)?;
                let speed = parameters::positive(&state.conditions, "speed", Unit::MeterPerSecond)?;
                let ground_speed = parameters::ground_speed(&state.conditions, speed)?;
                entry.conditions.insert(
                    "duration".into(),
                    Quantity::new(distance / ground_speed, Unit::Second)?,
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

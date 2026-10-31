use super::{
    external,
    parameters::{positive, read},
};
use crate::{
    domain::{
        result::AtmosphereState,
        schema::{EngineProfile, SimpleTurbofanDeck, TurbofanProfile},
    },
    models::propulsion::{self, PropulsionQuery},
    services::synthesis::catalog,
};
use ace_engine::SolverError;
use ace_model::{AnalysisResources, Quantity, Unit};
use std::collections::BTreeMap;

const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// Installed propulsion seen by the canonical flight phases.
pub(super) enum Powerplant {
    /// Constant installed thrust limit and thrust-specific fuel consumption.
    Thrust {
        engine: Box<TurbofanProfile>,
        available_thrust: f64,
    },
    /// Shaft power converted to thrust by a constant propeller efficiency.
    Propeller {
        shaft_power: f64,
        efficiency: f64,
        bsfc_kg_j: f64,
        lapse_exponent: f64,
    },
    /// Catalog turbofan deck, scaled by engine count and sizing factor.
    Deck {
        engine: Box<TurbofanProfile>,
        engine_count: u32,
        sizing_factor: f64,
    },
}

/// Engine operating point for one required thrust.
pub(super) struct Operating {
    pub altitude: f64,
    pub speed: f64,
    pub mach: f64,
    pub mode: propulsion::OperatingMode,
}

impl Powerplant {
    /// The port set is fixed per plugin, so the presence of `shaft_power` selects the
    /// propeller variant without a second source of truth.
    pub fn new(inputs: &BTreeMap<String, Quantity>) -> Result<Self, SolverError> {
        if !inputs.contains_key("shaft_power") {
            let available_thrust = positive(inputs, "available_thrust", Unit::Newton)?;
            let tsfc = positive(inputs, "tsfc", Unit::KilogramPerNewtonSecond)?;
            return Ok(Self::Thrust {
                engine: Box::new(engine(tsfc, available_thrust)),
                available_thrust,
            });
        }
        let efficiency = positive(inputs, "propeller_efficiency", Unit::Ratio)?;
        let lapse_exponent = read(inputs, "power_lapse_exponent", Unit::Ratio)?;
        if efficiency > 1.0 || !(0.0..=3.0).contains(&lapse_exponent) {
            return Err(SolverError::Failed(
                "propeller efficiency must be in (0, 1] and power lapse exponent in 0..3".into(),
            ));
        }
        Ok(Self::Propeller {
            shaft_power: positive(inputs, "shaft_power", Unit::Watt)?,
            efficiency,
            bsfc_kg_j: positive(inputs, "bsfc", Unit::KilogramPerJoule)?,
            lapse_exponent,
        })
    }

    /// The selected turbofan profile with its installed count and sizing ports.
    pub fn deck(
        inputs: &BTreeMap<String, Quantity>,
        resources: &AnalysisResources,
    ) -> Result<Self, SolverError> {
        let EngineProfile::Turbofan(engine) = catalog::engine(resources).map_err(external)? else {
            return Err(SolverError::Failed(
                "deck-backed flight phases need a turbofan engine profile".into(),
            ));
        };
        let count = positive(inputs, "engine_count", Unit::Ratio)?;
        if count.fract() != 0.0 || count > 16.0 {
            return Err(SolverError::Failed(
                "engine count must be a whole number from 1 through 16".into(),
            ));
        }
        Ok(Self::Deck {
            engine: Box::new(engine),
            engine_count: count as u32,
            sizing_factor: positive(inputs, "sizing_factor", Unit::Ratio)?,
        })
    }

    /// Full-throttle thrust; a deck point outside its tabulated or declared range fails.
    pub fn available_thrust(
        &self,
        atmosphere: &AtmosphereState,
        point: &Operating,
    ) -> Result<f64, SolverError> {
        match self {
            Self::Thrust {
                available_thrust, ..
            } => Ok(*available_thrust),
            Self::Propeller {
                shaft_power,
                efficiency,
                lapse_exponent,
                ..
            } => Ok(efficiency
                * shaft_power
                * (atmosphere.density_kg_m3 / SEA_LEVEL_DENSITY_KG_M3).powf(*lapse_exponent)
                / point.speed),
            Self::Deck {
                engine,
                engine_count,
                sizing_factor,
            } => {
                let state = propulsion::installed_turbofan(
                    engine,
                    *engine_count,
                    *sizing_factor,
                    atmosphere,
                    PropulsionQuery {
                        altitude_m: point.altitude,
                        true_airspeed_m_s: point.speed,
                        mach: point.mach,
                        throttle: 1.0,
                        mode: point.mode,
                    },
                )
                .map_err(external)?;
                if state.model.validity_status != "valid" {
                    return Err(SolverError::Failed(format!(
                        "engine deck {} {} outside its range at {} m, Mach {}",
                        engine.id, state.model.validity_status, point.altitude, point.mach
                    )));
                }
                state
                    .thrust_available_n
                    .ok_or_else(|| SolverError::Failed("turbofan deck reported no thrust".into()))
            }
        }
    }

    pub fn fuel_flow(&self, thrust: f64, point: &Operating) -> Result<f64, SolverError> {
        match self {
            Self::Thrust { engine, .. } | Self::Deck { engine, .. } => {
                Ok(propulsion::fuel_flow_for_required_thrust(
                    engine,
                    point.altitude,
                    point.mach,
                    point.mode,
                    thrust,
                )
                .map_err(external)?
                .flow_kg_s)
            }
            Self::Propeller {
                efficiency,
                bsfc_kg_j,
                ..
            } => Ok(bsfc_kg_j * thrust * point.speed / efficiency),
        }
    }

    pub fn basis(&self) -> &'static str {
        match self {
            Self::Thrust { .. } => "constant installed thrust limit and TSFC",
            Self::Deck { .. } => {
                "catalog turbofan deck by mode, altitude and Mach, times engine count, sizing factor and (1 - installation loss); fuel from deck TSFC at required thrust; no extrapolation"
            }
            Self::Propeller { .. } => {
                "shaft power P*(rho/rho0)^n; thrust available eta*P/V; fuel BSFC*T*V/eta; constant propeller efficiency and BSFC; no propeller map, static thrust or power-setting BSFC variation"
            }
        }
    }
}

fn engine(tsfc: f64, thrust: f64) -> TurbofanProfile {
    TurbofanProfile {
        id: "canonical_phase_engine".into(),
        version: 1,
        model: "propulsion.simple_turbofan".into(),
        source: "canonical phase input ports".into(),
        confidence: "conceptual".into(),
        dry_mass_kg: 0.0,
        bypass_ratio: None,
        thrust_loss_fraction: 0.0,
        nacelle_drag_area_m2: 0.0,
        overall_length_m: None,
        maximum_diameter_m: None,
        table_deck: None,
        reference_thrust_n: thrust,
        simple_deck: Some(SimpleTurbofanDeck {
            sea_level_static_thrust_n: thrust,
            altitude_exponent: 0.0,
            mach_linear_coefficient: 0.0,
            minimum_thrust_fraction: 1.0,
            tsfc_takeoff_kg_n_hr: tsfc * 3600.0,
            tsfc_cruise_kg_n_hr: tsfc * 3600.0,
            cruise_reference_altitude_m: 0.0,
            cruise_reference_mach: 0.0,
            maximum_mach: 0.9,
            maximum_altitude_m: 20_000.0,
        }),
    }
}

#[cfg(test)]
mod tests;

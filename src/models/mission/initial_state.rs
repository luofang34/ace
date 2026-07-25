//! Effective mission integration state at the first segment boundary.

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::schema::{MissionInitialState, ResolvedScenario};
use crate::domain::warning::WarningCode;
use crate::models::atmosphere::Isa1976;
use crate::services::resolver::usable_initial_fuel_kg;

#[derive(Debug, Clone, Copy)]
pub(super) struct MissionState {
    pub(super) mass_kg: f64,
    pub(super) altitude_m: f64,
    pub(super) speed_m_s: Option<f64>,
    pub(super) fuel_remaining_kg: f64,
    pub(super) payload_remaining_kg: f64,
}

#[derive(Debug)]
pub(super) struct InitialMissionState {
    pub(super) state: MissionState,
    pub(super) capacity_exceeded: bool,
    pub(super) warning: Option<Diagnostic>,
}

#[derive(Debug)]
pub(super) struct InitialFuelLoad {
    pub(super) loaded_kg: f64,
    pub(super) capacity_exceeded: bool,
    pub(super) warning: Option<Diagnostic>,
}

pub(super) fn initial_mission_state(scenario: &ResolvedScenario) -> AexResult<InitialMissionState> {
    let aircraft = &scenario.aircraft;
    let usable_fuel = usable_initial_fuel_kg(aircraft, &scenario.mission);
    let requested_fuel = requested_fuel_kg(scenario.mission.initial_state.as_ref(), usable_fuel);
    if requested_fuel > usable_fuel + 1.0e-8 {
        return Err(AexError::validation(
            "INITIAL_FUEL_EXCEEDS_USABLE",
            "mission.initial_state.fuel_mass",
            format!("initial fuel {requested_fuel} kg exceeds usable load {usable_fuel} kg"),
        ));
    }
    let fuel = initial_fuel_load(requested_fuel, aircraft.mass.maximum_fuel_mass_kg);
    let altitude_m = scenario
        .mission
        .initial_state
        .as_ref()
        .and_then(|state| state.altitude_m)
        .unwrap_or(0.0);
    let speed_m_s = initial_true_airspeed(scenario.mission.initial_state.as_ref(), altitude_m)?;
    let mass_kg =
        aircraft.mass.operating_empty_mass_kg + scenario.mission.payload_mass_kg + fuel.loaded_kg;
    Ok(InitialMissionState {
        state: MissionState {
            mass_kg,
            altitude_m,
            speed_m_s,
            fuel_remaining_kg: fuel.loaded_kg,
            payload_remaining_kg: scenario.mission.payload_mass_kg,
        },
        capacity_exceeded: fuel.capacity_exceeded,
        warning: fuel.warning,
    })
}

fn requested_fuel_kg(state: Option<&MissionInitialState>, usable_fuel_kg: f64) -> f64 {
    state
        .and_then(|value| value.fuel_mass_kg)
        .or_else(|| {
            state
                .and_then(|value| value.fuel_fraction)
                .map(|fraction| fraction * usable_fuel_kg)
        })
        .unwrap_or(usable_fuel_kg)
}

fn initial_true_airspeed(
    state: Option<&MissionInitialState>,
    altitude_m: f64,
) -> AexResult<Option<f64>> {
    let Some(state) = state else {
        return Ok(None);
    };
    if let Some(speed) = state.true_airspeed_m_s {
        return Ok(Some(speed));
    }
    if state.mach.is_none() && state.indicated_airspeed_m_s.is_none() {
        return Ok(None);
    }
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    if let Some(mach) = state.mach {
        return Ok(Some(mach * atmosphere.speed_of_sound_m_s));
    }
    Ok(state
        .indicated_airspeed_m_s
        .map(|speed| speed * (1.225 / atmosphere.density_kg_m3).sqrt()))
}

pub(super) fn initial_fuel_load(requested_kg: f64, capacity_kg: f64) -> InitialFuelLoad {
    let capacity_exceeded = requested_kg > capacity_kg + 1.0e-8;
    InitialFuelLoad {
        loaded_kg: requested_kg.min(capacity_kg),
        capacity_exceeded,
        warning: capacity_exceeded.then(|| {
            Diagnostic::warning(
                WarningCode::FuelCapacityExceeded,
                format!(
                    "Requested initial fuel load {requested_kg} kg exceeds tank capacity \
                     {capacity_kg} kg."
                ),
                "mission.initial_fuel_load",
            )
        }),
    }
}

#[cfg(test)]
mod tests;

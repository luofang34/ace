use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::schema::EngineProfile;
use crate::models::aerodynamics::{FlightCondition, evaluate as evaluate_aerodynamics};
use crate::models::propulsion::{OperatingMode, PropulsionQuery, evaluate as evaluate_propulsion};

use super::{MissionSimulator, extend_unique_diagnostics};

#[derive(Debug)]
pub(super) struct FuelEvaluation {
    pub(super) fuel_kg: f64,
    pub(super) warnings: Vec<Diagnostic>,
}

#[derive(Debug)]
pub(super) struct FuelFlowEvaluation {
    pub(super) flow_kg_s: f64,
    pub(super) warnings: Vec<Diagnostic>,
}

pub(super) fn integrated(
    simulator: &MissionSimulator,
    altitude_m: f64,
    speed_m_s: f64,
    start_mass_kg: f64,
    duration_s: f64,
    mode: OperatingMode,
    engine_off: bool,
) -> AexResult<FuelEvaluation> {
    if engine_off {
        return Ok(FuelEvaluation {
            fuel_kg: 0.0,
            warnings: Vec::new(),
        });
    }
    let steps = 24_u32;
    let step_duration = duration_s / f64::from(steps);
    let mut mass = start_mass_kg;
    let mut fuel = 0.0;
    let mut warnings = Vec::new();
    for _ in 0..steps {
        let flow = required(simulator, altitude_m, speed_m_s, mass, mode)?;
        extend_unique_diagnostics(&mut warnings, flow.warnings);
        let burn = flow.flow_kg_s * step_duration;
        fuel += burn;
        mass -= burn;
    }
    Ok(FuelEvaluation {
        fuel_kg: fuel,
        warnings,
    })
}

fn required(
    simulator: &MissionSimulator,
    altitude_m: f64,
    speed_m_s: f64,
    mass_kg: f64,
    mode: OperatingMode,
) -> AexResult<FuelFlowEvaluation> {
    let atmosphere = simulator.atmosphere.evaluate(altitude_m)?;
    let aero = evaluate_aerodynamics(
        &simulator.scenario.aircraft,
        "clean",
        FlightCondition {
            density_kg_m3: atmosphere.density_kg_m3,
            speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
            true_airspeed_m_s: speed_m_s,
            mass_kg,
        },
    )?;
    let propulsion = evaluate_propulsion(
        &simulator.scenario,
        &atmosphere,
        PropulsionQuery {
            altitude_m,
            true_airspeed_m_s: speed_m_s,
            mach: speed_m_s / atmosphere.speed_of_sound_m_s,
            throttle: 1.0,
            mode,
        },
    )?;
    let flow_kg_s = required_flow(simulator, &aero, mode);
    let mut warnings = aero.warnings;
    extend_unique_diagnostics(&mut warnings, propulsion.warnings);
    Ok(FuelFlowEvaluation {
        flow_kg_s,
        warnings,
    })
}

fn required_flow(
    simulator: &MissionSimulator,
    aero: &crate::domain::result::AerodynamicState,
    mode: OperatingMode,
) -> f64 {
    match &simulator.scenario.engine {
        EngineProfile::Piston(profile) => {
            let efficiency = simulator
                .scenario
                .propeller
                .as_ref()
                .map_or(0.75, |propeller| propeller.cruise_efficiency);
            let shaft_power_kw = aero.power_required_w / efficiency / 1000.0;
            let bsfc = match mode {
                OperatingMode::Economy => profile.bsfc_economy_kg_kwh,
                _ => profile.bsfc_cruise_kg_kwh,
            };
            bsfc * shaft_power_kw / 3600.0
        }
        EngineProfile::Turbofan(profile) => profile.tsfc_cruise_kg_n_hr * aero.drag_n / 3600.0,
    }
}

pub(super) fn available(
    simulator: &MissionSimulator,
    altitude_m: f64,
    speed_m_s: f64,
    throttle: f64,
    mode: OperatingMode,
) -> AexResult<FuelFlowEvaluation> {
    let atmosphere = simulator.atmosphere.evaluate(altitude_m)?;
    let propulsion = evaluate_propulsion(
        &simulator.scenario,
        &atmosphere,
        PropulsionQuery {
            altitude_m,
            true_airspeed_m_s: speed_m_s,
            mach: speed_m_s / atmosphere.speed_of_sound_m_s,
            throttle,
            mode,
        },
    )?;
    Ok(FuelFlowEvaluation {
        flow_kg_s: propulsion.fuel_flow_kg_s,
        warnings: propulsion.warnings,
    })
}

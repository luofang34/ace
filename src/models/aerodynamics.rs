use std::f64::consts::PI;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::result::{AerodynamicState, ModelMetadata};
use crate::domain::schema::{AeroConfiguration, Aircraft};
use crate::domain::warning::WarningCode;

#[derive(Debug, Clone, Copy)]
pub(crate) struct FlightCondition {
    pub(crate) density_kg_m3: f64,
    pub(crate) speed_of_sound_m_s: f64,
    pub(crate) true_airspeed_m_s: f64,
    pub(crate) mass_kg: f64,
}

pub(crate) fn evaluate(
    aircraft: &Aircraft,
    configuration: &str,
    condition: FlightCondition,
) -> AexResult<AerodynamicState> {
    let coefficients = aircraft
        .aerodynamics
        .configuration(configuration)
        .ok_or_else(|| {
            AexError::validation(
                "UNSUPPORTED_CONFIGURATION",
                "condition.configuration",
                configuration,
            )
        })?;
    if condition.true_airspeed_m_s <= 0.0 {
        return Err(AexError::analysis(
            "INVALID_AIRSPEED",
            "true airspeed must be positive",
        ));
    }
    let dynamic_pressure = 0.5 * condition.density_kg_m3 * condition.true_airspeed_m_s.powi(2);
    let weight = condition.mass_kg * GRAVITY_M_S2;
    let lift_coefficient = weight / (dynamic_pressure * aircraft.wing.area_m2);
    let induced_factor = 1.0 / (PI * coefficients.oswald_efficiency * aircraft.wing.aspect_ratio);
    let induced_drag = induced_factor * lift_coefficient.powi(2);
    let mach = condition.true_airspeed_m_s / condition.speed_of_sound_m_s;
    let (wave_drag, warnings) = wave_drag(coefficients, mach);
    let drag_coefficient = coefficients.cd0 + coefficients.additional_cd + induced_drag + wave_drag;
    let drag = dynamic_pressure * aircraft.wing.area_m2 * drag_coefficient;
    Ok(AerodynamicState {
        dynamic_pressure_pa: dynamic_pressure,
        lift_coefficient,
        drag_coefficient,
        induced_drag_coefficient: induced_drag,
        wave_drag_coefficient: wave_drag,
        drag_n: drag,
        power_required_w: drag * condition.true_airspeed_m_s,
        lift_to_drag_ratio: lift_coefficient / drag_coefficient,
        model: ModelMetadata {
            model_id: "aero.parabolic_polar".to_owned(),
            model_version: "1.0.0".to_owned(),
            fidelity_level: 1,
            validity_status: if mach <= 0.90 {
                "valid"
            } else {
                "extrapolated"
            }
            .to_owned(),
        },
        warnings,
    })
}

pub(crate) fn stall_speed_m_s(
    aircraft: &Aircraft,
    configuration: &str,
    mass_kg: f64,
    density_kg_m3: f64,
) -> AexResult<f64> {
    let coefficients = aircraft
        .aerodynamics
        .configuration(configuration)
        .ok_or_else(|| {
            AexError::validation("UNSUPPORTED_CONFIGURATION", "configuration", configuration)
        })?;
    let numerator = 2.0 * mass_kg * GRAVITY_M_S2;
    Ok((numerator / (density_kg_m3 * aircraft.wing.area_m2 * coefficients.cl_max)).sqrt())
}

pub(crate) fn minimum_drag_speed_m_s(
    aircraft: &Aircraft,
    configuration: &str,
    mass_kg: f64,
    density_kg_m3: f64,
) -> AexResult<f64> {
    characteristic_speed(aircraft, configuration, mass_kg, density_kg_m3, 1.0)
}

pub(crate) fn minimum_power_speed_m_s(
    aircraft: &Aircraft,
    configuration: &str,
    mass_kg: f64,
    density_kg_m3: f64,
) -> AexResult<f64> {
    characteristic_speed(aircraft, configuration, mass_kg, density_kg_m3, 3.0)
}

fn characteristic_speed(
    aircraft: &Aircraft,
    configuration: &str,
    mass_kg: f64,
    density_kg_m3: f64,
    induced_divisor: f64,
) -> AexResult<f64> {
    let coefficients = aircraft
        .aerodynamics
        .configuration(configuration)
        .ok_or_else(|| {
            AexError::validation("UNSUPPORTED_CONFIGURATION", "configuration", configuration)
        })?;
    let parasite = coefficients.cd0 + coefficients.additional_cd;
    let induced_factor = 1.0 / (PI * coefficients.oswald_efficiency * aircraft.wing.aspect_ratio);
    let weight = mass_kg * GRAVITY_M_S2;
    let numerator = 4.0 * induced_factor * weight.powi(2);
    let denominator =
        induced_divisor * density_kg_m3.powi(2) * aircraft.wing.area_m2.powi(2) * parasite;
    Ok((numerator / denominator).powf(0.25))
}

pub(crate) fn maximum_lift_to_drag_ratio(
    aircraft: &Aircraft,
    configuration: &str,
) -> AexResult<f64> {
    let coefficients = aircraft
        .aerodynamics
        .configuration(configuration)
        .ok_or_else(|| {
            AexError::validation("UNSUPPORTED_CONFIGURATION", "configuration", configuration)
        })?;
    let parasite = coefficients.cd0 + coefficients.additional_cd;
    let induced_factor = 1.0 / (PI * coefficients.oswald_efficiency * aircraft.wing.aspect_ratio);
    Ok(1.0 / (2.0 * (parasite * induced_factor).sqrt()))
}

fn wave_drag(configuration: &AeroConfiguration, mach: f64) -> (f64, Vec<Diagnostic>) {
    let mut warnings = Vec::new();
    let drag = match (&configuration.wave_drag, configuration.mach_critical) {
        (Some(model), Some(critical)) if mach > critical => {
            warnings.push(Diagnostic::warning(
                WarningCode::TransonicDragApproximation,
                "Wave drag uses a simple power-law correction.",
                "aircraft.aerodynamics.clean.wave_drag",
            ));
            model.coefficient * (mach - critical).powf(model.exponent)
        }
        _ => 0.0,
    };
    if mach > 0.90 {
        warnings.push(Diagnostic::warning(
            WarningCode::ModelExtrapolation,
            format!("Parabolic polar evaluated at Mach {mach:.3}."),
            "condition.mach",
        ));
    }
    (drag, warnings)
}

#[cfg(test)]
mod tests;

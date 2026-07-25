use serde_json::json;

use crate::domain::aerodynamics::{PolarCoefficients, TABLE_POLAR_MODEL_ID};
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

#[derive(Debug)]
pub(crate) struct PolarEvaluation {
    pub(crate) coefficients: PolarCoefficients,
    pub(crate) warnings: Vec<Diagnostic>,
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
    let mach = condition.true_airspeed_m_s / condition.speed_of_sound_m_s;
    let polar_evaluation = coefficient_evaluation_at_mach(aircraft, configuration, mach)?;
    let polar = polar_evaluation.coefficients;
    let induced_drag = polar.induced_drag_factor * lift_coefficient.powi(2);
    let (wave_drag, mut warnings) = wave_drag(coefficients, configuration, mach);
    warnings.extend(polar_evaluation.warnings);
    let drag_coefficient = polar.cd0 + coefficients.additional_cd + induced_drag + wave_drag;
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
            model_id: if coefficients.polar_table.is_some() {
                TABLE_POLAR_MODEL_ID
            } else {
                "aero.parabolic_polar"
            }
            .to_owned(),
            model_version: "1.0.0".to_owned(),
            fidelity_level: 1,
            validity_status: if polar.extrapolated
                || coefficients.polar_table.is_none() && mach > 0.90
            {
                "extrapolated"
            } else {
                "valid"
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
    let polar = coefficients_at_mach(aircraft, configuration, 0.0)?;
    let numerator = 2.0 * mass_kg * GRAVITY_M_S2;
    Ok((numerator / (density_kg_m3 * aircraft.wing.area_m2 * polar.cl_max)).sqrt())
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
    let polar = coefficients_at_mach(aircraft, configuration, 0.0)?;
    let parasite = polar.cd0 + coefficients.additional_cd;
    let weight = mass_kg * GRAVITY_M_S2;
    let numerator = 4.0 * polar.induced_drag_factor * weight.powi(2);
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
    let polar = coefficients_at_mach(aircraft, configuration, 0.0)?;
    let parasite = polar.cd0 + coefficients.additional_cd;
    Ok(1.0 / (2.0 * (parasite * polar.induced_drag_factor).sqrt()))
}

pub(crate) fn coefficients_at_mach(
    aircraft: &Aircraft,
    configuration: &str,
    mach: f64,
) -> AexResult<PolarCoefficients> {
    Ok(coefficient_evaluation_at_mach(aircraft, configuration, mach)?.coefficients)
}

pub(crate) fn coefficient_evaluation_at_mach(
    aircraft: &Aircraft,
    configuration: &str,
    mach: f64,
) -> AexResult<PolarEvaluation> {
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
    let polar = coefficients.polar_coefficients(mach, aircraft.wing.aspect_ratio);
    if polar.cd0.is_finite()
        && polar.cd0 > 0.0
        && polar.cl_max.is_finite()
        && polar.cl_max > 0.0
        && polar.induced_drag_factor.is_finite()
        && polar.induced_drag_factor > 0.0
    {
        let mut warnings = Vec::new();
        add_table_warning(
            &mut warnings,
            coefficients,
            configuration,
            mach,
            polar.extrapolated,
        );
        Ok(PolarEvaluation {
            coefficients: polar,
            warnings,
        })
    } else {
        Err(AexError::analysis(
            "INVALID_POLAR_TABLE_EXTRAPOLATION",
            "polar-table interpolation produced a nonpositive or non-finite coefficient",
        ))
    }
}

fn wave_drag(
    configuration: &AeroConfiguration,
    configuration_name: &str,
    mach: f64,
) -> (f64, Vec<Diagnostic>) {
    let mut warnings = Vec::new();
    let drag = match (&configuration.wave_drag, configuration.mach_critical) {
        (Some(model), Some(critical)) if mach > critical => {
            warnings.push(Diagnostic::warning(
                WarningCode::TransonicDragApproximation,
                "Wave drag uses a simple power-law correction.",
                format!("aircraft.aerodynamics.{configuration_name}.wave_drag"),
            ));
            model.coefficient * (mach - critical).powf(model.exponent)
        }
        _ => 0.0,
    };
    if configuration.polar_table.is_none() && mach > 0.90 {
        warnings.push(Diagnostic::warning(
            WarningCode::ModelExtrapolation,
            format!("Parabolic polar evaluated at Mach {mach:.3}."),
            "condition.mach",
        ));
    }
    (drag, warnings)
}

fn add_table_warning(
    warnings: &mut Vec<Diagnostic>,
    configuration: &AeroConfiguration,
    configuration_name: &str,
    mach: f64,
    extrapolated: bool,
) {
    let Some(table) = configuration.polar_table.as_ref() else {
        return;
    };
    if !extrapolated {
        return;
    }
    let Some((minimum, maximum)) = table.mach.first().zip(table.mach.last()) else {
        return;
    };
    warnings.push(Diagnostic::warning_with_context(
        WarningCode::ModelExtrapolation,
        format!("Polar table extrapolated Mach {mach:.3} outside [{minimum:.3}, {maximum:.3}]."),
        format!("aircraft.aerodynamics.{configuration_name}.polar_table.mach"),
        json!({
            "value": mach,
            "minimum": minimum,
            "maximum": maximum,
            "unit": "1",
            "basis": "tabulated_data",
        }),
    ));
}

#[cfg(test)]
mod tests;

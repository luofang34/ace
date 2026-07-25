use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::MetricValidity;
use crate::models::aerodynamics::coefficient_evaluation_at_mach;

pub(crate) struct FieldPerformanceEstimate {
    pub(crate) distance_m: f64,
    pub(crate) validity: MetricValidity,
    pub(crate) warnings: Vec<Diagnostic>,
}

pub(crate) fn estimate_takeoff_distance(
    scenario: &ResolvedScenario,
) -> AexResult<FieldPerformanceEstimate> {
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    let wing_loading = mass * GRAVITY_M_S2 / scenario.aircraft.wing.area_m2;
    let evaluation = coefficient_evaluation_at_mach(&scenario.aircraft, "takeoff", 0.0)?;
    let lift_off = (2.0 * wing_loading / (1.225 * evaluation.coefficients.cl_max)).sqrt() * 1.2;
    let thrust_to_weight = match &scenario.engine {
        EngineProfile::Piston(profile) => profile.rated_power_w / (mass * GRAVITY_M_S2 * lift_off),
        EngineProfile::Turbofan(profile) => {
            profile.installed_reference_thrust_n(
                scenario.aircraft.propulsion.engine_count,
                scenario.aircraft.propulsion.sizing_factor,
            ) / (mass * GRAVITY_M_S2)
        }
    };
    Ok(FieldPerformanceEstimate {
        distance_m: lift_off.powi(2) / (2.0 * GRAVITY_M_S2 * (thrust_to_weight - 0.04).max(0.03)),
        validity: field_validity(evaluation.coefficients.extrapolated),
        warnings: evaluation.warnings,
    })
}

pub(crate) fn estimate_landing_distance(
    scenario: &ResolvedScenario,
) -> AexResult<FieldPerformanceEstimate> {
    let evaluation = coefficient_evaluation_at_mach(&scenario.aircraft, "landing", 0.0)?;
    let numerator = 2.0 * scenario.aircraft.mass.maximum_takeoff_mass_kg * GRAVITY_M_S2;
    let denominator = 1.225 * scenario.aircraft.wing.area_m2 * evaluation.coefficients.cl_max;
    let stall = (numerator / denominator).sqrt();
    let approach_speed = 1.3 * stall;
    let ground_roll = approach_speed.powi(2) / (2.0 * GRAVITY_M_S2 * 0.30);
    let obstacle_allowance = 15.24 / 3.0_f64.to_radians().tan();
    Ok(FieldPerformanceEstimate {
        distance_m: 1.35 * (ground_roll + obstacle_allowance),
        validity: field_validity(evaluation.coefficients.extrapolated),
        warnings: evaluation.warnings,
    })
}

fn field_validity(extrapolated: bool) -> MetricValidity {
    if extrapolated {
        MetricValidity::extrapolated()
    } else {
        MetricValidity::default()
    }
}

#[cfg(test)]
mod tests;

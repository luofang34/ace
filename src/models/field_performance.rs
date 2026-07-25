use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::MetricValidity;
use crate::models::aerodynamics::{
    FlightCondition, coefficient_evaluation_at_mach, evaluate as evaluate_aerodynamics,
    stall_speed_m_s,
};
use crate::models::atmosphere::Isa1976;
use crate::models::propulsion::{OperatingMode, PropulsionQuery, evaluate as evaluate_propulsion};

pub(crate) struct FieldPerformanceEstimate {
    pub(crate) distance_m: f64,
    pub(crate) validity: MetricValidity,
    pub(crate) warnings: Vec<Diagnostic>,
}

pub(crate) struct ClimbGradientEstimate {
    pub(crate) gradient: f64,
    pub(crate) validity: MetricValidity,
}

pub(crate) fn estimate_takeoff_distance(
    scenario: &ResolvedScenario,
) -> AexResult<FieldPerformanceEstimate> {
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    let wing_loading = mass * GRAVITY_M_S2 / scenario.aircraft.wing.area_m2;
    let evaluation = coefficient_evaluation_at_mach(&scenario.aircraft, "takeoff", 0.0)?;
    let lift_off = (2.0 * wing_loading / (1.225 * evaluation.coefficients.cl_max)).sqrt() * 1.2;
    let thrust_to_weight = match &scenario.engine {
        EngineProfile::Piston(profile) => {
            let installed_power = profile.rated_power_w
                * f64::from(scenario.aircraft.propulsion.engine_count)
                * scenario.aircraft.propulsion.sizing_factor;
            installed_power / (mass * GRAVITY_M_S2 * lift_off)
        }
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

pub(crate) fn estimate_second_segment_climb_gradient(
    scenario: &ResolvedScenario,
    operating_engine_count: u32,
) -> AexResult<ClimbGradientEstimate> {
    if operating_engine_count == 0
        || operating_engine_count > scenario.aircraft.propulsion.engine_count
    {
        return Err(crate::domain::diagnostic::AexError::validation(
            "INVALID_OPERATING_ENGINE_COUNT",
            "aircraft.propulsion.engine_count",
            "operating engine count must be positive and cannot exceed the installed count",
        ));
    }
    let mut operating = scenario.clone();
    operating.aircraft.propulsion.engine_count = operating_engine_count;
    let atmosphere = Isa1976::new(0.0).evaluate(0.0)?;
    let mass = operating.aircraft.mass.maximum_takeoff_mass_kg;
    let stall = stall_speed_m_s(
        &operating.aircraft,
        "takeoff",
        mass,
        atmosphere.density_kg_m3,
    )?;
    let speed = 1.2 * stall;
    let aerodynamics = evaluate_aerodynamics(
        &operating.aircraft,
        "takeoff",
        FlightCondition {
            density_kg_m3: atmosphere.density_kg_m3,
            speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
            true_airspeed_m_s: speed,
            mass_kg: mass,
        },
    )?;
    let propulsion = evaluate_propulsion(
        &operating,
        &atmosphere,
        PropulsionQuery {
            altitude_m: 0.0,
            true_airspeed_m_s: speed,
            mach: speed / atmosphere.speed_of_sound_m_s,
            throttle: 1.0,
            mode: OperatingMode::Takeoff,
        },
    )?;
    let thrust = propulsion
        .thrust_available_n
        .or_else(|| {
            propulsion
                .propulsive_power_available_w
                .map(|power| power / speed)
        })
        .ok_or_else(|| {
            crate::domain::diagnostic::AexError::analysis(
                "CLIMB_PROPULSION_UNAVAILABLE",
                "propulsion model exposes neither thrust nor propulsive power",
            )
        })?;
    let extrapolated = aerodynamics.model.validity_status == "extrapolated"
        || propulsion.model.validity_status == "extrapolated";
    Ok(ClimbGradientEstimate {
        gradient: (thrust - aerodynamics.drag_n) / (mass * GRAVITY_M_S2),
        validity: field_validity(extrapolated),
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

use crate::domain::diagnostic::AexResult;
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::models::aerodynamics::stall_speed_m_s;

pub(crate) fn estimate_takeoff_distance_m(scenario: &ResolvedScenario) -> f64 {
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    let wing_loading = mass * GRAVITY_M_S2 / scenario.aircraft.wing.area_m2;
    let lift_off =
        (2.0 * wing_loading / (1.225 * scenario.aircraft.aerodynamics.takeoff.cl_max)).sqrt() * 1.2;
    let thrust_to_weight = match &scenario.engine {
        EngineProfile::Piston(profile) => profile.rated_power_w / (mass * GRAVITY_M_S2 * lift_off),
        EngineProfile::Turbofan(profile) => {
            profile.installed_reference_thrust_n(
                scenario.aircraft.propulsion.engine_count,
                scenario.aircraft.propulsion.sizing_factor,
            ) / (mass * GRAVITY_M_S2)
        }
    };
    lift_off.powi(2) / (2.0 * GRAVITY_M_S2 * (thrust_to_weight - 0.04).max(0.03))
}

pub(crate) fn estimate_landing_distance_m(scenario: &ResolvedScenario) -> AexResult<f64> {
    let stall = stall_speed_m_s(
        &scenario.aircraft,
        "landing",
        scenario.aircraft.mass.maximum_takeoff_mass_kg,
        1.225,
    )?;
    let approach_speed = 1.3 * stall;
    let ground_roll = approach_speed.powi(2) / (2.0 * GRAVITY_M_S2 * 0.30);
    let obstacle_allowance = 15.24 / 3.0_f64.to_radians().tan();
    Ok(1.35 * (ground_roll + obstacle_allowance))
}

#[cfg(test)]
mod tests;

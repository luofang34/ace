use serde_json::json;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::result::AtmosphereState;
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::MetricValidity;
use crate::domain::warning::WarningCode;
use crate::models::aerodynamics::{
    FlightCondition, coefficient_evaluation_at_mach, evaluate as evaluate_aerodynamics,
    stall_speed_m_s,
};
use crate::models::atmosphere::{Isa1976, scenario_atmosphere};
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

/// Highest field elevation the screening estimate is registered for.
pub(crate) const MAXIMUM_FIELD_ELEVATION_M: f64 = 4_500.0;
/// Runway grade magnitude beyond which the screening estimate is extrapolated.
const MAXIMUM_RUNWAY_SLOPE: f64 = 0.02;
/// Tailwind beyond which the screening estimate is extrapolated (10 kt).
const MAXIMUM_TAILWIND_M_S: f64 = 5.144;
/// Rolling resistance plus aerodynamic drag share during the takeoff roll, in g.
const ROLLING_RESISTANCE: f64 = 0.04;
/// Smallest net takeoff acceleration the screening form evaluates, in g.
const MINIMUM_NET_ACCELERATION: f64 = 0.03;

pub(crate) fn estimate_takeoff_distance(
    scenario: &ResolvedScenario,
) -> AexResult<FieldPerformanceEstimate> {
    let field = &scenario.mission.field;
    let mass = field
        .takeoff_mass_kg
        .unwrap_or(scenario.aircraft.mass.maximum_takeoff_mass_kg);
    let atmosphere = scenario_atmosphere(scenario).evaluate(field.elevation_m)?;
    let wing_loading = mass * GRAVITY_M_S2 / scenario.aircraft.wing.area_m2;
    let evaluation = coefficient_evaluation_at_mach(&scenario.aircraft, "takeoff", 0.0)?;
    let lift_off =
        (2.0 * wing_loading / (atmosphere.density_kg_m3 * evaluation.coefficients.cl_max)).sqrt()
            * 1.2;
    let lapse = takeoff_propulsion_lapse(scenario, &atmosphere, lift_off)?;
    let thrust_to_weight = lapse.ratio
        * match &scenario.engine {
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
    let mut warnings = evaluation.warnings;
    warnings.extend(lapse.warnings);
    // Net longitudinal acceleration in g: thrust less rolling resistance and the
    // uphill grade. The screening form is not credible near zero, so a floor keeps the
    // estimate finite and is reported rather than applied silently.
    let net_acceleration = thrust_to_weight - ROLLING_RESISTANCE - field.runway_slope;
    let floored = net_acceleration < MINIMUM_NET_ACCELERATION;
    if floored {
        warnings.push(Diagnostic::warning_with_context(
            WarningCode::ModelExtrapolation,
            format!(
                "takeoff net acceleration {net_acceleration:.3} g is below the {MINIMUM_NET_ACCELERATION} g screening floor"
            ),
            "performance.takeoff_field_length",
            json!({"net_acceleration_g": net_acceleration, "floor_g": MINIMUM_NET_ACCELERATION}),
        ));
    }
    // The ground roll covers the change in ground speed, so headwind shortens it.
    let lift_off_ground_speed = (lift_off - field.headwind_m_s).max(0.0);
    Ok(FieldPerformanceEstimate {
        distance_m: lift_off_ground_speed.powi(2)
            / (2.0 * GRAVITY_M_S2 * net_acceleration.max(MINIMUM_NET_ACCELERATION)),
        validity: field_validity(
            evaluation.coefficients.extrapolated
                || lapse.extrapolated
                || floored
                || outside_field_domain(scenario, mass),
        ),
        warnings,
    })
}

pub(crate) fn estimate_landing_distance(
    scenario: &ResolvedScenario,
) -> AexResult<FieldPerformanceEstimate> {
    let field = &scenario.mission.field;
    let mass = field
        .landing_mass_kg
        .unwrap_or(scenario.aircraft.mass.maximum_takeoff_mass_kg);
    let atmosphere = scenario_atmosphere(scenario).evaluate(field.elevation_m)?;
    let evaluation = coefficient_evaluation_at_mach(&scenario.aircraft, "landing", 0.0)?;
    let numerator = 2.0 * mass * GRAVITY_M_S2;
    let denominator =
        atmosphere.density_kg_m3 * scenario.aircraft.wing.area_m2 * evaluation.coefficients.cl_max;
    let stall = (numerator / denominator).sqrt();
    let approach_speed = 1.3 * stall;
    let approach_ground_speed = (approach_speed - field.headwind_m_s).max(0.0);
    // Braking plus the uphill grade decelerate the ground roll.
    let deceleration = field.braking_coefficient + field.runway_slope;
    if deceleration <= 0.0 {
        return Err(AexError::validation(
            "LANDING_DECELERATION_NONPOSITIVE",
            "mission.field.runway_slope",
            "the downhill grade exceeds the braking coefficient",
        ));
    }
    let ground_roll = approach_ground_speed.powi(2) / (2.0 * GRAVITY_M_S2 * deceleration);
    // A 3 degree air segment covers less ground in a headwind.
    let obstacle_allowance = field.obstacle_height_m / 3.0_f64.to_radians().tan()
        * approach_ground_speed
        / approach_speed;
    Ok(FieldPerformanceEstimate {
        distance_m: 1.35 * (ground_roll + obstacle_allowance),
        validity: field_validity(
            evaluation.coefficients.extrapolated || outside_field_domain(scenario, mass),
        ),
        warnings: evaluation.warnings,
    })
}

struct TakeoffLapse {
    ratio: f64,
    extrapolated: bool,
    warnings: Vec<Diagnostic>,
}

/// Installed takeoff thrust at the field relative to sea-level standard day, from
/// the engine's own lapse model; the sea-level estimate is scaled by this ratio.
fn takeoff_propulsion_lapse(
    scenario: &ResolvedScenario,
    field: &AtmosphereState,
    lift_off_m_s: f64,
) -> AexResult<TakeoffLapse> {
    let thrust = |atmosphere: &AtmosphereState| {
        evaluate_propulsion(
            scenario,
            atmosphere,
            PropulsionQuery {
                altitude_m: atmosphere.altitude_m,
                true_airspeed_m_s: lift_off_m_s,
                mach: lift_off_m_s / atmosphere.speed_of_sound_m_s,
                throttle: 1.0,
                mode: OperatingMode::Takeoff,
            },
        )
    };
    let reference = thrust(&Isa1976::new(0.0).evaluate(0.0)?)?;
    let actual = thrust(field)?;
    let available = |state: &crate::domain::result::PropulsionState| {
        state
            .thrust_available_n
            .or_else(|| {
                state
                    .propulsive_power_available_w
                    .map(|power| power / lift_off_m_s)
            })
            .unwrap_or(0.0)
    };
    let reference_thrust = available(&reference);
    Ok(TakeoffLapse {
        ratio: if reference_thrust > 0.0 {
            available(&actual) / reference_thrust
        } else {
            1.0
        },
        extrapolated: actual.model.validity_status == "extrapolated",
        warnings: actual.warnings,
    })
}

fn outside_field_domain(scenario: &ResolvedScenario, mass_kg: f64) -> bool {
    let field = &scenario.mission.field;
    !(0.0..=MAXIMUM_FIELD_ELEVATION_M).contains(&field.elevation_m)
        || mass_kg > scenario.aircraft.mass.maximum_takeoff_mass_kg
        || field.runway_slope.abs() > MAXIMUM_RUNWAY_SLOPE
        || field.headwind_m_s < -MAXIMUM_TAILWIND_M_S
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
    let field = &scenario.mission.field;
    let atmosphere = scenario_atmosphere(scenario).evaluate(field.elevation_m)?;
    let mass = field
        .takeoff_mass_kg
        .unwrap_or(operating.aircraft.mass.maximum_takeoff_mass_kg);
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
            altitude_m: field.elevation_m,
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

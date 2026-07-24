use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{MissionResult, PerformanceSummary, RequirementEvaluation};
use crate::domain::schema::{Requirement, ResolvedScenario};
use crate::models::field_performance::estimate_takeoff_distance_m;

pub(crate) fn evaluate_requirements(
    scenario: &ResolvedScenario,
    mission: &MissionResult,
    performance: &PerformanceSummary,
) -> Vec<RequirementEvaluation> {
    scenario
        .requirements
        .items
        .iter()
        .filter_map(|requirement| {
            metric_value(requirement, scenario, mission, performance)
                .map(|actual| evaluate_one(requirement, actual))
        })
        .collect()
}

fn metric_value(
    requirement: &Requirement,
    scenario: &ResolvedScenario,
    mission: &MissionResult,
    performance: &PerformanceSummary,
) -> Option<f64> {
    match requirement.metric.as_str() {
        "mission.payload_mass" => Some(scenario.mission.payload_mass_kg),
        "performance.cruise_true_airspeed" => scenario
            .mission
            .segments
            .iter()
            .find_map(|segment| segment.true_airspeed_m_s),
        "mission.completed_distance" => Some(mission.total_distance.value),
        "performance.service_ceiling" => Some(performance.service_ceiling_m),
        "performance.stall_speed_landing" => Some(performance.stall_speed_landing_m_s),
        "performance.cruise_mach" => performance.cruise_mach,
        "performance.takeoff_field_length" => Some(approximate_takeoff_distance(scenario)),
        _ => None,
    }
}

fn evaluate_one(requirement: &Requirement, actual: f64) -> RequirementEvaluation {
    let margin = match requirement.operator.as_str() {
        "le" => requirement.required - actual,
        "eq" => -(actual - requirement.required).abs(),
        _ => actual - requirement.required,
    };
    let passed = margin >= -1.0e-9;
    let percentage = if requirement.required.abs() > 1.0e-12 {
        Some(100.0 * margin / requirement.required.abs())
    } else {
        None
    };
    RequirementEvaluation {
        id: requirement.id.clone(),
        metric: requirement.metric.clone(),
        actual: quantity(actual, &requirement.unit, &requirement.metric),
        required: quantity(requirement.required, &requirement.unit, &requirement.metric),
        operator: requirement.operator.clone(),
        passed,
        absolute_margin: margin,
        percentage_margin: percentage,
        severity: requirement.severity.clone(),
        warning_state: margin.abs() <= requirement.required.abs() * 1.0e-6,
    }
}

fn quantity(value: f64, unit: &str, metric: &str) -> QuantityOutput {
    if metric == "mission.completed_distance" {
        QuantityOutput::range(value)
    } else {
        QuantityOutput::si(value, unit)
    }
}

fn approximate_takeoff_distance(scenario: &ResolvedScenario) -> f64 {
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    let wing_loading =
        mass * crate::domain::quantity::GRAVITY_M_S2 / scenario.aircraft.wing.area_m2;
    let lift_off =
        (2.0 * wing_loading / (1.225 * scenario.aircraft.aerodynamics.takeoff.cl_max)).sqrt() * 1.2;
    let thrust_to_weight = match &scenario.engine {
        crate::domain::schema::EngineProfile::Piston(profile) => {
            profile.rated_power_w / (mass * crate::domain::quantity::GRAVITY_M_S2 * lift_off)
        }
        crate::domain::schema::EngineProfile::Turbofan(profile) => {
            profile.sea_level_static_thrust_n * f64::from(scenario.aircraft.propulsion.engine_count)
                / (mass * crate::domain::quantity::GRAVITY_M_S2)
        }
    };
    lift_off.powi(2)
        / (2.0 * crate::domain::quantity::GRAVITY_M_S2 * (thrust_to_weight - 0.04).max(0.03))
}

use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{
    MissionResult, PayloadRangeResult, PerformanceSummary, RequirementEvaluation,
};
use crate::domain::schema::{Requirement, ResolvedScenario, SegmentKind};
use crate::models::field_performance::estimate_takeoff_distance_m;

pub(crate) fn evaluate_requirements(
    scenario: &ResolvedScenario,
    mission: &MissionResult,
    performance: &PerformanceSummary,
    payload_range: Option<&PayloadRangeResult>,
) -> Vec<RequirementEvaluation> {
    scenario
        .requirements
        .items
        .iter()
        .filter_map(|requirement| {
            metric_value(requirement, scenario, mission, performance, payload_range)
                .map(|actual| evaluate_one(requirement, actual))
        })
        .collect()
}

pub(crate) fn hard_requirements_passed(
    mission_completed: bool,
    declared: &[Requirement],
    evaluations: &[RequirementEvaluation],
) -> bool {
    let (passed, total) = hard_requirement_counts(declared, evaluations);
    mission_completed && passed == total
}

pub(crate) fn hard_requirement_counts(
    declared: &[Requirement],
    evaluations: &[RequirementEvaluation],
) -> (usize, usize) {
    let hard = declared
        .iter()
        .filter(|requirement| requirement.severity == "hard");
    let total = hard.clone().count();
    let passed = hard
        .filter(|requirement| requirement_passed(requirement, evaluations))
        .count();
    (passed, total)
}

pub(crate) fn failed_hard_requirement_ids(
    declared: &[Requirement],
    evaluations: &[RequirementEvaluation],
) -> Vec<String> {
    declared
        .iter()
        .filter(|requirement| requirement.severity == "hard")
        .filter(|requirement| !requirement_passed(requirement, evaluations))
        .map(|requirement| requirement.id.clone())
        .collect()
}

fn requirement_passed(requirement: &Requirement, evaluations: &[RequirementEvaluation]) -> bool {
    evaluations.iter().any(|evaluation| {
        evaluation.id == requirement.id
            && evaluation.metric == requirement.metric
            && evaluation.passed
    })
}

fn metric_value(
    requirement: &Requirement,
    scenario: &ResolvedScenario,
    mission: &MissionResult,
    performance: &PerformanceSummary,
    payload_range: Option<&PayloadRangeResult>,
) -> Option<f64> {
    match requirement.metric.as_str() {
        "mission.payload_mass" => Some(scenario.mission.payload_mass_kg),
        "performance.cruise_true_airspeed" => scenario
            .mission
            .segments
            .iter()
            .filter(|segment| segment.kind == SegmentKind::Cruise)
            .find_map(|segment| segment.true_airspeed_m_s),
        "mission.completed_distance" => Some(mission.total_distance.value),
        "performance.service_ceiling" => Some(performance.service_ceiling_m),
        "performance.stall_speed_landing" => Some(performance.stall_speed_landing_m_s),
        "performance.cruise_mach" => performance.cruise_mach,
        "performance.takeoff_field_length" => Some(estimate_takeoff_distance_m(scenario)),
        "performance.full_payload_range" => {
            payload_range.and_then(|result| point_range(result, "full_payload_mission"))
        }
        "performance.zero_payload_ferry_range" => {
            payload_range.and_then(|result| point_range(result, "zero_payload_ferry"))
        }
        _ => None,
    }
}

fn point_range(result: &PayloadRangeResult, id: &str) -> Option<f64> {
    result
        .points
        .iter()
        .find(|point| point.id == id)
        .map(|point| point.range.value)
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
    if matches!(
        metric,
        "mission.completed_distance"
            | "performance.full_payload_range"
            | "performance.zero_payload_ferry_range"
    ) {
        QuantityOutput::range(value)
    } else {
        QuantityOutput::si(value, unit)
    }
}

#[cfg(test)]
mod tests;

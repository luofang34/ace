use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{
    MissionResult, PayloadRangeResult, PerformanceSummary, RequirementEvaluation, RequirementStatus,
};
use crate::domain::schema::{Requirement, ResolvedScenario};
use crate::domain::validity::{MetricValidity, ValidityStatus};
use crate::models::field_performance::estimate_takeoff_distance_m;

struct MetricInput {
    actual: f64,
    validity: MetricValidity,
}

impl MetricInput {
    fn valid(actual: f64) -> Self {
        Self {
            actual,
            validity: MetricValidity::default(),
        }
    }
}

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
                .map(|input| evaluate_one(requirement, input))
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
            && evaluation.is_passed()
    })
}

fn metric_value(
    requirement: &Requirement,
    scenario: &ResolvedScenario,
    mission: &MissionResult,
    performance: &PerformanceSummary,
    payload_range: Option<&PayloadRangeResult>,
) -> Option<MetricInput> {
    match requirement.metric.as_str() {
        "mission.payload_mass" => Some(MetricInput::valid(scenario.mission.payload_mass_kg)),
        "performance.achieved_cruise_true_airspeed" => performance
            .achieved_cruise_true_airspeed_m_s
            .map(|actual| performance_input(performance, &requirement.metric, actual)),
        "mission.completed_distance" => Some(MetricInput::valid(mission.total_distance.value)),
        "performance.service_ceiling" => Some(MetricInput {
            actual: performance.service_ceiling_m,
            validity: performance.validity_for("performance.service_ceiling"),
        }),
        "performance.stall_speed_landing" => {
            Some(MetricInput::valid(performance.stall_speed_landing_m_s))
        }
        "performance.achieved_cruise_mach" => performance
            .achieved_cruise_mach
            .map(|actual| performance_input(performance, &requirement.metric, actual)),
        "performance.minimum_cruise_excess_power" => performance
            .minimum_cruise_excess_power_w
            .map(|actual| performance_input(performance, &requirement.metric, actual)),
        "performance.cruise_feasible" => performance.cruise_feasible.map(|actual| {
            performance_input(
                performance,
                &requirement.metric,
                f64::from(u8::from(actual)),
            )
        }),
        "performance.takeoff_field_length" => {
            Some(MetricInput::valid(estimate_takeoff_distance_m(scenario)))
        }
        "performance.full_payload_range" => payload_range
            .and_then(|result| point_range(result, "full_payload_mission"))
            .map(MetricInput::valid),
        "performance.zero_payload_ferry_range" => payload_range
            .and_then(|result| point_range(result, "zero_payload_ferry"))
            .map(MetricInput::valid),
        _ => None,
    }
}

fn performance_input(performance: &PerformanceSummary, metric: &str, actual: f64) -> MetricInput {
    MetricInput {
        actual,
        validity: performance.validity_for(metric),
    }
}

fn point_range(result: &PayloadRangeResult, id: &str) -> Option<f64> {
    result
        .points
        .iter()
        .find(|point| point.id == id)
        .map(|point| point.range.value)
}

fn evaluate_one(requirement: &Requirement, input: MetricInput) -> RequirementEvaluation {
    let actual = input.actual;
    let margin = match requirement.operator.as_str() {
        "le" => requirement.required - actual,
        "eq" => -(actual - requirement.required).abs(),
        _ => actual - requirement.required,
    };
    let numerical_pass = margin >= -1.0e-9;
    let (status, passed) = if input.validity.status == ValidityStatus::BoundaryLimited {
        (RequirementStatus::Indeterminate, None)
    } else if numerical_pass {
        (RequirementStatus::Pass, Some(true))
    } else {
        (RequirementStatus::Fail, Some(false))
    };
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
        status: Some(status),
        passed,
        validity: input.validity,
        absolute_margin: margin,
        percentage_margin: percentage,
        severity: requirement.severity.clone(),
        warning_state: status != RequirementStatus::Indeterminate
            && margin.abs() <= requirement.required.abs() * 1.0e-6,
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

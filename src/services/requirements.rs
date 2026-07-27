use crate::domain::capabilities::{RequirementMetric, requirement_metric};
use crate::domain::diagnostic::AexResult;
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{
    MassPropertiesAnalysis, MissionResult, PayloadRangeResult, PerformanceSummary,
    RequirementEvaluation, RequirementStatus,
};
use crate::domain::schema::{Requirement, ResolvedScenario, SegmentKind};
use crate::domain::validity::{MetricValidity, ValidityStatus};
use crate::models::field_performance::{
    FieldPerformanceEstimate, estimate_landing_distance, estimate_second_segment_climb_gradient,
    estimate_takeoff_distance,
};
use crate::models::mass_properties;

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
) -> AexResult<Vec<RequirementEvaluation>> {
    let mass_properties = mass_properties::evaluate(scenario, mission)?;
    let mut evaluations = Vec::new();
    for requirement in &scenario.requirements.items {
        if let Some(input) = metric_value(
            requirement,
            scenario,
            mission,
            performance,
            payload_range,
            &mass_properties,
        )? {
            evaluations.push(evaluate_one(requirement, input));
        }
    }
    Ok(evaluations)
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
    mass_properties: &MassPropertiesAnalysis,
) -> AexResult<Option<MetricInput>> {
    let Some(metric) = requirement_metric(&requirement.metric).map(|item| item.metric) else {
        return Ok(None);
    };
    let input = match metric {
        RequirementMetric::MissionPayloadMass => mission_payload_input(scenario),
        RequirementMetric::AchievedCruiseTrueAirspeed => performance
            .achieved_cruise_true_airspeed_m_s
            .map(|actual| performance_input(performance, &requirement.metric, actual)),
        RequirementMetric::MissionCompletedDistance => valid_input(mission.total_distance.value),
        RequirementMetric::MissionLandingFuel => mission
            .landing_fuel
            .as_ref()
            .map(|fuel| MetricInput::valid(fuel.value)),
        RequirementMetric::MissionReserveDuration => {
            valid_input(reserve_duration_s(scenario, mission))
        }
        RequirementMetric::ServiceCeiling => Some(MetricInput {
            actual: performance.service_ceiling_m,
            validity: performance.validity_for("performance.service_ceiling"),
        }),
        RequirementMetric::StallSpeedLanding => Some(performance_input(
            performance,
            &requirement.metric,
            performance.stall_speed_landing_m_s,
        )),
        RequirementMetric::AchievedCruiseMach => performance
            .achieved_cruise_mach
            .map(|actual| performance_input(performance, &requirement.metric, actual)),
        RequirementMetric::MinimumCruiseExcessPower => performance
            .minimum_cruise_excess_power_w
            .map(|actual| performance_input(performance, &requirement.metric, actual)),
        RequirementMetric::CruiseFeasible => performance.cruise_feasible.map(|actual| {
            performance_input(
                performance,
                &requirement.metric,
                f64::from(u8::from(actual)),
            )
        }),
        RequirementMetric::TakeoffFieldLength => field_input(estimate_takeoff_distance(scenario)?),
        RequirementMetric::LandingFieldLength => field_input(estimate_landing_distance(scenario)?),
        RequirementMetric::AllEngineClimbGradient => Some(climb_gradient_input(
            scenario,
            scenario.aircraft.propulsion.engine_count,
        )?),
        RequirementMetric::OeiSecondSegmentClimbGradient => {
            Some(oei_climb_gradient_input(scenario)?)
        }
        RequirementMetric::FullPayloadRange => payload_range
            .and_then(|result| point_range(result, "full_payload_mission"))
            .map(MetricInput::valid),
        RequirementMetric::ZeroPayloadFerryRange => payload_range
            .and_then(|result| point_range(result, "zero_payload_ferry"))
            .map(MetricInput::valid),
        RequirementMetric::MinimumCenterOfGravity => Some(MetricInput::valid(
            mass_properties.minimum_center_of_gravity.value,
        )),
        RequirementMetric::MaximumCenterOfGravity => Some(MetricInput::valid(
            mass_properties.maximum_center_of_gravity.value,
        )),
        RequirementMetric::MinimumStaticMargin => {
            optional_stability_input(mass_properties.minimum_static_margin)
        }
        RequirementMetric::MaximumStaticMargin => {
            optional_stability_input(mass_properties.maximum_static_margin)
        }
        RequirementMetric::DeclaredCruiseMach | RequirementMetric::DeclaredCruiseTrueAirspeed => {
            None
        }
    };
    Ok(input)
}

fn valid_input(actual: f64) -> Option<MetricInput> {
    Some(MetricInput::valid(actual))
}

fn mission_payload_input(scenario: &ResolvedScenario) -> Option<MetricInput> {
    valid_input(scenario.mission.payload_mass_kg)
}

fn field_input(estimate: FieldPerformanceEstimate) -> Option<MetricInput> {
    Some(field_distance_input(estimate))
}

fn optional_stability_input(value: Option<f64>) -> Option<MetricInput> {
    Some(value.map_or_else(
        || MetricInput {
            actual: 0.0,
            validity: MetricValidity::unsupported(),
        },
        MetricInput::valid,
    ))
}

fn field_distance_input(estimate: FieldPerformanceEstimate) -> MetricInput {
    MetricInput {
        actual: estimate.distance_m,
        validity: estimate.validity,
    }
}

fn oei_climb_gradient_input(scenario: &ResolvedScenario) -> AexResult<MetricInput> {
    let operating_engines = scenario
        .aircraft
        .propulsion
        .engine_count
        .checked_sub(1)
        .filter(|count| *count > 0)
        .ok_or_else(|| {
            crate::domain::diagnostic::AexError::validation(
                "OEI_REQUIRES_MULTIPLE_ENGINES",
                "aircraft.propulsion.engine_count",
                "OEI second-segment climb requires at least two installed engines",
            )
        })?;
    climb_gradient_input(scenario, operating_engines)
}

fn climb_gradient_input(
    scenario: &ResolvedScenario,
    operating_engine_count: u32,
) -> AexResult<MetricInput> {
    let estimate = estimate_second_segment_climb_gradient(scenario, operating_engine_count)?;
    Ok(MetricInput {
        actual: estimate.gradient,
        validity: estimate.validity,
    })
}

fn reserve_duration_s(scenario: &ResolvedScenario, mission: &MissionResult) -> f64 {
    scenario
        .mission
        .segments
        .iter()
        .filter(|segment| segment.kind == SegmentKind::Reserve)
        .filter_map(|segment| {
            mission
                .segments
                .iter()
                .find(|result| result.segment_id == segment.id)
        })
        .map(|result| result.duration_s)
        .sum()
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
    let (status, passed) = if matches!(
        input.validity.status,
        ValidityStatus::BoundaryLimited | ValidityStatus::Unsupported
    ) {
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
        provenance: Some(requirement.provenance.clone()),
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

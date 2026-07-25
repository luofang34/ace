use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::{GRAVITY_M_S2, QuantityOutput};
use crate::domain::result::ResultProvenance;
use crate::domain::schema::EngineProfile;
use crate::domain::validity::MetricValidity;
use crate::domain::warning::WarningCode;
use crate::services::analysis::ApplicationService;
use crate::services::requirements::{evaluate_requirements, hard_requirements_passed};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScenarioComparison {
    pub(crate) scenarios: Vec<ComparisonRow>,
    pub(crate) warnings: Vec<Diagnostic>,
    pub(crate) provenance: ResultProvenance,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ComparisonRow {
    pub(crate) scenario_id: String,
    pub(crate) metrics: BTreeMap<String, QuantityOutput>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) metric_validity: BTreeMap<String, MetricValidity>,
}

impl ApplicationService {
    pub(crate) fn compare_blocking(
        &self,
        paths: &[PathBuf],
        metrics: &[String],
    ) -> AexResult<ScenarioComparison> {
        let mut rows = Vec::new();
        let mut propulsion_kinds = Vec::new();
        let mut warnings = Vec::new();
        for path in paths {
            let (scenario, performance) = self.performance_blocking(path, &BTreeMap::new())?;
            let (_, mission) = self.mission_blocking(path, &BTreeMap::new())?;
            let (_, payload_range) = self.payload_range_blocking(path, &BTreeMap::new())?;
            append_scenario_warnings(
                &mut warnings,
                &scenario.id,
                performance
                    .warnings
                    .iter()
                    .chain(&mission.warnings)
                    .chain(&payload_range.warnings),
            );
            propulsion_kinds.push(match scenario.engine {
                EngineProfile::Piston(_) => "power",
                EngineProfile::Turbofan(_) => "thrust",
            });
            let values = metrics
                .iter()
                .map(|metric| {
                    comparison_metric(metric, &scenario, &performance, &mission, &payload_range)
                        .map(|value| (metric.clone(), value))
                })
                .collect::<AexResult<BTreeMap<_, _>>>()?;
            rows.push(ComparisonRow {
                scenario_id: scenario.id,
                metrics: values,
                metric_validity: metrics
                    .iter()
                    .filter_map(|metric| {
                        performance
                            .metric_validity
                            .get(metric)
                            .map(|validity| (metric.clone(), validity.clone()))
                    })
                    .collect(),
            });
        }
        if propulsion_kinds.windows(2).any(|pair| pair[0] != pair[1]) {
            warnings.push(Diagnostic::warning(
                WarningCode::CrossClassComparison,
                "Power-loading and thrust-loading metrics are not directly comparable.",
                "scenario_paths",
            ));
        }
        Ok(ScenarioComparison {
            scenarios: rows,
            warnings,
            provenance: comparison_provenance(),
        })
    }
}

fn append_scenario_warnings<'a>(
    target: &mut Vec<Diagnostic>,
    scenario_id: &str,
    warnings: impl Iterator<Item = &'a Diagnostic>,
) {
    for warning in warnings {
        let mut contextualized = warning.clone();
        if let Some(context) = contextualized.context.as_object_mut() {
            context.insert(
                "scenario_id".to_owned(),
                serde_json::Value::String(scenario_id.to_owned()),
            );
        }
        if !target.contains(&contextualized) {
            target.push(contextualized);
        }
    }
}

fn comparison_metric(
    metric: &str,
    scenario: &crate::domain::schema::ResolvedScenario,
    performance: &crate::domain::result::PerformanceSummary,
    mission: &crate::domain::result::MissionResult,
    payload_range: &crate::domain::result::PayloadRangeResult,
) -> AexResult<QuantityOutput> {
    match metric {
        "performance.wing_loading" => Ok(QuantityOutput::si(
            scenario.aircraft.mass.maximum_takeoff_mass_kg * GRAVITY_M_S2
                / scenario.aircraft.wing.area_m2,
            "N/m^2",
        )),
        "performance.thrust_or_power_loading" => Ok(QuantityOutput::si(
            installed_loading(scenario),
            match scenario.engine {
                EngineProfile::Piston(_) => "W/kg",
                EngineProfile::Turbofan(_) => "N/N",
            },
        )),
        "performance.service_ceiling" => Ok(QuantityOutput::si(performance.service_ceiling_m, "m")),
        "performance.achieved_cruise_mach" => {
            optional_performance_metric(metric, performance.achieved_cruise_mach, "1")
        }
        "performance.achieved_cruise_true_airspeed" => optional_performance_metric(
            metric,
            performance.achieved_cruise_true_airspeed_m_s,
            "m/s",
        ),
        "performance.minimum_cruise_excess_power" => {
            optional_performance_metric(metric, performance.minimum_cruise_excess_power_w, "W")
        }
        "performance.cruise_feasible" => optional_performance_metric(
            metric,
            performance
                .cruise_feasible
                .map(|feasible| f64::from(u8::from(feasible))),
            "bool",
        ),
        "aerodynamics.maximum_lift_to_drag_ratio" => Ok(QuantityOutput::si(
            performance.maximum_lift_to_drag_ratio,
            "1",
        )),
        "mission.total_fuel" => Ok(QuantityOutput::si(mission.total_fuel_burn_kg, "kg")),
        "mission.landing_fuel" => mission
            .landing_fuel
            .clone()
            .ok_or_else(|| landing_fuel_unavailable(metric)),
        "mission.completed_distance" => Ok(QuantityOutput::range(mission.total_distance.value)),
        "feasibility.hard_constraints_passed" => {
            let requirements =
                evaluate_requirements(scenario, mission, performance, Some(payload_range));
            let passed = hard_requirements_passed(
                mission.completed,
                &scenario.requirements.items,
                &requirements,
            );
            Ok(QuantityOutput::si(f64::from(u8::from(passed)), "bool"))
        }
        "performance.full_payload_range" => {
            payload_range_value(payload_range, "full_payload_mission")
        }
        "performance.zero_payload_ferry_range" => {
            payload_range_value(payload_range, "zero_payload_ferry")
        }
        _ => Err(AexError::validation(
            "UNSUPPORTED_COMPARISON_METRIC",
            metric,
            "metric is not implemented",
        )),
    }
}

fn landing_fuel_unavailable(metric: &str) -> AexError {
    AexError::analysis(
        "MISSION_LANDING_FUEL_UNAVAILABLE",
        format!("{metric} is available only for a completed mission"),
    )
}

fn optional_performance_metric(
    metric: &str,
    value: Option<f64>,
    unit: &str,
) -> AexResult<QuantityOutput> {
    value
        .map(|value| QuantityOutput::si(value, unit))
        .ok_or_else(|| {
            AexError::analysis(
                "CRUISE_PERFORMANCE_UNAVAILABLE",
                format!("{metric} requires at least one cruise segment"),
            )
        })
}

fn payload_range_value(
    result: &crate::domain::result::PayloadRangeResult,
    point_id: &str,
) -> AexResult<QuantityOutput> {
    result
        .points
        .iter()
        .find(|point| point.id == point_id)
        .map(|point| point.range.clone())
        .ok_or_else(|| {
            AexError::analysis(
                "PAYLOAD_RANGE_POINT_MISSING",
                format!("payload-range result does not contain {point_id}"),
            )
        })
}

fn installed_loading(scenario: &crate::domain::schema::ResolvedScenario) -> f64 {
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    match &scenario.engine {
        EngineProfile::Piston(profile) => {
            profile.rated_power_w
                * f64::from(scenario.aircraft.propulsion.engine_count)
                * scenario.aircraft.propulsion.sizing_factor
                / mass
        }
        EngineProfile::Turbofan(profile) => {
            profile.installed_reference_thrust_n(
                scenario.aircraft.propulsion.engine_count,
                scenario.aircraft.propulsion.sizing_factor,
            ) / (mass * GRAVITY_M_S2)
        }
    }
}

fn comparison_provenance() -> ResultProvenance {
    ResultProvenance {
        method: "native conceptual metric comparison".to_owned(),
        backend: "native".to_owned(),
        assumptions: vec!["each design is evaluated independently".to_owned()],
        validity_range: vec!["designs using registered native models".to_owned()],
        validity_domains: Vec::new(),
        units: BTreeMap::from([("metrics".to_owned(), "result-specific SI".to_owned())]),
        warnings: vec![Diagnostic::limitation(
            "Comparison ranks conceptual estimates rather than certified performance.",
        )],
    }
}

#[cfg(test)]
mod tests;

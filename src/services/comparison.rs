use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::{GRAVITY_M_S2, QuantityOutput};
use crate::domain::result::ResultProvenance;
use crate::domain::schema::EngineProfile;
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
}

impl ApplicationService {
    pub(crate) fn compare_blocking(
        &self,
        paths: &[PathBuf],
        metrics: &[String],
    ) -> AexResult<ScenarioComparison> {
        let mut rows = Vec::new();
        let mut propulsion_kinds = Vec::new();
        for path in paths {
            let (scenario, performance) = self.performance_blocking(path, &BTreeMap::new())?;
            let (_, mission) = self.mission_blocking(path, &BTreeMap::new())?;
            let (_, payload_range) = self.payload_range_blocking(path, &BTreeMap::new())?;
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
            });
        }
        let mut warnings = Vec::new();
        if propulsion_kinds.windows(2).any(|pair| pair[0] != pair[1]) {
            warnings.push(Diagnostic::warning(
                "CROSS_CLASS_COMPARISON",
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
        "aerodynamics.maximum_lift_to_drag_ratio" => Ok(QuantityOutput::si(
            performance.maximum_lift_to_drag_ratio,
            "1",
        )),
        "mission.total_fuel" => Ok(QuantityOutput::si(mission.total_fuel_burn_kg, "kg")),
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
            profile.sea_level_static_thrust_n
                * f64::from(scenario.aircraft.propulsion.engine_count)
                * scenario.aircraft.propulsion.sizing_factor
                / (mass * GRAVITY_M_S2)
        }
    }
}

fn comparison_provenance() -> ResultProvenance {
    ResultProvenance {
        method: "native conceptual metric comparison".to_owned(),
        backend: "native".to_owned(),
        assumptions: vec!["each design is evaluated independently".to_owned()],
        validity_range: vec!["designs using registered native models".to_owned()],
        units: BTreeMap::from([("metrics".to_owned(), "result-specific SI".to_owned())]),
        warnings: vec![Diagnostic::limitation(
            "Comparison ranks conceptual estimates rather than certified performance.",
        )],
    }
}

#[cfg(test)]
mod tests;

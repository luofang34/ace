use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::{GRAVITY_M_S2, parse_quantity};
use crate::domain::result::{ResultProvenance, SweepResult, SweepRow};
use crate::domain::schema::{EngineProfile, Wing};
use crate::domain::validity::MetricValidity;
use crate::models::breguet::{self, BreguetEstimate};
use crate::models::field_performance::{estimate_landing_distance, estimate_takeoff_distance};
use crate::services::analysis::ApplicationService;
use crate::services::requirements::{evaluate_requirements, hard_requirements_passed};
use crate::services::resolver::complete_planform_overrides;

#[derive(Debug, Clone)]
pub(crate) struct SweepVariable {
    pub(crate) path: String,
    pub(crate) values: Vec<String>,
}

#[derive(Debug, Clone)]
struct SweepEvaluation {
    metrics: BTreeMap<String, f64>,
    feasible: bool,
    metric_validity: BTreeMap<String, MetricValidity>,
    warnings: Vec<Diagnostic>,
}

struct SweepMetricContext<'a> {
    scenario: &'a crate::domain::schema::ResolvedScenario,
    performance: &'a crate::domain::result::PerformanceSummary,
    mission: &'a crate::domain::result::MissionResult,
    payload_range: &'a crate::domain::result::PayloadRangeResult,
    requirements: &'a [crate::domain::result::RequirementEvaluation],
    breguet: BreguetEstimate,
}

impl SweepVariable {
    pub(crate) fn linear(
        path: String,
        start: &str,
        stop: &str,
        count: u32,
        logarithmic: bool,
    ) -> AexResult<Self> {
        if count < 2 {
            return Err(AexError::validation(
                "INVALID_SWEEP_COUNT",
                &path,
                "sweep count must be at least two",
            ));
        }
        let (start_value, start_unit) = split_value_unit(start, &path)?;
        let (stop_value, stop_unit) = split_value_unit(stop, &path)?;
        if start_unit != stop_unit {
            return Err(AexError::validation(
                "INCOMPATIBLE_UNITS",
                &path,
                format!("{start_unit} and {stop_unit} do not match"),
            ));
        }
        let values = (0..count)
            .map(|index| {
                let fraction = f64::from(index) / f64::from(count - 1);
                let value = if logarithmic {
                    start_value * (stop_value / start_value).powf(fraction)
                } else {
                    start_value + fraction * (stop_value - start_value)
                };
                format_sweep_value(value, &start_unit, &path)
            })
            .collect();
        Ok(Self { path, values })
    }
}

impl ApplicationService {
    pub(crate) fn sweep_blocking(
        &self,
        scenario_path: &Path,
        variables: &[SweepVariable],
        metrics: &[String],
    ) -> AexResult<SweepResult> {
        if variables.is_empty() || variables.len() > 2 {
            return Err(AexError::validation(
                "INVALID_SWEEP_DIMENSIONS",
                "variables",
                "one or two sweep variables are required",
            ));
        }
        let scenario = self.resolve_blocking(scenario_path, &BTreeMap::new())?;
        let combinations = combinations(variables);
        let cache: Arc<Mutex<BTreeMap<String, SweepEvaluation>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let rows = combinations
            .par_iter()
            .map(|overrides| {
                self.sweep_row(
                    scenario_path,
                    overrides,
                    metrics,
                    &scenario.aircraft.wing,
                    &cache,
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<AexResult<Vec<_>>>()?;
        let warnings = unique_row_warnings(&rows);
        Ok(SweepResult {
            scenario_id: scenario.id,
            rows,
            deterministic_ordering: true,
            warnings,
            provenance: sweep_provenance(),
        })
    }

    fn sweep_row(
        &self,
        scenario_path: &Path,
        overrides: &BTreeMap<String, String>,
        metrics: &[String],
        baseline_wing: &Wing,
        cache: &Arc<Mutex<BTreeMap<String, SweepEvaluation>>>,
    ) -> AexResult<SweepRow> {
        let evaluation_overrides = complete_planform_overrides(overrides, baseline_wing)?;
        let key = serde_json::to_string(&evaluation_overrides)
            .map_err(|source| AexError::Json { source })?;
        if let Some(cached) = cache
            .lock()
            .map_err(|source| AexError::analysis("SWEEP_CACHE_POISONED", source.to_string()))?
            .get(&key)
            .cloned()
        {
            return Ok(row(overrides, cached));
        }
        let (_, performance) = self.performance_blocking(scenario_path, &evaluation_overrides)?;
        let (scenario, mission) = self.mission_blocking(scenario_path, &evaluation_overrides)?;
        let (_, payload_range) =
            self.payload_range_blocking(scenario_path, &evaluation_overrides)?;
        let requirements =
            evaluate_requirements(&scenario, &mission, &performance, Some(&payload_range))?;
        let feasible = hard_requirements_passed(
            mission.completed,
            &scenario.requirements.items,
            &requirements,
        );
        let evaluation = SweepEvaluation {
            metrics: metric_values(
                &scenario,
                &performance,
                &mission,
                &payload_range,
                &requirements,
                metrics,
            )?,
            feasible,
            metric_validity: metrics
                .iter()
                .filter_map(|metric| {
                    performance
                        .metric_validity
                        .get(metric)
                        .map(|validity| (metric.clone(), validity.clone()))
                })
                .collect(),
            warnings: analysis_warnings(&performance, &mission, &payload_range),
        };
        cache
            .lock()
            .map_err(|source| AexError::analysis("SWEEP_CACHE_POISONED", source.to_string()))?
            .insert(key, evaluation.clone());
        Ok(row(overrides, evaluation))
    }
}

fn metric_values(
    scenario: &crate::domain::schema::ResolvedScenario,
    performance: &crate::domain::result::PerformanceSummary,
    mission: &crate::domain::result::MissionResult,
    payload_range: &crate::domain::result::PayloadRangeResult,
    requirements: &[crate::domain::result::RequirementEvaluation],
    metrics: &[String],
) -> AexResult<BTreeMap<String, f64>> {
    let breguet = breguet::estimate(
        scenario,
        performance.maximum_lift_to_drag_ratio,
        mission.total_fuel_burn_kg,
    )?;
    let context = SweepMetricContext {
        scenario,
        performance,
        mission,
        payload_range,
        requirements,
        breguet,
    };
    metrics
        .iter()
        .map(|metric| context.value(metric).map(|value| (metric.clone(), value)))
        .collect()
}

impl SweepMetricContext<'_> {
    fn value(&self, metric: &str) -> AexResult<f64> {
        let value = match metric {
            "performance.stall_speed" => self.performance.stall_speed_clean_m_s,
            "performance.stall_speed_landing" => self.performance.stall_speed_landing_m_s,
            "performance.service_ceiling" => self.performance.service_ceiling_m,
            "performance.maximum_level_speed" => self.performance.maximum_level_speed_m_s,
            "performance.achieved_cruise_mach" => {
                optional_performance_metric(metric, self.performance.achieved_cruise_mach)?
            }
            "performance.achieved_cruise_true_airspeed" => optional_performance_metric(
                metric,
                self.performance.achieved_cruise_true_airspeed_m_s,
            )?,
            "performance.minimum_cruise_excess_power" => {
                optional_performance_metric(metric, self.performance.minimum_cruise_excess_power_w)?
            }
            "performance.cruise_feasible" => optional_performance_metric(
                metric,
                self.performance
                    .cruise_feasible
                    .map(|feasible| f64::from(u8::from(feasible))),
            )?,
            "performance.takeoff_field_length" => {
                estimate_takeoff_distance(self.scenario)?.distance_m
            }
            "performance.landing_field_length" => {
                estimate_landing_distance(self.scenario)?.distance_m
            }
            "aerodynamics.maximum_lift_to_drag_ratio" => {
                self.performance.maximum_lift_to_drag_ratio
            }
            "geometry.aspect_ratio" => self.scenario.aircraft.wing.aspect_ratio,
            "geometry.wing_area" => self.scenario.aircraft.wing.area_m2,
            "geometry.wing_span" => self.scenario.aircraft.wing.span_m,
            "performance.wing_loading" => {
                self.scenario.aircraft.mass.maximum_takeoff_mass_kg * GRAVITY_M_S2
                    / self.scenario.aircraft.wing.area_m2
            }
            "performance.thrust_or_power_loading" => installed_loading(self.scenario),
            "mission.total_fuel" => self.mission.total_fuel_burn_kg,
            "mission.landing_fuel" => landing_fuel_metric(self.mission)?,
            "mission.completed_distance" | "mission.range" => self.mission.total_distance.value,
            "mission.breguet_range" => self.breguet.range_m,
            "mission.breguet_endurance" => self.breguet.endurance_s,
            "mission.payload_mass" => self.scenario.mission.payload_mass_kg,
            "performance.full_payload_range" => {
                payload_range_metric(self.payload_range, "full_payload_mission")?
            }
            "performance.zero_payload_ferry_range" => {
                payload_range_metric(self.payload_range, "zero_payload_ferry")?
            }
            "feasibility.hard_constraints_passed" => {
                let passed = hard_requirements_passed(
                    self.mission.completed,
                    &self.scenario.requirements.items,
                    self.requirements,
                );
                f64::from(u8::from(passed))
            }
            _ => {
                return Err(AexError::validation(
                    "UNSUPPORTED_SWEEP_METRIC",
                    metric,
                    "metric is not implemented",
                ));
            }
        };
        Ok(value)
    }
}

fn landing_fuel_metric(mission: &crate::domain::result::MissionResult) -> AexResult<f64> {
    mission
        .landing_fuel
        .as_ref()
        .map(|fuel| fuel.value)
        .ok_or_else(|| {
            AexError::analysis(
                "MISSION_LANDING_FUEL_UNAVAILABLE",
                "mission.landing_fuel is available only for a completed mission",
            )
        })
}

fn optional_performance_metric(metric: &str, value: Option<f64>) -> AexResult<f64> {
    value.ok_or_else(|| {
        AexError::analysis(
            "CRUISE_PERFORMANCE_UNAVAILABLE",
            format!("{metric} requires at least one cruise segment"),
        )
    })
}

fn payload_range_metric(
    payload_range: &crate::domain::result::PayloadRangeResult,
    point_id: &str,
) -> AexResult<f64> {
    payload_range
        .points
        .iter()
        .find(|point| point.id == point_id)
        .map(|point| point.range.value)
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

fn combinations(variables: &[SweepVariable]) -> Vec<BTreeMap<String, String>> {
    if variables.len() == 1 {
        return variables[0]
            .values
            .iter()
            .map(|value| BTreeMap::from([(variables[0].path.clone(), value.clone())]))
            .collect();
    }
    let mut results = Vec::new();
    for first in &variables[0].values {
        for second in &variables[1].values {
            results.push(BTreeMap::from([
                (variables[0].path.clone(), first.clone()),
                (variables[1].path.clone(), second.clone()),
            ]));
        }
    }
    results
}

fn row(overrides: &BTreeMap<String, String>, evaluation: SweepEvaluation) -> SweepRow {
    SweepRow {
        variables: overrides
            .iter()
            .map(|(path, value)| (path.clone(), serde_json::Value::String(value.clone())))
            .collect(),
        metrics: evaluation.metrics,
        feasible: evaluation.feasible,
        metric_validity: evaluation.metric_validity,
        warnings: evaluation.warnings,
    }
}

fn analysis_warnings(
    performance: &crate::domain::result::PerformanceSummary,
    mission: &crate::domain::result::MissionResult,
    payload_range: &crate::domain::result::PayloadRangeResult,
) -> Vec<Diagnostic> {
    let mut warnings = Vec::new();
    for warning in performance
        .warnings
        .iter()
        .chain(&mission.warnings)
        .chain(&payload_range.warnings)
    {
        if !warnings.contains(warning) {
            warnings.push(warning.clone());
        }
    }
    warnings
}

fn unique_row_warnings(rows: &[SweepRow]) -> Vec<Diagnostic> {
    let mut warnings = Vec::new();
    for warning in rows.iter().flat_map(|row| &row.warnings) {
        if !warnings.contains(warning) {
            warnings.push(warning.clone());
        }
    }
    warnings
}

fn split_value_unit(raw: &str, path: &str) -> AexResult<(f64, String)> {
    let Some(index) = raw.find(char::is_whitespace) else {
        if dimensionless_sweep_path(path) {
            return raw
                .parse::<f64>()
                .map(|value| (value, String::new()))
                .map_err(|source| {
                    AexError::validation("INVALID_SWEEP_VALUE", "sweep", source.to_string())
                });
        }
        return Err(AexError::validation(
            "AMBIGUOUS_UNITLESS_VALUE",
            "sweep",
            "physical sweep bounds require explicit units",
        ));
    };
    let value = raw[..index].parse::<f64>().map_err(|source| {
        AexError::validation("INVALID_SWEEP_VALUE", "sweep", source.to_string())
    })?;
    let unit = raw[index..].trim();
    if dimensionless_sweep_path(path) && unit != "1" {
        return Err(AexError::validation(
            "INCOMPATIBLE_UNITS",
            path,
            "aspect-ratio sweep bounds must be bare numbers or use unit 1",
        ));
    }
    let _validated = match unit {
        "kg" => parse_quantity(raw, crate::domain::quantity::Dimension::Mass)?,
        "m^2" | "ft^2" => parse_quantity(raw, crate::domain::quantity::Dimension::Area)?,
        "m" | "ft" | "nmi" => parse_quantity(raw, crate::domain::quantity::Dimension::Length)?,
        _ => value,
    };
    Ok((
        value,
        if dimensionless_sweep_path(path) {
            String::new()
        } else {
            unit.to_owned()
        },
    ))
}

fn format_sweep_value(value: f64, unit: &str, path: &str) -> String {
    if dimensionless_sweep_path(path) {
        format!("{value:.12}")
    } else {
        format!("{value:.12} {unit}")
    }
}

fn dimensionless_sweep_path(path: &str) -> bool {
    path == "aircraft.geometry.wing.aspect_ratio"
}

fn sweep_provenance() -> ResultProvenance {
    ResultProvenance {
        method: "deterministic Cartesian parameter sweep".to_owned(),
        backend: "native".to_owned(),
        assumptions: vec![
            "each row uses the resolved native conceptual models".to_owned(),
            "rows are independent and retain stable input ordering".to_owned(),
        ],
        validity_range: vec!["one or two canonical design parameters".to_owned()],
        validity_domains: Vec::new(),
        units: BTreeMap::from([
            ("physical_metrics".to_owned(), "SI".to_owned()),
            ("feasibility_flags".to_owned(), "1".to_owned()),
        ]),
        warnings: vec![crate::domain::diagnostic::Diagnostic::limitation(
            "Sweep precision does not increase the fidelity of the underlying models.",
        )],
    }
}

#[cfg(test)]
mod tests;

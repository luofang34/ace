use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{GRAVITY_M_S2, parse_quantity};
use crate::domain::result::{SweepResult, SweepRow};
use crate::domain::schema::EngineProfile;
use crate::services::analysis::ApplicationService;

#[derive(Debug, Clone)]
pub(crate) struct SweepVariable {
    pub(crate) path: String,
    pub(crate) values: Vec<String>,
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
        let (start_value, start_unit) = split_value_unit(start)?;
        let (stop_value, stop_unit) = split_value_unit(stop)?;
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
                format!("{value:.12} {start_unit}")
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
        let combinations = combinations(variables);
        let cache: Arc<Mutex<BTreeMap<String, BTreeMap<String, f64>>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let rows = combinations
            .par_iter()
            .map(|overrides| self.sweep_row(scenario_path, overrides, metrics, &cache))
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<AexResult<Vec<_>>>()?;
        let scenario = self.resolve_blocking(scenario_path, &BTreeMap::new())?;
        Ok(SweepResult {
            scenario_id: scenario.id,
            rows,
            deterministic_ordering: true,
            warnings: Vec::new(),
        })
    }

    fn sweep_row(
        &self,
        scenario_path: &Path,
        overrides: &BTreeMap<String, String>,
        metrics: &[String],
        cache: &Arc<Mutex<BTreeMap<String, BTreeMap<String, f64>>>>,
    ) -> AexResult<SweepRow> {
        let key = serde_json::to_string(overrides).map_err(|source| AexError::Json { source })?;
        if let Some(cached) = cache
            .lock()
            .map_err(|source| AexError::analysis("SWEEP_CACHE_POISONED", source.to_string()))?
            .get(&key)
            .cloned()
        {
            return Ok(row(overrides, cached));
        }
        let (_, performance) = self.performance_blocking(scenario_path, overrides)?;
        let (scenario, mission) = self.mission_blocking(scenario_path, overrides)?;
        let resolved = metric_values(&scenario, &performance, &mission, metrics)?;
        cache
            .lock()
            .map_err(|source| AexError::analysis("SWEEP_CACHE_POISONED", source.to_string()))?
            .insert(key, resolved.clone());
        Ok(row(overrides, resolved))
    }
}

fn metric_values(
    scenario: &crate::domain::schema::ResolvedScenario,
    performance: &crate::domain::result::PerformanceSummary,
    mission: &crate::domain::result::MissionResult,
    metrics: &[String],
) -> AexResult<BTreeMap<String, f64>> {
    metrics
        .iter()
        .map(|metric| {
            let value = match metric.as_str() {
                "performance.stall_speed" => performance.stall_speed_clean_m_s,
                "performance.stall_speed_landing" => performance.stall_speed_landing_m_s,
                "performance.service_ceiling" => performance.service_ceiling_m,
                "performance.maximum_level_speed" => performance.maximum_level_speed_m_s,
                "performance.wing_loading" => {
                    scenario.aircraft.mass.maximum_takeoff_mass_kg * GRAVITY_M_S2
                        / scenario.aircraft.wing.area_m2
                }
                "performance.thrust_or_power_loading" => installed_loading(scenario),
                "mission.total_fuel" => mission.total_fuel_burn_kg,
                "mission.completed_distance" | "mission.range" => mission.total_distance.value,
                _ => {
                    return Err(AexError::validation(
                        "UNSUPPORTED_SWEEP_METRIC",
                        metric,
                        "metric is not implemented",
                    ));
                }
            };
            Ok((metric.clone(), value))
        })
        .collect()
}

fn installed_loading(scenario: &crate::domain::schema::ResolvedScenario) -> f64 {
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    match &scenario.engine {
        EngineProfile::Piston(profile) => {
            profile.rated_power_w * f64::from(scenario.aircraft.propulsion.engine_count) / mass
        }
        EngineProfile::Turbofan(profile) => {
            profile.sea_level_static_thrust_n * f64::from(scenario.aircraft.propulsion.engine_count)
                / (mass * GRAVITY_M_S2)
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

fn row(overrides: &BTreeMap<String, String>, metrics: BTreeMap<String, f64>) -> SweepRow {
    SweepRow {
        variables: overrides
            .iter()
            .map(|(path, value)| (path.clone(), serde_json::Value::String(value.clone())))
            .collect(),
        metrics,
        warnings: Vec::new(),
    }
}

fn split_value_unit(raw: &str) -> AexResult<(f64, String)> {
    let index = raw.find(char::is_whitespace).ok_or_else(|| {
        AexError::validation(
            "AMBIGUOUS_UNITLESS_VALUE",
            "sweep",
            "sweep bounds require explicit units",
        )
    })?;
    let value = raw[..index].parse::<f64>().map_err(|source| {
        AexError::validation("INVALID_SWEEP_VALUE", "sweep", source.to_string())
    })?;
    let unit = raw[index..].trim();
    let _validated = match unit {
        "kg" => parse_quantity(raw, crate::domain::quantity::Dimension::Mass)?,
        "m^2" | "ft^2" => parse_quantity(raw, crate::domain::quantity::Dimension::Area)?,
        "m" | "ft" | "nmi" => parse_quantity(raw, crate::domain::quantity::Dimension::Length)?,
        _ => value,
    };
    Ok((value, unit.to_owned()))
}

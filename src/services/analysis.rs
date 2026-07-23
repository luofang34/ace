use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::result::{
    ConstraintResult, MissionResult, PayloadRangeResult, PerformanceSummary, PointPerformanceResult,
};
use crate::domain::schema::{RawProfile, ResolvedScenario};
use crate::models::atmosphere::Isa1976;
use crate::models::constraints::ConstraintAnalyzer;
use crate::models::mission::MissionSimulator;
use crate::models::payload_range::PayloadRangeAnalyzer;
use crate::models::performance::PointAnalyzer;
use crate::services::resolver::ScenarioResolver;
use crate::storage::profile_store::{FileProfileStore, ProfileRepository};
use crate::storage::project_store::read_yaml_value_blocking;
use crate::storage::run_store::{FileRunStore, PersistRunRequest, RunRecord, RunRepository};

#[derive(Debug, Clone)]
pub(crate) struct PointCondition {
    pub(crate) altitude_m: f64,
    pub(crate) speed_m_s: Option<f64>,
    pub(crate) mach: Option<f64>,
    pub(crate) mass_kg: Option<f64>,
    pub(crate) configuration: String,
}

#[derive(Clone)]
pub(crate) struct ApplicationService {
    resolver: ScenarioResolver,
    profiles: Arc<dyn ProfileRepository>,
    runs: Arc<dyn RunRepository>,
}

impl ApplicationService {
    pub(crate) fn filesystem(run_root: PathBuf) -> Self {
        let profiles: Arc<dyn ProfileRepository> = Arc::new(FileProfileStore);
        let runs: Arc<dyn RunRepository> = Arc::new(FileRunStore::new(run_root));
        Self::new(profiles, runs)
    }

    pub(crate) fn new(profiles: Arc<dyn ProfileRepository>, runs: Arc<dyn RunRepository>) -> Self {
        Self {
            resolver: ScenarioResolver::new(Arc::clone(&profiles)),
            profiles,
            runs,
        }
    }

    pub(crate) fn resolve_blocking(
        &self,
        path: &Path,
        overrides: &BTreeMap<String, String>,
    ) -> AexResult<ResolvedScenario> {
        self.resolver.resolve_blocking(path, overrides)
    }

    pub(crate) fn point_blocking(
        &self,
        path: &Path,
        overrides: &BTreeMap<String, String>,
        condition: PointCondition,
    ) -> AexResult<(ResolvedScenario, PointPerformanceResult)> {
        let scenario = self.resolve_blocking(path, overrides)?;
        let atmosphere = Isa1976::new(0.0).evaluate(condition.altitude_m)?;
        let speed = condition
            .speed_m_s
            .or_else(|| {
                condition
                    .mach
                    .map(|mach| mach * atmosphere.speed_of_sound_m_s)
            })
            .ok_or_else(|| {
                AexError::validation(
                    "MISSING_AIRSPEED",
                    "condition",
                    "true airspeed or Mach is required",
                )
            })?;
        let mass = condition
            .mass_kg
            .unwrap_or(scenario.aircraft.mass.maximum_takeoff_mass_kg);
        let analyzer = PointAnalyzer::new(scenario.clone());
        let result = analyzer.point(condition.altitude_m, speed, mass, &condition.configuration)?;
        Ok((scenario, result))
    }

    pub(crate) fn performance_blocking(
        &self,
        path: &Path,
        overrides: &BTreeMap<String, String>,
    ) -> AexResult<(ResolvedScenario, PerformanceSummary)> {
        let scenario = self.resolve_blocking(path, overrides)?;
        let result = PointAnalyzer::new(scenario.clone()).summary()?;
        Ok((scenario, result))
    }

    pub(crate) fn mission_blocking(
        &self,
        path: &Path,
        overrides: &BTreeMap<String, String>,
    ) -> AexResult<(ResolvedScenario, MissionResult)> {
        let scenario = self.resolve_blocking(path, overrides)?;
        let result = MissionSimulator::new(scenario.clone()).simulate()?;
        Ok((scenario, result))
    }

    pub(crate) fn constraints_blocking(
        &self,
        path: &Path,
        overrides: &BTreeMap<String, String>,
        start_n_m2: f64,
        stop_n_m2: f64,
        count: u32,
    ) -> AexResult<(ResolvedScenario, ConstraintResult)> {
        let scenario = self.resolve_blocking(path, overrides)?;
        let result =
            ConstraintAnalyzer::new(scenario.clone()).analyze(start_n_m2, stop_n_m2, count)?;
        Ok((scenario, result))
    }

    pub(crate) fn payload_range_blocking(
        &self,
        path: &Path,
        overrides: &BTreeMap<String, String>,
    ) -> AexResult<(ResolvedScenario, PayloadRangeResult)> {
        let scenario = self.resolve_blocking(path, overrides)?;
        let result = PayloadRangeAnalyzer::new(scenario.clone()).analyze()?;
        Ok((scenario, result))
    }

    pub(crate) fn list_profiles_blocking(
        &self,
        directory: &Path,
        profile_type: Option<&str>,
        query: Option<&str>,
    ) -> AexResult<Vec<RawProfile>> {
        let mut profiles = self.profiles.list_profiles_blocking(directory)?;
        profiles.retain(|profile| {
            profile_type.is_none_or(|kind| profile.kind == kind)
                && query.is_none_or(|needle| {
                    profile.id.to_lowercase().contains(&needle.to_lowercase())
                        || profile.metadata.as_ref().is_some_and(|metadata| {
                            metadata
                                .display_name
                                .to_lowercase()
                                .contains(&needle.to_lowercase())
                        })
                })
        });
        Ok(profiles)
    }

    pub(crate) fn get_profile_blocking(
        &self,
        directory: &Path,
        profile_id: &str,
    ) -> AexResult<RawProfile> {
        self.profiles.load_profile_blocking(directory, profile_id)
    }

    pub(crate) fn persist_blocking<T: Serialize>(
        &self,
        scenario: &ResolvedScenario,
        analysis: &str,
        result: &T,
        warnings: &[Diagnostic],
        seed: u64,
        artifacts: &[String],
    ) -> AexResult<RunRecord> {
        let result_value =
            serde_json::to_value(result).map_err(|source| AexError::Json { source })?;
        let input = read_yaml_value_blocking(&scenario.source_path)?;
        let input_value =
            serde_json::to_value(input).map_err(|source| AexError::Json { source })?;
        self.runs.persist_blocking(PersistRunRequest {
            scenario,
            analysis,
            original_input: &input_value,
            result: &result_value,
            warnings,
            seed,
            artifacts,
        })
    }

    pub(crate) fn load_run_blocking(&self, run_id: &str) -> AexResult<(Value, Value)> {
        let (manifest, result) = self.runs.load_result_blocking(run_id)?;
        let manifest_value =
            serde_json::to_value(manifest).map_err(|source| AexError::Json { source })?;
        Ok((manifest_value, result))
    }
}

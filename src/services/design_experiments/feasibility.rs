//! Scenario feasibility composed from topology preflight and backend analyses, the
//! harness the backend tests exercise.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{UnsupportedTopologyResult, unknown_backend};
use crate::backends::contracts::{
    AnalysisBackend, AnalysisOutput, AnalysisRequest, GeometryBackend, GeometryOutput,
    GeometryRequest,
};
use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::services::analysis::ApplicationService;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BackendEvaluation {
    pub(crate) geometry: GeometryOutput,
    pub(crate) analysis: AnalysisOutput,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum FeasibilityResult {
    Completed(Box<CompletedFeasibility>),
    Unsupported(UnsupportedTopologyResult),
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompletedFeasibility {
    pub(crate) scenario_id: String,
    pub(crate) feasible: bool,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) baseline: BackendEvaluation,
    pub(crate) refinement: Option<BackendEvaluation>,
    pub(crate) warnings: Vec<Diagnostic>,
}

impl FeasibilityResult {
    pub(crate) fn completed(&self) -> AexResult<&CompletedFeasibility> {
        match self {
            Self::Completed(result) => Ok(result),
            Self::Unsupported(result) => Err(result.as_error()),
        }
    }
}

impl ApplicationService {
    pub(crate) fn evaluate_feasibility_blocking(
        &self,
        scenario_path: &Path,
        backend: &str,
        artifact_path: Option<&Path>,
    ) -> AexResult<FeasibilityResult> {
        let scenario = self.resolve_blocking(scenario_path, &BTreeMap::new())?;
        if let Some(result) = self.feasibility_preflight(&scenario, backend)? {
            return Ok(FeasibilityResult::Unsupported(result));
        }
        let native = self.backends.native();
        let geometry = native.generate_geometry_blocking(GeometryRequest {
            scenario: &scenario,
            artifact_path: None,
        })?;
        let analysis = native.analyze_blocking(AnalysisRequest {
            scenario: &scenario,
            geometry: &geometry,
        })?;
        let feasible = analysis.feasible.unwrap_or(false);
        let failed_constraints = analysis.failed_constraints.clone();
        let baseline = BackendEvaluation { geometry, analysis };
        let refinement = match backend {
            "native" => None,
            "openvsp" => {
                Some(self.openvsp_evaluation_blocking(scenario_path, &scenario, artifact_path)?)
            }
            _ => {
                return Err(unknown_backend(backend));
            }
        };
        let warnings = evaluation_warnings(&baseline, refinement.as_ref());
        Ok(FeasibilityResult::Completed(Box::new(
            CompletedFeasibility {
                scenario_id: scenario.id,
                feasible,
                failed_constraints,
                baseline,
                refinement,
                warnings,
            },
        )))
    }

    fn openvsp_evaluation_blocking(
        &self,
        scenario_path: &Path,
        scenario: &crate::domain::schema::ResolvedScenario,
        artifact_path: Option<&Path>,
    ) -> AexResult<BackendEvaluation> {
        let openvsp = self.backends.openvsp()?;
        let default_artifact = default_artifact_path(scenario_path, &scenario.id);
        let artifact = artifact_path.unwrap_or(&default_artifact);
        let geometry = openvsp.generate_geometry_blocking(GeometryRequest {
            scenario,
            artifact_path: Some(artifact),
        })?;
        let analysis = openvsp.analyze_blocking(AnalysisRequest {
            scenario,
            geometry: &geometry,
        })?;
        Ok(BackendEvaluation { geometry, analysis })
    }
}

fn default_artifact_path(scenario_path: &Path, scenario_id: &str) -> PathBuf {
    let safe_id: String = scenario_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    scenario_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("artifacts")
        .join(format!("{safe_id}.vsp3"))
}

fn evaluation_warnings(
    baseline: &BackendEvaluation,
    refinement: Option<&BackendEvaluation>,
) -> Vec<Diagnostic> {
    let mut warnings = baseline.geometry.provenance.warnings.clone();
    warnings.extend(baseline.analysis.provenance.warnings.clone());
    if let Some(result) = refinement {
        warnings.extend(result.geometry.provenance.warnings.clone());
        warnings.extend(result.analysis.provenance.warnings.clone());
    }
    warnings
}

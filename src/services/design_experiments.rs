use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backends::contracts::{
    AnalysisBackend, AnalysisOutput, AnalysisRequest, BackendDescriptor, GeometryBackend,
    GeometryOutput, GeometryRequest,
};
use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::services::analysis::ApplicationService;
use crate::storage::design_store::{
    CreateDesignSpec, DesignRecord, create_design_blocking, update_parameters_blocking,
};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BackendEvaluation {
    pub(crate) geometry: GeometryOutput,
    pub(crate) analysis: AnalysisOutput,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FeasibilityResult {
    pub(crate) scenario_id: String,
    pub(crate) feasible: bool,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) baseline: BackendEvaluation,
    pub(crate) refinement: Option<BackendEvaluation>,
    pub(crate) warnings: Vec<Diagnostic>,
}

impl ApplicationService {
    pub(crate) fn create_design_blocking(
        &self,
        design_id: &str,
        display_name: &str,
        design_root: &Path,
        baseline: Option<&str>,
        source_scenario: Option<&Path>,
        parameters: &BTreeMap<String, String>,
    ) -> AexResult<DesignRecord> {
        let source = match source_scenario {
            Some(path) => path.to_path_buf(),
            None => baseline_path(baseline.unwrap_or("c172"))?,
        };
        self.resolve_blocking(&source, parameters)?;
        let design = create_design_blocking(CreateDesignSpec {
            design_id,
            display_name,
            source_scenario: &source,
            design_root,
            parameters,
        })?;
        self.resolve_blocking(&design.scenario_path, &BTreeMap::new())?;
        Ok(design)
    }

    pub(crate) fn update_design_parameters_blocking(
        &self,
        scenario_path: &Path,
        updates: &BTreeMap<String, String>,
    ) -> AexResult<DesignRecord> {
        self.resolve_blocking(scenario_path, updates)?;
        update_parameters_blocking(scenario_path, updates)
    }

    pub(crate) fn evaluate_feasibility_blocking(
        &self,
        scenario_path: &Path,
        backend: &str,
        artifact_path: Option<&Path>,
    ) -> AexResult<FeasibilityResult> {
        let scenario = self.resolve_blocking(scenario_path, &BTreeMap::new())?;
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
                return Err(AexError::validation(
                    "UNKNOWN_ANALYSIS_BACKEND",
                    "backend",
                    format!("expected native or openvsp, got {backend}"),
                ));
            }
        };
        let warnings = evaluation_warnings(&baseline, refinement.as_ref());
        Ok(FeasibilityResult {
            scenario_id: scenario.id,
            feasible,
            failed_constraints,
            baseline,
            refinement,
            warnings,
        })
    }

    pub(crate) fn list_analysis_backends(&self) -> Vec<BackendDescriptor> {
        self.backends.descriptors()
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

fn baseline_path(baseline: &str) -> AexResult<PathBuf> {
    let directory = match baseline {
        "c172" | "light_aircraft" => "c172",
        "transport" | "b777" => "b777",
        _ => {
            return Err(AexError::validation(
                "UNKNOWN_DESIGN_BASELINE",
                "baseline",
                format!("expected c172, light_aircraft, transport, or b777; got {baseline}"),
            ));
        }
    };
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(directory)
        .join("scenario.yaml"))
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

#[cfg(test)]
mod tests;

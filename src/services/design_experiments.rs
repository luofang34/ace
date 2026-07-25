use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backends::contracts::{
    AnalysisBackend, AnalysisOutput, AnalysisRequest, BackendDescriptor, GeometryBackend,
    GeometryOutput, GeometryRequest,
};
use crate::backends::native::native_topology_violations;
use crate::backends::openvsp::openvsp_topology_violations;
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UnsupportedTopologyResult {
    pub(crate) scenario_id: String,
    pub(crate) backend: String,
    pub(crate) feasible: Option<bool>,
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) path: String,
    pub(crate) unsupported_component_kinds: Vec<String>,
    pub(crate) unsupported_relationship_kinds: Vec<String>,
    pub(crate) unsupported_features: Vec<String>,
}

impl UnsupportedTopologyResult {
    pub(crate) fn as_error(&self) -> AexError {
        AexError::validation(
            "UNSUPPORTED_BACKEND_TOPOLOGY",
            self.path.clone(),
            self.message.clone(),
        )
    }
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

    pub(crate) fn feasibility_preflight(
        &self,
        scenario: &crate::domain::schema::ResolvedScenario,
        backend: &str,
    ) -> AexResult<Option<UnsupportedTopologyResult>> {
        let requested_descriptor = self
            .backends
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id == backend)
            .ok_or_else(|| unknown_backend(backend))?;
        let native_descriptor = self.backends.native().descriptor();
        if let Some(result) = unsupported_topology(scenario, &native_descriptor) {
            return Ok(Some(result));
        }
        if let Some(result) = unsupported_topology(scenario, &requested_descriptor) {
            return Ok(Some(result));
        }
        if !requested_descriptor.available {
            return Err(AexError::BackendUnavailable {
                backend: requested_descriptor.id,
                reason: requested_descriptor
                    .unavailable_reason
                    .unwrap_or_else(|| "backend is unavailable".to_owned()),
            });
        }
        Ok(None)
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

fn unsupported_topology(
    scenario: &crate::domain::schema::ResolvedScenario,
    descriptor: &BackendDescriptor,
) -> Option<UnsupportedTopologyResult> {
    let topology = &scenario.aircraft.topology;
    let unsupported_component_kinds =
        topology.unsupported_component_kinds(&descriptor.topology.component_kinds);
    let mut accepted_relationship_kinds = descriptor.topology.relationship_kinds.clone();
    accepted_relationship_kinds.extend(descriptor.topology.delegated_relationship_kinds.clone());
    let unsupported_relationship_kinds =
        topology.unsupported_relationship_kinds(&accepted_relationship_kinds);
    let unsupported_features = match descriptor.id.as_str() {
        "native" => native_topology_violations(scenario),
        "openvsp" => openvsp_topology_violations(scenario),
        _ => Vec::new(),
    };
    if unsupported_component_kinds.is_empty()
        && unsupported_relationship_kinds.is_empty()
        && unsupported_features.is_empty()
    {
        return None;
    }
    let message = format!(
        "backend {} does not support component kinds {:?}, relationship kinds {:?}, or features {:?}",
        descriptor.id,
        unsupported_component_kinds,
        unsupported_relationship_kinds,
        unsupported_features
    );
    Some(UnsupportedTopologyResult {
        scenario_id: scenario.id.clone(),
        backend: descriptor.id.clone(),
        feasible: None,
        code: "UNSUPPORTED_BACKEND_TOPOLOGY".to_owned(),
        message,
        path: "aircraft.topology".to_owned(),
        unsupported_component_kinds,
        unsupported_relationship_kinds,
        unsupported_features,
    })
}

fn unknown_backend(backend: &str) -> AexError {
    AexError::validation(
        "UNKNOWN_ANALYSIS_BACKEND",
        "backend",
        format!("expected native or openvsp, got {backend}"),
    )
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

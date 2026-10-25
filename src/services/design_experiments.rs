use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backends::contracts::{BackendDescriptor, GeometryBackend};
use crate::backends::native::native_topology_violations;
use crate::backends::openvsp::openvsp_topology_violations;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::services::analysis::ApplicationService;
use crate::storage::design_store::{CreateDesignSpec, DesignRecord, create_design_blocking};

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

#[cfg(test)]
mod feasibility;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rmcp::ErrorData;
use rmcp::handler::server::wrapper::Json;
use serde_json::json;

use crate::services::analysis::ApplicationService;
use crate::services::refinement::RefinementSpec;

use super::mcp_error;
use super::output::{ObjectOutput, json_output_for_scenario};
use super::schema::{AutoRefineDesignRequest, EvaluateFeasibilityRequest, UpdateDesignRequest};

pub(super) fn update(
    service: &ApplicationService,
    request: UpdateDesignRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let scenario_path = Path::new(&request.scenario_path);
    let design = service
        .update_design_parameters_blocking(scenario_path, &request.updates)
        .map_err(mcp_error)?;
    let resolved = service
        .resolve_blocking(scenario_path, &BTreeMap::new())
        .map_err(mcp_error)?;
    json_output_for_scenario(
        json!({ "design": design, "resolved_design": resolved }),
        scenario_path,
        request.units.as_deref(),
    )
}

pub(super) fn evaluate(
    service: &ApplicationService,
    request: EvaluateFeasibilityRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let scenario_path = Path::new(&request.scenario_path);
    let artifact = request.artifact_path.as_deref().map(Path::new);
    let result = service
        .evaluate_feasibility_blocking(
            scenario_path,
            request.backend.as_deref().unwrap_or("native"),
            artifact,
        )
        .map_err(mcp_error)?;
    json_output_for_scenario(result, scenario_path, request.units.as_deref())
}

pub(super) fn refine(
    service: &ApplicationService,
    request: AutoRefineDesignRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let display_name = request
        .display_name
        .unwrap_or_else(|| request.output_design_id.clone());
    let design_root = request
        .design_root
        .map_or_else(|| PathBuf::from(".ace/designs"), PathBuf::from);
    let scenario_path = Path::new(&request.scenario_path);
    let artifact_path = request.artifact_path.as_deref().map(Path::new);
    let result = service
        .auto_refine_design_blocking(RefinementSpec {
            scenario_path,
            output_design_id: &request.output_design_id,
            display_name: &display_name,
            design_root: &design_root,
            backend: request.backend.as_deref().unwrap_or("native"),
            artifact_path,
            max_iterations: request.max_iterations.unwrap_or(12),
        })
        .map_err(mcp_error)?;
    json_output_for_scenario(result, scenario_path, request.units.as_deref())
}

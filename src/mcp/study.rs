use std::path::{Path, PathBuf};

use rmcp::ErrorData;
use rmcp::handler::server::wrapper::Json;
use serde_json::json;

use crate::charts::renderer::render_svg_blocking;
use crate::charts::spec::ChartSpec;
use crate::charts::study::trade_space;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::mcp::schema::{
    LoadStudyRequest, PromoteStudyCandidateRequest, QueryStudyRequest, RunStudyRequest,
};
use crate::mcp::{ObjectOutput, json_output, mcp_error};
use crate::services::analysis::ApplicationService;

pub(super) fn load(
    service: &ApplicationService,
    request: LoadStudyRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    service
        .load_study_blocking(Path::new(&request.study_path))
        .map_err(mcp_error)
        .and_then(json_output)
}

pub(super) fn run(
    service: &ApplicationService,
    request: RunStudyRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let result = service
        .run_study_blocking(Path::new(&request.study_path))
        .map_err(mcp_error)?;
    let chart = trade_space(&result);
    persist_chart_blocking(chart.as_ref(), request.artifact_path.as_deref()).map_err(mcp_error)?;
    json_output(json!({
        "result": result,
        "chart_spec": chart,
        "artifact_path": request.artifact_path,
    }))
}

fn persist_chart_blocking(chart: Option<&ChartSpec>, artifact_path: Option<&str>) -> AexResult<()> {
    match (chart, artifact_path) {
        (Some(spec), Some(path)) => render_svg_blocking(spec, Path::new(path)),
        (None, Some(_)) => Err(AexError::validation(
            "STUDY_CHART_UNAVAILABLE",
            "artifact_path",
            "trade-space charts require at least two reported objectives",
        )),
        _ => Ok(()),
    }
}

pub(super) fn query(
    service: &ApplicationService,
    request: QueryStudyRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    if let Some(candidate_id) = request.candidate_id {
        return service
            .get_study_evidence_blocking(
                &request.study_id,
                request.archive_id.as_deref(),
                &candidate_id,
            )
            .map_err(mcp_error)
            .and_then(json_output);
    }
    let result = service
        .query_study_blocking(
            &request.study_id,
            request.archive_id.as_deref(),
            request.limit.unwrap_or(10),
        )
        .map_err(mcp_error)?;
    let chart = trade_space(&result);
    json_output(json!({
        "result": result,
        "chart_spec": chart,
    }))
}

pub(super) fn promote(
    service: &ApplicationService,
    request: PromoteStudyCandidateRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let display_name = request
        .display_name
        .unwrap_or_else(|| request.design_id.clone());
    let design_root = request
        .design_root
        .map_or_else(|| PathBuf::from(".ace/designs"), PathBuf::from);
    let design = service
        .promote_study_candidate_blocking(
            Path::new(&request.study_path),
            &request.candidate_id,
            &request.design_id,
            &display_name,
            &design_root,
        )
        .map_err(mcp_error)?;
    json_output(json!({
        "design": design,
        "source_candidate_id": request.candidate_id,
        "next_actions": ["evaluate_feasibility", "generate_report"],
    }))
}

#[cfg(test)]
mod tests;

use super::output::{ObjectOutput, canonical_json_output};
use rmcp::handler::server::wrapper::Json;
use rmcp::{ErrorData, schemars};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct Request {
    vehicle_path: String,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

pub(super) fn run(request: Request) -> Result<Json<ObjectOutput>, ErrorData> {
    let result = crate::services::synthesis::run_blocking(
        Path::new(&request.vehicle_path),
        &request.overrides,
    )
    .map_err(super::domain_error::mcp_error)?;
    canonical_json_output(result)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct StudyRequest {
    vehicle_path: String,
    plan_path: String,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
    /// Output parameter to chart over the studied inputs.
    chart_metric: Option<String>,
    /// SVG path for the chart; requires `chart_metric`.
    artifact_path: Option<String>,
}

pub(super) fn study(request: StudyRequest) -> Result<Json<ObjectOutput>, ErrorData> {
    let result = crate::services::vehicle_study::run_blocking(
        Path::new(&request.vehicle_path),
        Path::new(&request.plan_path),
        &request.overrides,
    )
    .and_then(|archive| {
        crate::services::vehicle_study::chart_blocking(
            archive,
            request.chart_metric,
            request.artifact_path.map(Into::into),
        )
    })
    .map_err(super::domain_error::mcp_error)?;
    canonical_json_output(result)
}

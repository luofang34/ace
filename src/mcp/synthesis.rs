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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CompareRequest {
    vehicle_paths: Vec<String>,
    /// Parameter identifiers reported for every vehicle in its declared unit.
    metrics: Vec<String>,
}

pub(super) fn compare(request: CompareRequest) -> Result<Json<ObjectOutput>, ErrorData> {
    let paths: Vec<_> = request.vehicle_paths.iter().map(Into::into).collect();
    let result = crate::services::vehicle_comparison::compare_blocking(&paths, &request.metrics)
        .map_err(super::domain_error::mcp_error)?;
    canonical_json_output(result)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PayloadRangeRequest {
    vehicle_path: String,
    /// Route-distance parameter the mission flies.
    distance: String,
    /// Fuel-remaining parameter that reaches zero at the range limit.
    remaining: String,
    maximum_takeoff_mass: String,
    maximum_payload: String,
    /// Write the payload-range chart as SVG to this path.
    artifact_path: Option<String>,
}

pub(super) fn payload_range(request: PayloadRangeRequest) -> Result<Json<ObjectOutput>, ErrorData> {
    let result = crate::services::vehicle_payload_range::compute_blocking(
        request.vehicle_path.as_ref(),
        &crate::services::vehicle_payload_range::PayloadRangeRoles {
            distance: &request.distance,
            remaining: &request.remaining,
            maximum_takeoff_mass: &request.maximum_takeoff_mass,
            maximum_payload: &request.maximum_payload,
        },
    )
    .and_then(|result| {
        crate::services::vehicle_chart::payload_range_blocking(
            result,
            request.artifact_path.as_deref().map(std::path::Path::new),
        )
    })
    .map_err(super::domain_error::mcp_error)?;
    canonical_json_output(result)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ChartRequest {
    vehicle_path: String,
    /// Analysis identifier declared by the vehicle.
    analysis: String,
    /// Write the chart as SVG to this path.
    artifact_path: Option<String>,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

pub(super) fn chart(request: ChartRequest) -> Result<Json<ObjectOutput>, ErrorData> {
    let result = crate::services::vehicle_chart::analysis_blocking(
        request.vehicle_path.as_ref(),
        &request.overrides,
        &request.analysis,
        request.artifact_path.as_deref().map(std::path::Path::new),
    )
    .map_err(super::domain_error::mcp_error)?;
    canonical_json_output(result)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ExplainRequest {
    vehicle_path: String,
    /// Canonical parameter identifier.
    parameter: String,
    /// Producer levels to walk upstream, 0 through 16; defaults to 2.
    depth: Option<u32>,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

pub(super) fn explain(request: ExplainRequest) -> Result<Json<ObjectOutput>, ErrorData> {
    let result = crate::services::vehicle_explanation::explain_blocking(
        Path::new(&request.vehicle_path),
        &request.overrides,
        &request.parameter,
        request.depth.unwrap_or(2),
    )
    .map_err(super::domain_error::mcp_error)?;
    canonical_json_output(result)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct EditRequest {
    vehicle_path: String,
    /// Typed structural declarations, applied before `updates`.
    #[serde(default)]
    #[schemars(with = "Vec<serde_json::Value>")]
    edits: Vec<ace_model::ModelEdit>,
    /// Parameter assignments such as `{"wing.span": "37 m"}`.
    #[serde(default)]
    updates: BTreeMap<String, String>,
    /// Refuse the edit unless the file is at this model revision.
    expected_revision: Option<u64>,
    /// Write the edited vehicle to this new file instead of replacing the source.
    save_as: Option<String>,
}

pub(super) fn edit(request: EditRequest) -> Result<Json<ObjectOutput>, ErrorData> {
    let save_as = request.save_as.as_deref().map(Path::new);
    let result = crate::services::vehicle_edit::edit_blocking(
        Path::new(&request.vehicle_path),
        &crate::services::vehicle_edit::VehicleEdit {
            edits: &request.edits,
            updates: &request.updates,
            expected_revision: request.expected_revision,
            save_as,
        },
    )
    .map_err(super::domain_error::mcp_error)?;
    canonical_json_output(result)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PromoteRequest {
    /// Study archive JSON returned by `study_vehicle`.
    archive_path: String,
    candidate_id: String,
    /// New vehicle file; an existing file is never replaced.
    save_as: String,
}

pub(super) fn promote(request: PromoteRequest) -> Result<Json<ObjectOutput>, ErrorData> {
    let result = crate::services::vehicle_promotion::promote_blocking(
        Path::new(&request.archive_path),
        &request.candidate_id,
        Path::new(&request.save_as),
    )
    .map_err(super::domain_error::mcp_error)?;
    canonical_json_output(result)
}

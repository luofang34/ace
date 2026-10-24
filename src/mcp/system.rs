use super::AexMcpServer;
use super::domain_error::mcp_error;
use super::output::{ObjectOutput, canonical_json_output};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData, schemars, tool, tool_router};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct UncertaintyRequest {
    /// Vehicle synthesis model whose inputs declare uncertainty.
    vehicle_path: String,
    /// Uncertainty plan with sample count and seed.
    plan_path: String,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CalibrationRequest {
    /// Vehicle synthesis model whose predictions are tested.
    vehicle_path: String,
    /// Calibration case with measured observations and bounded inputs.
    case_path: String,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CampaignRequest {
    /// Campaign file whose fleets reference vehicle models.
    campaign_path: String,
}

#[tool_router(router = system_router, vis = "pub(super)")]
impl AexMcpServer {
    #[tool(
        description = "Propagate declared input uncertainty through a vehicle synthesis graph; returns requirement satisfaction probabilities with 95% intervals and tri-state verdicts"
    )]
    fn propagate_vehicle_uncertainty(
        &self,
        Parameters(request): Parameters<UncertaintyRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let record = crate::services::system::uncertainty_blocking(
            Path::new(&request.vehicle_path),
            Path::new(&request.plan_path),
            &request.overrides,
        )
        .map_err(mcp_error)?;
        canonical_json_output(record)
    }

    #[tool(
        description = "Fit bounded vehicle inputs to measured observations; returns prior and posterior residuals, consistency and calibrated updates with provenance"
    )]
    fn calibrate_vehicle(
        &self,
        Parameters(request): Parameters<CalibrationRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let record = crate::services::system::calibrate_blocking(
            Path::new(&request.vehicle_path),
            Path::new(&request.case_path),
            &request.overrides,
        )
        .map_err(mcp_error)?;
        canonical_json_output(record)
    }

    #[tool(
        description = "Compare fleets of synthesized vehicles on a persistent-presence campaign; returns success intervals, verdicts, costs and the most cost-effective passing fleet"
    )]
    fn evaluate_fleet_campaign(
        &self,
        Parameters(request): Parameters<CampaignRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let record = crate::services::system::campaign_blocking(Path::new(&request.campaign_path))
            .map_err(mcp_error)?;
        canonical_json_output(record)
    }
}

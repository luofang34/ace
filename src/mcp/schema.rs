use std::collections::BTreeMap;

use rmcp::schemars;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ValidateDocumentRequest {
    pub(super) document: Value,
    pub(super) document_type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ListProfilesRequest {
    pub(super) profile_type: Option<String>,
    pub(super) query: Option<String>,
    pub(super) directory: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct GetProfileRequest {
    pub(super) profile_id: String,
    pub(super) version: Option<u32>,
    pub(super) directory: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ScenarioRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    #[serde(default)]
    pub(super) strict: bool,
    #[serde(default)]
    pub(super) overrides: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct MissionRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    #[serde(default)]
    pub(super) strict: bool,
    #[serde(default)]
    pub(super) detail: bool,
    #[serde(default)]
    pub(super) overrides: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PointRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    #[serde(default)]
    pub(super) strict: bool,
    pub(super) condition: PointRequestCondition,
    #[serde(default)]
    pub(super) overrides: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PointRequestCondition {
    pub(super) altitude: String,
    pub(super) true_airspeed: Option<String>,
    pub(super) mach: Option<f64>,
    pub(super) mass: Option<String>,
    pub(super) configuration: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ConstraintRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    #[serde(default)]
    pub(super) strict: bool,
    pub(super) wing_loading: WingLoadingRequest,
    #[serde(default)]
    pub(super) overrides: BTreeMap<String, String>,
    pub(super) artifact_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct WingLoadingRequest {
    pub(super) start: String,
    pub(super) stop: String,
    pub(super) count: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct SweepRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    #[serde(default)]
    pub(super) strict: bool,
    pub(super) variables: Vec<SweepVariableRequest>,
    pub(super) metrics: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct SweepVariableRequest {
    pub(super) path: String,
    pub(super) start: Option<String>,
    pub(super) stop: Option<String>,
    pub(super) count: Option<u32>,
    pub(super) values: Option<Vec<String>>,
    pub(super) logarithmic: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CompareRequest {
    pub(super) scenario_paths: Vec<String>,
    pub(super) units: Option<String>,
    #[serde(default)]
    pub(super) strict: bool,
    pub(super) metrics: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ExplainRequest {
    pub(super) run_id: String,
    pub(super) result_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ReportRequest {
    pub(super) run_id: Option<String>,
    pub(super) scenario_path: Option<String>,
    pub(super) units: Option<String>,
    pub(super) backend: Option<String>,
    pub(super) format: String,
    #[serde(default)]
    pub(super) sections: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PayloadRangeRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    #[serde(default)]
    pub(super) strict: bool,
    #[serde(default)]
    pub(super) overrides: BTreeMap<String, String>,
    pub(super) artifact_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CreateDesignRequest {
    pub(super) design_id: String,
    pub(super) display_name: Option<String>,
    pub(super) design_root: Option<String>,
    pub(super) baseline: Option<String>,
    pub(super) source_scenario_path: Option<String>,
    #[serde(default)]
    pub(super) parameters: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct UpdateDesignRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    pub(super) updates: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct EvaluateFeasibilityRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    pub(super) backend: Option<String>,
    pub(super) artifact_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct AutoRefineDesignRequest {
    pub(super) scenario_path: String,
    pub(super) units: Option<String>,
    pub(super) output_design_id: String,
    pub(super) display_name: Option<String>,
    pub(super) design_root: Option<String>,
    pub(super) backend: Option<String>,
    pub(super) artifact_path: Option<String>,
    pub(super) max_iterations: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CompareDesignsRequest {
    pub(super) design_paths: Vec<String>,
    pub(super) units: Option<String>,
    #[serde(default)]
    pub(super) strict: bool,
    pub(super) metrics: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct LoadStudyRequest {
    pub(super) study_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct RunStudyRequest {
    pub(super) study_path: String,
    pub(super) artifact_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct QueryStudyRequest {
    pub(super) study_id: String,
    pub(super) archive_id: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) candidate_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PromoteStudyCandidateRequest {
    pub(super) study_path: String,
    pub(super) candidate_id: String,
    pub(super) design_id: String,
    pub(super) display_name: Option<String>,
    pub(super) design_root: Option<String>,
}

use std::path::Path;

use rmcp::ErrorData;
use rmcp::handler::server::wrapper::Json;
use serde_json::Value;

use crate::domain::warning::enforce_strict;
use crate::services::analysis::ApplicationService;
use crate::services::requirements::{evaluate_requirements, hard_requirements_passed};

use super::schema::ScenarioRequest;
use super::{ObjectOutput, json_output_for_scenario, mcp_error, mcp_serialization_error};

pub(super) fn simulate(
    service: &ApplicationService,
    request: ScenarioRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let path = Path::new(&request.scenario_path);
    let (scenario, mission) = service
        .mission_blocking(path, &request.overrides)
        .map_err(mcp_error)?;
    enforce_strict(request.strict, &mission.warnings).map_err(mcp_error)?;
    let (_, performance) = service
        .performance_blocking(path, &request.overrides)
        .map_err(mcp_error)?;
    let payload_range = if requires_payload_range(&scenario) {
        Some(
            service
                .payload_range_blocking(path, &request.overrides)
                .map_err(mcp_error)?
                .1,
        )
    } else {
        None
    };
    let requirements =
        evaluate_requirements(&scenario, &mission, &performance, payload_range.as_ref());
    let passed = hard_requirements_passed(
        mission.completed,
        &scenario.requirements.items,
        &requirements,
    );
    let mut response = serde_json::to_value(mission).map_err(mcp_serialization_error)?;
    let fields = response.as_object_mut().ok_or_else(|| {
        ErrorData::internal_error("mission result did not serialize as an object", None)
    })?;
    fields.insert("hard_requirements_passed".to_owned(), Value::Bool(passed));
    json_output_for_scenario(response, path, request.units.as_deref())
}

fn requires_payload_range(scenario: &crate::domain::schema::ResolvedScenario) -> bool {
    scenario.requirements.items.iter().any(|requirement| {
        requirement.severity == "hard"
            && matches!(
                requirement.metric.as_str(),
                "performance.full_payload_range" | "performance.zero_payload_ferry_range"
            )
    })
}

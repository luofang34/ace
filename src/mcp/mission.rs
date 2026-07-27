use std::path::Path;

use rmcp::ErrorData;
use rmcp::handler::server::wrapper::Json;
use serde_json::{Map, Value, json};

use crate::domain::presentation::DisplayUnitSystem;
use crate::domain::warning::enforce_strict;
use crate::models::mass_properties;
use crate::services::analysis::ApplicationService;
use crate::services::requirements::{evaluate_requirements, hard_requirements_passed};
use crate::storage::project_store::display_unit_system_blocking;

use super::output::json_output_with_system;
use super::schema::MissionRequest;
use super::{ObjectOutput, mcp_error, mcp_serialization_error};

pub(super) fn simulate(
    service: &ApplicationService,
    request: MissionRequest,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let path = Path::new(&request.scenario_path);
    let unit_system =
        display_unit_system_blocking(path, request.units.as_deref()).map_err(mcp_error)?;
    let (scenario, mission) = service
        .mission_blocking(path, &request.overrides)
        .map_err(mcp_error)?;
    enforce_strict(request.strict, &mission.warnings).map_err(mcp_error)?;
    let supporting = service
        .mission_supporting_analysis_blocking(&scenario, &mission)
        .map_err(mcp_error)?;
    let requirements = evaluate_requirements(
        &scenario,
        &mission,
        &supporting.performance,
        supporting.payload_range.as_ref(),
    )
    .map_err(mcp_error)?;
    let passed = hard_requirements_passed(
        mission.completed,
        &scenario.requirements.items,
        &requirements,
    );
    let mass_properties = mass_properties::evaluate(&scenario, &mission).map_err(mcp_error)?;
    let detail = detailed_result(&mission, &mass_properties, passed)?;
    let run = service
        .persist_blocking(&scenario, "mission", &detail, &mission.warnings, 0, &[])
        .map_err(mcp_error)?;
    let response = if request.detail {
        detailed_response(detail, &run.run_id, unit_system)?
    } else {
        compact_response(&mission, passed, &run.run_id, unit_system)
    };
    json_output_with_system(response, unit_system)
}

fn detailed_result(
    mission: &crate::domain::result::MissionResult,
    mass_properties: &crate::domain::result::MassPropertiesAnalysis,
    passed: bool,
) -> Result<Value, ErrorData> {
    let mut result = serde_json::to_value(mission).map_err(mcp_serialization_error)?;
    let fields = object_fields(&mut result)?;
    fields.insert("hard_requirements_passed".to_owned(), Value::Bool(passed));
    fields.insert(
        "mass_properties".to_owned(),
        serde_json::to_value(mass_properties).map_err(mcp_serialization_error)?,
    );
    Ok(result)
}

fn detailed_response(
    mut detail: Value,
    run_id: &str,
    units: DisplayUnitSystem,
) -> Result<Value, ErrorData> {
    let fields = object_fields(&mut detail)?;
    fields.insert("run_id".to_owned(), Value::String(run_id.to_owned()));
    fields.insert("retrieval".to_owned(), retrieval(run_id, units));
    Ok(detail)
}

fn compact_response(
    mission: &crate::domain::result::MissionResult,
    passed: bool,
    run_id: &str,
    units: DisplayUnitSystem,
) -> Value {
    let segments = mission
        .segments
        .iter()
        .map(|segment| {
            json!({
                "segment_id": segment.segment_id,
                "distance_m": segment.distance_m,
                "duration_s": segment.duration_s,
                "fuel_burn_kg": segment.fuel_burn_kg,
                "end_mass_kg": segment.end_mass_kg,
                "end_altitude_m": segment.end_altitude_m,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "run_id": run_id,
        "completion_status": if mission.completed { "completed" } else { "incomplete" },
        "completed": mission.completed,
        "hard_requirements_passed": passed,
        "totals": {
            "distance": mission.total_distance,
            "duration_s": mission.total_duration_s,
            "fuel_burn_kg": mission.total_fuel_burn_kg,
            "landing_fuel": mission.landing_fuel,
            "initial_takeoff_mass_kg": mission.initial_takeoff_mass_kg,
            "final_mass_kg": mission.final_mass_kg,
            "final_payload_mass_kg": mission.final_payload_mass_kg,
        },
        "segments": segments,
        "diagnostics": mission.warnings,
        "retrieval": retrieval(run_id, units),
    })
}

fn retrieval(run_id: &str, units: DisplayUnitSystem) -> Value {
    let units = match units {
        DisplayUnitSystem::Si => "si",
        DisplayUnitSystem::AviationUs => "aviation_us",
    };
    json!({
        "tool": "generate_report",
        "arguments": {
            "run_id": run_id,
            "format": "json",
            "sections": [],
            "units": units,
        },
        "result_path": "result",
    })
}

fn object_fields(value: &mut Value) -> Result<&mut Map<String, Value>, ErrorData> {
    value.as_object_mut().ok_or_else(|| {
        ErrorData::internal_error("mission result did not serialize as an object", None)
    })
}

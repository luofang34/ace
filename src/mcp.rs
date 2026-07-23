mod schema;
pub(crate) mod serialization;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{
    ErrorData, ServerHandler, ServiceExt, handler::server::router::tool::ToolRouter, schemars,
    tool, tool_handler, tool_router, transport::stdio,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::charts::generators::{constraints, payload_range, sweep};
use crate::charts::renderer::render_svg_blocking;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::services::analysis::{ApplicationService, PointCondition};
use crate::services::report::LIMITATION;
use crate::services::sweep::SweepVariable;
use crate::services::validator::{error_validation, validate_document_value};
use schema::{
    CompareRequest, ConstraintRequest, ExplainRequest, GetProfileRequest, ListProfilesRequest,
    PayloadRangeRequest, PointRequest, ReportRequest, ScenarioRequest, SweepRequest,
    SweepVariableRequest, ValidateDocumentRequest,
};

#[derive(Clone)]
pub(crate) struct AexMcpServer {
    service: ApplicationService,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ObjectOutput {
    #[serde(flatten)]
    fields: BTreeMap<String, Value>,
}

impl AexMcpServer {
    fn new(service: ApplicationService) -> Self {
        Self {
            service,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl AexMcpServer {
    #[tool(description = "Validate an aircraft, mission, requirements, or profile document")]
    fn validate_document(
        &self,
        Parameters(request): Parameters<ValidateDocumentRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let yaml = serde_yaml::to_value(&request.document).map_err(mcp_error)?;
        let result = match validate_document_value(&yaml, Some(&request.document_type)) {
            Ok(valid) => valid,
            Err(error) => error_validation(&request.document_type, &error),
        };
        json_output(result)
    }

    #[tool(description = "List data profiles with optional type and text filters")]
    fn list_profiles(
        &self,
        Parameters(request): Parameters<ListProfilesRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let directory = request.directory.unwrap_or_else(|| "profiles".to_owned());
        let profiles = self
            .service
            .list_profiles_blocking(
                Path::new(&directory),
                request.profile_type.as_deref(),
                request.query.as_deref(),
            )
            .map_err(mcp_error)?;
        let summaries: Vec<Value> = profiles
            .into_iter()
            .map(|profile| {
                json!({
                    "id": profile.id,
                    "version": profile.version,
                    "display_name": profile.metadata.map(|metadata| metadata.display_name),
                    "type": profile.kind,
                    "model": profile.model,
                })
            })
            .collect();
        json_output(json!({ "profiles": summaries }))
    }

    #[tool(description = "Get one versioned propulsion or propeller profile")]
    fn get_profile(
        &self,
        Parameters(request): Parameters<GetProfileRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let directory = request.directory.unwrap_or_else(|| "profiles".to_owned());
        let profile = self
            .service
            .get_profile_blocking(Path::new(&directory), &request.profile_id)
            .map_err(mcp_error)?;
        if request
            .version
            .is_some_and(|version| version != profile.version)
        {
            return Err(ErrorData::invalid_params(
                format!(
                    "profile {} has version {}, not the requested version",
                    profile.id, profile.version
                ),
                None,
            ));
        }
        json_output(profile)
    }

    #[tool(description = "Resolve a scenario, profiles, units, assumptions, and overrides")]
    fn resolve_scenario(
        &self,
        Parameters(request): Parameters<ScenarioRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let resolved = self
            .service
            .resolve_blocking(Path::new(&request.scenario_path), &request.overrides)
            .map_err(mcp_error)?;
        json_output(json!({
            "resolved_scenario": resolved,
            "assumptions": resolved.assumptions,
            "warnings": resolved.warnings,
        }))
    }

    #[tool(description = "Calculate deterministic point performance for a resolved scenario")]
    fn calculate_point_performance(
        &self,
        Parameters(request): Parameters<PointRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let altitude =
            parse_quantity(&request.condition.altitude, Dimension::Length).map_err(mcp_error)?;
        let speed = request
            .condition
            .true_airspeed
            .as_deref()
            .map(|value| parse_quantity(value, Dimension::Speed))
            .transpose()
            .map_err(mcp_error)?;
        let mass = request
            .condition
            .mass
            .as_deref()
            .map(|value| parse_quantity(value, Dimension::Mass))
            .transpose()
            .map_err(mcp_error)?;
        let (_, result) = self
            .service
            .point_blocking(
                Path::new(&request.scenario_path),
                &request.overrides,
                PointCondition {
                    altitude_m: altitude,
                    speed_m_s: speed,
                    mach: request.condition.mach,
                    mass_kg: mass,
                    configuration: request
                        .condition
                        .configuration
                        .unwrap_or_else(|| "clean".to_owned()),
                },
            )
            .map_err(mcp_error)?;
        json_output(result)
    }

    #[tool(description = "Simulate an ordered quasi-steady mission with mass continuity")]
    fn simulate_mission(
        &self,
        Parameters(request): Parameters<ScenarioRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let (_, result) = self
            .service
            .mission_blocking(Path::new(&request.scenario_path), &request.overrides)
            .map_err(mcp_error)?;
        json_output(result)
    }

    #[tool(description = "Generate structured constraint data and an optional SVG artifact")]
    fn generate_constraint_diagram(
        &self,
        Parameters(request): Parameters<ConstraintRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let start = parse_wing_loading(&request.wing_loading.start).map_err(mcp_error)?;
        let stop = parse_wing_loading(&request.wing_loading.stop).map_err(mcp_error)?;
        let (scenario, result) = self
            .service
            .constraints_blocking(
                Path::new(&request.scenario_path),
                &request.overrides,
                start,
                stop,
                request.wing_loading.count,
            )
            .map_err(mcp_error)?;
        let chart = constraints(&scenario, &result);
        if let Some(path) = &request.artifact_path {
            render_svg_blocking(&chart, Path::new(path)).map_err(mcp_error)?;
        }
        json_output(json!({
            "result": result,
            "chart_spec": chart,
            "artifact_path": request.artifact_path,
        }))
    }

    #[tool(description = "Run a deterministic one- or two-dimensional parameter sweep")]
    fn run_parameter_sweep(
        &self,
        Parameters(request): Parameters<SweepRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let variables = request
            .variables
            .into_iter()
            .map(sweep_variable)
            .collect::<AexResult<Vec<_>>>()
            .map_err(mcp_error)?;
        let result = self
            .service
            .sweep_blocking(
                Path::new(&request.scenario_path),
                &variables,
                &request.metrics,
            )
            .map_err(mcp_error)?;
        let chart = request
            .metrics
            .first()
            .map(|metric| sweep(&result, metric))
            .transpose()
            .map_err(mcp_error)?;
        json_output(json!({ "result": result, "chart_spec": chart }))
    }

    #[tool(description = "Compare selected metrics across aircraft scenarios")]
    fn compare_scenarios(
        &self,
        Parameters(request): Parameters<CompareRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let paths: Vec<PathBuf> = request
            .scenario_paths
            .into_iter()
            .map(PathBuf::from)
            .collect();
        let result = self
            .service
            .compare_blocking(&paths, &request.metrics)
            .map_err(mcp_error)?;
        json_output(result)
    }

    #[tool(description = "Explain a persisted result value and its governing low-fidelity model")]
    fn explain_result(
        &self,
        Parameters(request): Parameters<ExplainRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let (manifest, result) = self
            .service
            .load_run_blocking(&request.run_id)
            .map_err(mcp_error)?;
        let value = dotted_value(&result, &request.result_path);
        json_output(json!({
            "value": value,
            "units": infer_result_unit(&request.result_path),
            "governing_equations": governing_equations(&request.result_path),
            "source_models": manifest.get("models"),
            "input_dependencies": input_dependencies(&request.result_path),
            "active_assumptions": [],
            "warnings": [],
            "local_sensitivity": null,
        }))
    }

    #[tool(description = "Generate a JSON or Markdown report for an immutable analysis run")]
    fn generate_report(
        &self,
        Parameters(request): Parameters<ReportRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let (manifest, result) = self
            .service
            .load_run_blocking(&request.run_id)
            .map_err(mcp_error)?;
        if request.format == "json" {
            return json_output(json!({
                "format": "json",
                "sections": request.sections,
                "manifest": manifest,
                "result": result,
                "limitation": LIMITATION,
            }));
        }
        if request.format != "markdown" {
            return Err(ErrorData::invalid_params(
                "format must be markdown or json",
                None,
            ));
        }
        let markdown = format!(
            "# Analysis run {}\n\n{}\n\n## Summary\n\n```json\n{}\n```\n",
            request.run_id,
            LIMITATION,
            serde_json::to_string_pretty(&result).map_err(mcp_error)?
        );
        json_output(json!({
            "format": "markdown",
            "sections": request.sections,
            "content": markdown,
        }))
    }

    #[tool(description = "Generate payload-range data, chart specification, and optional SVG")]
    fn generate_payload_range(
        &self,
        Parameters(request): Parameters<PayloadRangeRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let (scenario, result) = self
            .service
            .payload_range_blocking(Path::new(&request.scenario_path), &request.overrides)
            .map_err(mcp_error)?;
        let chart = payload_range(&scenario, &result);
        if let Some(path) = &request.artifact_path {
            render_svg_blocking(&chart, Path::new(path)).map_err(mcp_error)?;
        }
        json_output(json!({
            "result": result,
            "chart_spec": chart,
            "artifact_path": request.artifact_path,
        }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AexMcpServer {}

pub(crate) async fn serve_stdio(service: ApplicationService) -> AexResult<()> {
    let server = AexMcpServer::new(service)
        .serve(stdio())
        .await
        .map_err(|source| AexError::analysis("MCP_SERVE_ERROR", source.to_string()))?;
    server
        .waiting()
        .await
        .map_err(|source| AexError::analysis("MCP_TRANSPORT_ERROR", source.to_string()))?;
    Ok(())
}

fn json_output<T: serde::Serialize>(value: T) -> Result<Json<ObjectOutput>, ErrorData> {
    let serialized = serde_json::to_value(value).map_err(mcp_error)?;
    let interface_value = serialization::attach_units(serialized);
    let fields = match interface_value {
        Value::Object(mapping) => mapping.into_iter().collect(),
        scalar => BTreeMap::from([("result".to_owned(), scalar)]),
    };
    Ok(Json(ObjectOutput { fields }))
}

fn mcp_error<E: std::fmt::Display>(source: E) -> ErrorData {
    ErrorData::internal_error(source.to_string(), None)
}

fn sweep_variable(request: SweepVariableRequest) -> AexResult<SweepVariable> {
    if let Some(values) = request.values {
        if values.is_empty() {
            return Err(AexError::validation(
                "EMPTY_SWEEP_VALUES",
                request.path,
                "explicit value list cannot be empty",
            ));
        }
        return Ok(SweepVariable {
            path: request.path,
            values,
        });
    }
    let start = request.start.ok_or_else(|| {
        AexError::validation("MISSING_SWEEP_START", &request.path, "start is required")
    })?;
    let stop = request.stop.ok_or_else(|| {
        AexError::validation("MISSING_SWEEP_STOP", &request.path, "stop is required")
    })?;
    SweepVariable::linear(
        request.path,
        &start,
        &stop,
        request.count.unwrap_or(25),
        request.logarithmic.unwrap_or(false),
    )
}

fn parse_wing_loading(raw: &str) -> AexResult<f64> {
    crate::cli::commands::parse_wing_loading(raw)
}

fn dotted_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(value, |current, part| current.get(part))
}

fn infer_result_unit(path: &str) -> &'static str {
    if path.contains("distance") || path.contains("range") {
        "nmi display, m internal"
    } else if path.contains("ceiling") || path.ends_with("_m") {
        "m"
    } else if path.contains("speed") {
        "m/s"
    } else if path.contains("fuel") || path.contains("mass") {
        "kg"
    } else {
        "dimensionless or result-specific"
    }
}

fn governing_equations(path: &str) -> Vec<&'static str> {
    if path.contains("ceiling") {
        vec![
            "ROC = excess power / weight",
            "bounded root: ROC - threshold = 0",
        ]
    } else if path.contains("range") || path.contains("distance") {
        vec![
            "distance = true airspeed × segment duration",
            "fuel flow from BSFC or TSFC",
        ]
    } else {
        vec!["CD = CD0 + k CL² + CDadditional + CDwave", "drag = q S CD"]
    }
}

fn input_dependencies(path: &str) -> Vec<&'static str> {
    if path.contains("ceiling") {
        vec![
            "aircraft mass",
            "wing geometry",
            "drag polar",
            "propulsion lapse",
            "ISA atmosphere",
        ]
    } else {
        vec!["resolved aircraft", "mission", "selected profiles"]
    }
}

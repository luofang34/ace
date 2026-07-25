mod capabilities;
mod design;
mod domain_error;
mod handler;
mod mission;
mod output;
mod parameters;
mod report;
mod schema;
pub(crate) mod serialization;
mod study;

use std::path::{Path, PathBuf};

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{
    ErrorData, ServiceExt, handler::server::router::tool::ToolRouter, tool, tool_router,
    transport::stdio,
};
use serde_json::{Value, json};

use crate::charts::generators::{constraints, payload_range, sweep};
use crate::charts::renderer::render_svg_blocking;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::warning::enforce_strict;
use crate::services::analysis::{ApplicationService, PointCondition};
use crate::services::validator::{error_validation, validate_document_value};
use domain_error::mcp_error;
use output::{ObjectOutput, json_output, json_output_for_scenario};
use parameters::{
    dotted_value, governing_equations, infer_result_unit, input_dependencies, parse_wing_loading,
    sweep_variable,
};
use schema::{
    AutoRefineDesignRequest, CompareDesignsRequest, CompareRequest, ConstraintRequest,
    CreateDesignRequest, EvaluateFeasibilityRequest, ExplainRequest, GetProfileRequest,
    ListProfilesRequest, LoadStudyRequest, PayloadRangeRequest, PointRequest,
    PromoteStudyCandidateRequest, QueryStudyRequest, ReportRequest, RunStudyRequest,
    ScenarioRequest, SweepRequest, UpdateDesignRequest, ValidateDocumentRequest,
};

#[derive(Clone)]
pub(crate) struct AexMcpServer {
    service: ApplicationService,
    tool_router: ToolRouter<Self>,
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
    #[tool(description = "Discover accepted vocabulary, models, warnings, and backends")]
    fn get_capabilities(&self) -> Result<Json<ObjectOutput>, ErrorData> {
        capabilities::get(&self.service)
    }
    #[tool(description = "Load and validate a portable aircraft design study")]
    fn load_design_study(
        &self,
        Parameters(request): Parameters<LoadStudyRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        study::load(&self.service, request)
    }

    #[tool(description = "Run or resume a descriptor-based multi-objective design study")]
    fn run_design_study(
        &self,
        Parameters(request): Parameters<RunStudyRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        study::run(&self.service, request)
    }

    #[tool(description = "Query bounded Pareto and selected-candidate study evidence")]
    fn query_design_study(
        &self,
        Parameters(request): Parameters<QueryStudyRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        study::query(&self.service, request)
    }

    #[tool(description = "Promote a feasible study candidate into an editable design")]
    fn promote_study_candidate(
        &self,
        Parameters(request): Parameters<PromoteStudyCandidateRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        study::promote(&self.service, request)
    }

    #[tool(description = "Validate a supported portable document")]
    fn validate_document(
        &self,
        Parameters(request): Parameters<ValidateDocumentRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let yaml = serde_yaml::to_value(&request.document).map_err(mcp_serialization_error)?;
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

    #[tool(description = "Create an editable design from a C172, transport, or scenario baseline")]
    fn create_design(
        &self,
        Parameters(request): Parameters<CreateDesignRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let display_name = request
            .display_name
            .unwrap_or_else(|| request.design_id.clone());
        let design_root = request
            .design_root
            .map_or_else(|| PathBuf::from(".ace/designs"), PathBuf::from);
        let source = request.source_scenario_path.as_deref().map(Path::new);
        let design = self
            .service
            .create_design_blocking(
                &request.design_id,
                &display_name,
                &design_root,
                request.baseline.as_deref(),
                source,
                &request.parameters,
            )
            .map_err(mcp_error)?;
        json_output(json!({
            "design": design,
            "next_actions": ["update_design_parameters", "evaluate_feasibility"],
        }))
    }

    #[tool(description = "Update canonical design parameters using backend-neutral dotted paths")]
    fn update_design_parameters(
        &self,
        Parameters(request): Parameters<UpdateDesignRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        design::update(&self.service, request)
    }

    #[tool(
        description = "Evaluate conceptual feasibility with native analysis and optional explicit OpenVSP refinement"
    )]
    fn evaluate_feasibility(
        &self,
        Parameters(request): Parameters<EvaluateFeasibilityRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        design::evaluate(&self.service, request)
    }

    #[tool(
        description = "Auto-refine a fixed-wing concept until requirements and conceptual aero-structural screens converge"
    )]
    fn auto_refine_design(
        &self,
        Parameters(request): Parameters<AutoRefineDesignRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        design::refine(&self.service, request)
    }

    #[tool(
        description = "List native and optional analysis backends with availability and capabilities"
    )]
    fn list_analysis_backends(&self) -> Result<Json<ObjectOutput>, ErrorData> {
        json_output(json!({ "backends": self.service.list_analysis_backends() }))
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
        enforce_strict(request.strict, &resolved.warnings).map_err(mcp_error)?;
        json_output_for_scenario(
            json!({
                "resolved_scenario": resolved,
                "assumptions": resolved.assumptions,
                "warnings": resolved.warnings,
            }),
            Path::new(&request.scenario_path),
            request.units.as_deref(),
        )
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
        enforce_strict(request.strict, &result.warnings).map_err(mcp_error)?;
        json_output_for_scenario(
            result,
            Path::new(&request.scenario_path),
            request.units.as_deref(),
        )
    }

    #[tool(description = "Simulate an ordered quasi-steady mission with mass continuity")]
    fn simulate_mission(
        &self,
        Parameters(request): Parameters<ScenarioRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        mission::simulate(&self.service, request)
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
        enforce_strict(request.strict, &result.warnings).map_err(mcp_error)?;
        let chart = constraints(&scenario, &result);
        if let Some(path) = &request.artifact_path {
            render_svg_blocking(&chart, Path::new(path)).map_err(mcp_error)?;
        }
        json_output_for_scenario(
            json!({
                "result": result,
                "chart_spec": chart,
                "artifact_path": request.artifact_path,
            }),
            Path::new(&request.scenario_path),
            request.units.as_deref(),
        )
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
        enforce_strict(request.strict, &result.warnings).map_err(mcp_error)?;
        let chart = request
            .metrics
            .first()
            .map(|metric| sweep(&result, metric))
            .transpose()
            .map_err(mcp_error)?;
        json_output_for_scenario(
            json!({ "result": result, "chart_spec": chart }),
            Path::new(&request.scenario_path),
            request.units.as_deref(),
        )
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
        enforce_strict(request.strict, &result.warnings).map_err(mcp_error)?;
        let path = paths
            .first()
            .ok_or_else(|| ErrorData::invalid_params("scenario_paths must not be empty", None))?;
        json_output_for_scenario(result, path, request.units.as_deref())
    }

    #[tool(description = "Compare selected conceptual metrics across editable designs")]
    fn compare_designs(
        &self,
        Parameters(request): Parameters<CompareDesignsRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let paths = request
            .design_paths
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let result = self
            .service
            .compare_blocking(&paths, &request.metrics)
            .map_err(mcp_error)?;
        enforce_strict(request.strict, &result.warnings).map_err(mcp_error)?;
        let path = paths
            .first()
            .ok_or_else(|| ErrorData::invalid_params("design_paths must not be empty", None))?;
        json_output_for_scenario(result, path, request.units.as_deref())
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

    #[tool(description = "Generate an organized concept-design report or immutable-run report")]
    fn generate_report(
        &self,
        Parameters(request): Parameters<ReportRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        let scenario_path = request.scenario_path.clone();
        let units = request.units.clone();
        let value = report::generate_report_blocking(&self.service, request).map_err(mcp_error)?;
        match scenario_path {
            Some(path) => json_output_for_scenario(value, Path::new(&path), units.as_deref()),
            None => json_output(value),
        }
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
        enforce_strict(request.strict, &result.warnings).map_err(mcp_error)?;
        let chart = payload_range(&scenario, &result);
        if let Some(path) = &request.artifact_path {
            render_svg_blocking(&chart, Path::new(path)).map_err(mcp_error)?;
        }
        json_output_for_scenario(
            json!({
                "result": result,
                "chart_spec": chart,
                "artifact_path": request.artifact_path,
            }),
            Path::new(&request.scenario_path),
            request.units.as_deref(),
        )
    }
}

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

fn mcp_serialization_error(source: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(source.to_string(), None)
}

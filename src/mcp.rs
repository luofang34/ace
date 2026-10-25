mod capabilities;
mod domain_error;
mod handler;
mod output;
mod parameters;
mod report;
mod schema;
pub(crate) mod serialization;
mod study;
mod synthesis;
mod system;
mod transport;
pub(crate) use transport::serve_stdio;

use std::path::Path;

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData, handler::server::router::tool::ToolRouter, tool, tool_router};
use serde_json::{Value, json};

use crate::charts::generators::{constraints, payload_range};
use crate::charts::renderer::render_svg_blocking;

use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::warning::enforce_strict;
use crate::services::analysis::{ApplicationService, PointCondition};
use crate::services::validator::{error_validation, validate_document_value};
use domain_error::mcp_error;
use output::{ObjectOutput, json_output, json_output_for_scenario, json_output_for_units};
use parameters::parse_wing_loading;
use schema::{
    ConstraintRequest, GetProfileRequest, ListProfilesRequest, LoadStudyRequest,
    PayloadRangeRequest, PointRequest, PromoteStudyCandidateRequest, QueryStudyRequest,
    ReportRequest, RunStudyRequest, ScenarioRequest, ValidateDocumentRequest,
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
            tool_router: Self::tool_router() + Self::system_router(),
        }
    }
}

#[tool_router]
impl AexMcpServer {
    #[tool(
        description = "Evaluate a vehicle synthesis graph with closure, margins, and calculation provenance"
    )]
    fn synthesize_vehicle(
        &self,
        Parameters(request): Parameters<synthesis::Request>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        synthesis::run(request)
    }

    #[tool(
        description = "Run a bounded vehicle study with canonical objectives, margins and separate provisional Pareto evidence"
    )]
    fn study_vehicle(
        &self,
        Parameters(request): Parameters<synthesis::StudyRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        synthesis::study(request)
    }

    #[tool(
        description = "Synthesize vehicles and tabulate named parameters with provenance, margins and feasibility"
    )]
    fn compare_vehicles(
        &self,
        Parameters(request): Parameters<synthesis::CompareRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        synthesis::compare(request)
    }

    #[tool(
        description = "Explain a vehicle parameter: declaration, value, producing calculation, upstream producers, consumers and margins"
    )]
    fn explain_vehicle_value(
        &self,
        Parameters(request): Parameters<synthesis::ExplainRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        synthesis::explain(request)
    }

    #[tool(
        description = "Apply typed edits and parameter assignments to a vehicle file as one validated, revision-checked revision"
    )]
    fn edit_vehicle(
        &self,
        Parameters(request): Parameters<synthesis::EditRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        synthesis::edit(request)
    }

    #[tool(
        description = "Verify a vehicle study archive and write one feasible candidate as a new vehicle file"
    )]
    fn promote_vehicle_candidate(
        &self,
        Parameters(request): Parameters<synthesis::PromoteRequest>,
    ) -> Result<Json<ObjectOutput>, ErrorData> {
        synthesis::promote(request)
    }

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
        let condition = &request.condition;
        let speed = optional_quantity(condition.true_airspeed.as_deref(), Dimension::Speed)?;
        let mass = optional_quantity(condition.mass.as_deref(), Dimension::Mass)?;
        let center_of_gravity =
            optional_quantity(condition.center_of_gravity.as_deref(), Dimension::Length)?;
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
                    center_of_gravity_m: center_of_gravity,
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
            None => json_output_for_units(value, units.as_deref()),
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

fn mcp_serialization_error(source: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(source.to_string(), None)
}
fn optional_quantity(value: Option<&str>, dimension: Dimension) -> Result<Option<f64>, ErrorData> {
    value
        .map(|value| parse_quantity(value, dimension))
        .transpose()
        .map_err(mcp_error)
}

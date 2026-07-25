use std::env;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::cli::{
    AnalyzeCommand, Command, CompareArgs, ConstraintArgs, McpCommand, PointArgs, ProfileCommand,
    ReportArgs, ScenarioArgs, SweepArgs,
};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::result::{MissionResult, PerformanceSummary, RequirementEvaluation};
use crate::mcp::serve_stdio;
use crate::services::analysis::{ApplicationService, PointCondition};
use crate::services::report::LIMITATION;
use crate::services::report::markdown_report;
use crate::services::requirements::{evaluate_requirements, hard_requirements_passed};
use crate::services::sweep::SweepVariable;

use super::output::emit_blocking;
use super::plots::execute_plot;
use super::strict::enforce as enforce_strict;

#[derive(Debug, Serialize)]
struct AnalysisEnvelope<T: Serialize> {
    scenario_name: String,
    run_id: String,
    completion_status: String,
    warning_count: usize,
    result: T,
}

#[derive(Debug, Serialize)]
struct MissionEnvelope {
    scenario_name: String,
    run_id: String,
    completion_status: String,
    hard_requirements_passed: bool,
    warning_count: usize,
    mission: MissionResult,
    performance: PerformanceSummary,
    requirements: Vec<RequirementEvaluation>,
    limitation: &'static str,
    report_markdown: String,
}

pub(super) async fn execute(command: Command) -> AexResult<()> {
    let service = service_blocking()?;
    match command {
        Command::Validate { path, output } => {
            let result = service.validate_path_blocking(&path)?;
            emit_blocking(&result, &output)
        }
        Command::Resolve { scenario, common } => {
            let result = service.resolve_blocking(&scenario, &common.override_map())?;
            enforce_strict(common.strict, &result.warnings)?;
            emit_blocking(&result, &common.output)
        }
        Command::Analyze { analysis } => execute_analysis(&service, analysis),
        Command::Sweep(arguments) => execute_sweep(&service, arguments),
        Command::Compare(arguments) => execute_compare(&service, arguments),
        Command::Profile { profile } => execute_profile(&service, profile),
        Command::Plot(arguments) => execute_plot(&service, arguments),
        Command::Report(arguments) => execute_report(&service, arguments),
        Command::Mcp {
            mcp: McpCommand::Serve,
        } => serve_stdio(service).await,
    }
}

fn execute_analysis(service: &ApplicationService, analysis: AnalyzeCommand) -> AexResult<()> {
    match analysis {
        AnalyzeCommand::Point(arguments) => execute_point(service, arguments),
        AnalyzeCommand::Mission(arguments) => execute_mission(service, arguments),
        AnalyzeCommand::Constraints(arguments) => execute_constraints(service, arguments),
        AnalyzeCommand::PayloadRange(arguments) => execute_payload_range(service, arguments),
        AnalyzeCommand::Performance(arguments) => execute_performance(service, arguments),
    }
}

fn execute_point(service: &ApplicationService, arguments: PointArgs) -> AexResult<()> {
    let altitude = parse_quantity(&arguments.altitude, Dimension::Length)?;
    let speed = arguments
        .speed
        .as_deref()
        .map(|value| parse_quantity(value, Dimension::Speed))
        .transpose()?;
    let mass = arguments
        .mass
        .as_deref()
        .map(|value| parse_quantity(value, Dimension::Mass))
        .transpose()?;
    let (scenario, result) = service.point_blocking(
        &arguments.scenario,
        &arguments.common.override_map(),
        PointCondition {
            altitude_m: altitude,
            speed_m_s: speed,
            mach: arguments.mach,
            mass_kg: mass,
            configuration: arguments.configuration,
        },
    )?;
    enforce_strict(arguments.common.strict, &result.warnings)?;
    let run = service.persist_blocking(
        &scenario,
        "point",
        &result,
        &result.warnings,
        arguments.common.seed,
        &[],
    )?;
    emit_blocking(
        &AnalysisEnvelope {
            scenario_name: scenario.name,
            run_id: run.run_id,
            completion_status: "completed".to_owned(),
            warning_count: result.warnings.len(),
            result,
        },
        &arguments.common.output,
    )
}

fn execute_mission(service: &ApplicationService, arguments: ScenarioArgs) -> AexResult<()> {
    let overrides = arguments.common.override_map();
    let (scenario, mission) = service.mission_blocking(&arguments.scenario, &overrides)?;
    let (_, performance) = service.performance_blocking(&arguments.scenario, &overrides)?;
    let (_, payload_range) = service.payload_range_blocking(&arguments.scenario, &overrides)?;
    let requirements =
        evaluate_requirements(&scenario, &mission, &performance, Some(&payload_range));
    enforce_strict(arguments.common.strict, &mission.warnings)?;
    let run = service.persist_blocking(
        &scenario,
        "mission",
        &mission,
        &mission.warnings,
        arguments.common.seed,
        &[],
    )?;
    let headline_passed = hard_requirements_passed(
        mission.completed,
        &scenario.requirements.items,
        &requirements,
    );
    let report_markdown = markdown_report(
        &scenario,
        &run.run_id,
        &mission,
        &performance,
        &requirements,
    )?;
    emit_blocking(
        &MissionEnvelope {
            scenario_name: scenario.name,
            run_id: run.run_id,
            completion_status: if mission.completed {
                "completed"
            } else {
                "incomplete"
            }
            .to_owned(),
            hard_requirements_passed: headline_passed,
            warning_count: mission.warnings.len(),
            mission,
            performance,
            requirements,
            limitation: LIMITATION,
            report_markdown,
        },
        &arguments.common.output,
    )
}

fn execute_constraints(service: &ApplicationService, arguments: ConstraintArgs) -> AexResult<()> {
    let start = parse_wing_loading(&arguments.start)?;
    let stop = parse_wing_loading(&arguments.stop)?;
    let (scenario, result) = service.constraints_blocking(
        &arguments.scenario,
        &arguments.common.override_map(),
        start,
        stop,
        arguments.count,
    )?;
    enforce_strict(arguments.common.strict, &result.warnings)?;
    let run = service.persist_blocking(
        &scenario,
        "constraints",
        &result,
        &result.warnings,
        arguments.common.seed,
        &[],
    )?;
    emit_blocking(
        &AnalysisEnvelope {
            scenario_name: scenario.name,
            run_id: run.run_id,
            completion_status: "completed".to_owned(),
            warning_count: result.warnings.len(),
            result,
        },
        &arguments.common.output,
    )
}

fn execute_payload_range(service: &ApplicationService, arguments: ScenarioArgs) -> AexResult<()> {
    let (scenario, result) =
        service.payload_range_blocking(&arguments.scenario, &arguments.common.override_map())?;
    enforce_strict(arguments.common.strict, &result.warnings)?;
    let run = service.persist_blocking(
        &scenario,
        "payload-range",
        &result,
        &result.warnings,
        arguments.common.seed,
        &[],
    )?;
    emit_blocking(
        &AnalysisEnvelope {
            scenario_name: scenario.name,
            run_id: run.run_id,
            completion_status: "completed".to_owned(),
            warning_count: result.warnings.len(),
            result,
        },
        &arguments.common.output,
    )
}

fn execute_performance(service: &ApplicationService, arguments: ScenarioArgs) -> AexResult<()> {
    let (scenario, result) =
        service.performance_blocking(&arguments.scenario, &arguments.common.override_map())?;
    enforce_strict(arguments.common.strict, &result.warnings)?;
    let run = service.persist_blocking(
        &scenario,
        "performance",
        &result,
        &result.warnings,
        arguments.common.seed,
        &[],
    )?;
    emit_blocking(
        &AnalysisEnvelope {
            scenario_name: scenario.name,
            run_id: run.run_id,
            completion_status: "completed".to_owned(),
            warning_count: result.warnings.len(),
            result,
        },
        &arguments.common.output,
    )
}

fn execute_sweep(service: &ApplicationService, arguments: SweepArgs) -> AexResult<()> {
    let variables = arguments
        .variables
        .iter()
        .map(|raw| parse_sweep_variable(raw, arguments.logarithmic))
        .collect::<AexResult<Vec<_>>>()?;
    let result = service.sweep_blocking(&arguments.scenario, &variables, &arguments.metrics)?;
    enforce_strict(arguments.common.strict, &result.warnings)?;
    let scenario =
        service.resolve_blocking(&arguments.scenario, &arguments.common.override_map())?;
    let run = service.persist_blocking(
        &scenario,
        "sweep",
        &result,
        &result.warnings,
        arguments.common.seed,
        &[],
    )?;
    emit_blocking(
        &AnalysisEnvelope {
            scenario_name: scenario.name,
            run_id: run.run_id,
            completion_status: "completed".to_owned(),
            warning_count: result.warnings.len(),
            result,
        },
        &arguments.common.output,
    )
}

fn execute_compare(service: &ApplicationService, arguments: CompareArgs) -> AexResult<()> {
    let result = service.compare_blocking(&arguments.scenarios, &arguments.metrics)?;
    emit_blocking(&result, &arguments.output)
}

fn execute_profile(service: &ApplicationService, command: ProfileCommand) -> AexResult<()> {
    match command {
        ProfileCommand::List {
            directory,
            profile_type,
            query,
            output,
        } => {
            let profiles = service.list_profiles_blocking(
                &directory,
                profile_type.as_deref(),
                query.as_deref(),
            )?;
            emit_blocking(&serde_json::json!({ "profiles": profiles }), &output)
        }
        ProfileCommand::Show {
            profile_id,
            directory,
            output,
        } => {
            let profile = service.get_profile_blocking(&directory, &profile_id)?;
            emit_blocking(&profile, &output)
        }
        ProfileCommand::Validate { path, output } => {
            let result = service.validate_path_blocking(&path)?;
            emit_blocking(&result, &output)
        }
    }
}

fn execute_report(service: &ApplicationService, arguments: ReportArgs) -> AexResult<()> {
    if arguments.format != "markdown" && arguments.format != "json" {
        return Err(AexError::validation(
            "UNSUPPORTED_REPORT_FORMAT",
            "format",
            arguments.format,
        ));
    }
    let (manifest, result) = service.load_run_blocking(&arguments.run_id)?;
    if arguments.format == "json" {
        return emit_blocking(
            &serde_json::json!({ "manifest": manifest, "result": result }),
            &arguments.output,
        );
    }
    let report = format!(
        "# Analysis run {}\n\n{}\n\n## Manifest\n\n```json\n{}\n```\n\n## Results\n\n```json\n{}\n```\n",
        arguments.run_id,
        LIMITATION,
        pretty_json(&manifest)?,
        pretty_json(&result)?,
    );
    emit_blocking(&report, &arguments.output)
}

fn service_blocking() -> AexResult<ApplicationService> {
    let current = env::current_dir().map_err(|source| AexError::Read {
        path: PathBuf::from("."),
        source,
    })?;
    Ok(ApplicationService::filesystem(current.join("runs")))
}

fn parse_sweep_variable(raw: &str, logarithmic: bool) -> AexResult<SweepVariable> {
    let (path, specification) = raw.split_once('=').ok_or_else(|| {
        AexError::validation(
            "INVALID_SWEEP_VARIABLE",
            "var",
            "expected PATH=START:STOP:COUNT",
        )
    })?;
    let parts: Vec<&str> = specification.rsplitn(3, ':').collect();
    if parts.len() != 3 {
        return Err(AexError::validation(
            "INVALID_SWEEP_VARIABLE",
            path,
            "expected START:STOP:COUNT",
        ));
    }
    let count = parts[0]
        .parse::<u32>()
        .map_err(|source| AexError::validation("INVALID_SWEEP_COUNT", path, source.to_string()))?;
    SweepVariable::linear(path.to_owned(), parts[2], parts[1], count, logarithmic)
}

pub(crate) fn parse_wing_loading(raw: &str) -> AexResult<f64> {
    let (number, unit) = raw.split_once(char::is_whitespace).ok_or_else(|| {
        AexError::validation(
            "AMBIGUOUS_UNITLESS_VALUE",
            "wing_loading",
            "wing loading requires units",
        )
    })?;
    let value = number.parse::<f64>().map_err(|source| {
        AexError::validation("INVALID_WING_LOADING", "wing_loading", source.to_string())
    })?;
    match unit.trim() {
        "N/m^2" | "N/m2" => Ok(value),
        "lbf/ft^2" | "psf" => Ok(value * 47.880_258_98),
        other => Err(AexError::validation(
            "INCOMPATIBLE_UNITS",
            "wing_loading",
            format!("unsupported unit {other}"),
        )),
    }
}

fn pretty_json(value: &Value) -> AexResult<String> {
    serde_json::to_string_pretty(value).map_err(|source| AexError::Json { source })
}

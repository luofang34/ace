use std::env;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::cli::{
    AnalyzeCommand, Command, McpCommand, PerformanceArgs, PointArgs, ProfileCommand, ReportArgs,
};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::warning::enforce_strict;
use crate::mcp::serve_stdio;
use crate::services::analysis::{ApplicationService, PointCondition};
use crate::services::report::LIMITATION;
use crate::storage::run_store::RunAttribution;

use super::output::{emit_blocking, emit_canonical_blocking, emit_scenario_blocking};
use super::plots::execute_plot;

#[derive(Debug, Serialize)]
struct AnalysisEnvelope<T: Serialize> {
    scenario_name: String,
    run_id: String,
    completion_status: String,
    warning_count: usize,
    result: T,
}

pub(super) async fn execute(command: Command) -> AexResult<()> {
    let service = service_blocking()?;
    match command {
        Command::Drawing(args) => super::vehicle::drawing_blocking(args),
        Command::Capabilities { output } => emit_blocking(&service.capabilities()?, &output),
        Command::Validate { path, output } => {
            let result = service.validate_path_blocking(&path)?;
            emit_blocking(&result, &output)
        }
        Command::Resolve { scenario, common } => {
            let result = service.resolve_blocking(&scenario, &common.override_map())?;
            enforce_strict(common.strict, &result.warnings)?;
            emit_scenario_blocking(&result, &common.output, &scenario)
        }
        Command::Analyze { analysis } => execute_analysis(&service, analysis),
        Command::Compare(arguments) => {
            let result = crate::services::vehicle_comparison::compare_blocking(
                &arguments.vehicles,
                &arguments.metrics,
            )?;
            emit_canonical_blocking(&result, &arguments.output)
        }
        Command::PayloadRange(arguments) => {
            let result = crate::services::vehicle_payload_range::compute_blocking(
                &arguments.vehicle,
                &crate::services::vehicle_payload_range::PayloadRangeRoles {
                    distance: &arguments.distance,
                    remaining: &arguments.remaining,
                    maximum_takeoff_mass: &arguments.maximum_takeoff_mass,
                    maximum_payload: &arguments.maximum_payload,
                },
            )?;
            let result = crate::services::vehicle_chart::payload_range_blocking(
                result,
                arguments.artifact.as_deref(),
            )?;
            emit_canonical_blocking(&result, &arguments.output)
        }
        Command::Chart {
            vehicle,
            analysis,
            artifact,
            common,
        } => {
            let result = crate::services::vehicle_chart::analysis_blocking(
                &vehicle,
                &common.override_map(),
                &analysis,
                artifact.as_deref(),
            )?;
            emit_canonical_blocking(&result, &common.output)
        }
        Command::Profile { profile } => execute_profile(&service, profile),
        Command::Plot(arguments) => execute_plot(&service, arguments),
        Command::Report(arguments) => execute_report(&service, arguments),
        Command::Synthesize(arguments) => {
            let result = crate::services::synthesis::run_blocking(
                &arguments.scenario,
                &arguments.common.override_map(),
            )?;
            emit_canonical_blocking(&result, &arguments.common.output)
        }
        Command::Study {
            scenario,
            plan,
            chart,
            artifact,
            common,
        } => {
            let archive = crate::services::vehicle_study::run_blocking(
                &scenario,
                &plan,
                &common.override_map(),
            )?;
            let result = crate::services::vehicle_study::chart_blocking(archive, chart, artifact)?;
            emit_canonical_blocking(&result, &common.output)
        }
        command @ Command::Edit { .. } => super::vehicle::edit_blocking(command),
        Command::Promote {
            archive,
            candidate,
            save_as,
            output,
        } => emit_canonical_blocking(
            &crate::services::vehicle_promotion::promote_blocking(&archive, &candidate, &save_as)?,
            &output,
        ),
        Command::Explain {
            scenario,
            parameter,
            depth,
            common,
        } => {
            let result = crate::services::vehicle_explanation::explain_blocking(
                &scenario,
                &common.override_map(),
                &parameter,
                depth,
            )?;
            emit_canonical_blocking(&result, &common.output)
        }
        command @ (Command::Uncertainty { .. }
        | Command::Calibrate { .. }
        | Command::Campaign { .. }) => super::vehicle::system_blocking(command),
        Command::Sheet {
            scenario,
            port,
            common,
        } => super::vehicle::sheet(scenario, port, common).await,
        Command::Mcp {
            mcp: McpCommand::Serve,
        } => serve_stdio(service).await,
    }
}

fn execute_analysis(service: &ApplicationService, analysis: AnalyzeCommand) -> AexResult<()> {
    match analysis {
        AnalyzeCommand::Point(arguments) => execute_point(service, arguments),
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
    let center_of_gravity = arguments
        .cg
        .as_deref()
        .map(|value| parse_quantity(value, Dimension::Length))
        .transpose()?;
    let (scenario, result) = service.point_blocking(
        &arguments.scenario,
        &arguments.common.override_map(),
        PointCondition {
            altitude_m: altitude,
            speed_m_s: speed,
            mach: arguments.mach,
            mass_kg: mass,
            center_of_gravity_m: center_of_gravity,
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
        RunAttribution::default(),
    )?;
    emit_scenario_blocking(
        &AnalysisEnvelope {
            scenario_name: scenario.name,
            run_id: run.run_id,
            completion_status: "completed".to_owned(),
            warning_count: result.warnings.len(),
            result,
        },
        &arguments.common.output,
        &arguments.scenario,
    )
}

fn execute_performance(service: &ApplicationService, arguments: PerformanceArgs) -> AexResult<()> {
    let mut overrides = arguments.common.override_map();
    for (path, value) in [
        ("mission.field.takeoff_mass", &arguments.mass),
        ("mission.field.landing_mass", &arguments.landing_mass),
    ] {
        if let Some(value) = value {
            overrides.insert(path.to_owned(), value.clone());
        }
    }
    let (scenario, result) = service.performance_blocking(&arguments.scenario, &overrides)?;
    enforce_strict(arguments.common.strict, &result.warnings)?;
    let run = service.persist_blocking(
        &scenario,
        "performance",
        &result,
        &result.warnings,
        arguments.common.seed,
        RunAttribution::default(),
    )?;
    emit_scenario_blocking(
        &AnalysisEnvelope {
            scenario_name: scenario.name,
            run_id: run.run_id,
            completion_status: "completed".to_owned(),
            warning_count: result.warnings.len(),
            result,
        },
        &arguments.common.output,
        &arguments.scenario,
    )
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
    if let Some(vehicle) = &arguments.vehicle {
        let report =
            crate::services::vehicle_report::report_blocking(vehicle, &Default::default())?;
        return if arguments.format == "json" {
            emit_blocking(&report, &arguments.output)
        } else {
            emit_blocking(
                &crate::services::vehicle_report::markdown(&report)?,
                &arguments.output,
            )
        };
    }
    let run_id = arguments.run_id.as_deref().unwrap_or_default();
    let (manifest, result) = service.load_run_blocking(run_id)?;
    if arguments.format == "json" {
        return emit_blocking(
            &serde_json::json!({ "manifest": manifest, "result": result }),
            &arguments.output,
        );
    }
    let report = format!(
        "# Analysis run {}\n\n{}\n\n## Manifest\n\n```json\n{}\n```\n\n## Results\n\n```json\n{}\n```\n",
        run_id,
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

fn pretty_json(value: &Value) -> AexResult<String> {
    serde_json::to_string_pretty(value).map_err(|source| AexError::Json { source })
}

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::charts::generators::{
    climb_envelope, constraints, drag_polar, mission_mass, payload_range, performance_curves,
    requirement_margins,
};
use crate::charts::renderer::render_svg_blocking;
use crate::charts::spec::ChartSpec;
use crate::cli::{PlotArgs, PlotKind};
use crate::domain::diagnostic::AexResult;
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::services::analysis::ApplicationService;
use crate::services::requirements::evaluate_requirements;

use super::output::emit_scenario_blocking;
use super::strict::enforce as enforce_strict;

#[derive(Debug, Serialize)]
struct PlotOutput {
    artifact_path: PathBuf,
    chart: ChartSpec,
}

pub(super) fn execute_plot(service: &ApplicationService, arguments: PlotArgs) -> AexResult<()> {
    let overrides = arguments.override_map();
    let scenario = service.resolve_blocking(&arguments.scenario, &overrides)?;
    let chart = generate_chart(service, &arguments, &overrides, &scenario)?;
    enforce_strict(arguments.strict, &chart.warnings)?;
    let output = arguments
        .artifact
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("{}.svg", arguments.kind.file_stem())));
    render_svg_blocking(&chart, &output)?;
    let artifact = output.display().to_string();
    let run = service.persist_blocking(
        &scenario,
        "plot",
        &chart,
        &chart.warnings,
        arguments.seed,
        std::slice::from_ref(&artifact),
    )?;
    emit_scenario_blocking(
        &PlotOutput {
            artifact_path: output,
            chart,
        },
        &arguments.output_args(),
        &arguments.scenario,
    )?;
    tracing::info!(run_id = %run.run_id, artifact = %artifact, "plot completed");
    Ok(())
}

fn generate_chart(
    service: &ApplicationService,
    arguments: &PlotArgs,
    overrides: &BTreeMap<String, String>,
    scenario: &crate::domain::schema::ResolvedScenario,
) -> AexResult<ChartSpec> {
    match arguments.kind {
        PlotKind::DragPolar => Ok(drag_polar(scenario)),
        PlotKind::PowerCurves | PlotKind::ThrustCurves => {
            let altitude = parse_quantity(&arguments.altitude, Dimension::Length)?;
            performance_curves(scenario, altitude)
        }
        PlotKind::ClimbEnvelope => climb_envelope(scenario),
        PlotKind::PayloadRange => {
            let (_, result) = service.payload_range_blocking(&arguments.scenario, overrides)?;
            Ok(payload_range(scenario, &result))
        }
        PlotKind::Constraints => {
            let (_, result) =
                service.constraints_blocking(&arguments.scenario, overrides, 300.0, 9000.0, 80)?;
            Ok(constraints(scenario, &result))
        }
        PlotKind::MissionMass => {
            let (_, result) = service.mission_blocking(&arguments.scenario, overrides)?;
            Ok(mission_mass(scenario, &result))
        }
        PlotKind::RequirementMargins => {
            let (_, mission) = service.mission_blocking(&arguments.scenario, overrides)?;
            let (_, performance) = service.performance_blocking(&arguments.scenario, overrides)?;
            let (_, payload_range_result) =
                service.payload_range_blocking(&arguments.scenario, overrides)?;
            let evaluations = evaluate_requirements(
                scenario,
                &mission,
                &performance,
                Some(&payload_range_result),
            );
            Ok(requirement_margins(scenario, &evaluations))
        }
    }
}

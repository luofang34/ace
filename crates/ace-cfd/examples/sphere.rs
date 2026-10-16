//! Run an isothermal spherical forebody case from a JSON input file.
use ace_cfd::{
    Convection, FlowOperator, IdealGas, Mesh, Outflow, Primitive, SolverConfig, SphereGrid,
    solve_checkpointed,
};
use serde::Deserialize;
use std::error::Error;
use std::path::Path;
use std::time::Instant;
use tracing::info;

#[path = "sphere/checkpoint.rs"]
mod checkpoint;

const USAGE: &str = "usage: sphere INPUT.json OUTPUT.json [RESTART.json] \
    [--audit|--diagnose|--faces|--checkpoint=PATH]";

enum RunMode<'a> {
    Solve(Option<&'a Path>),
    Audit,
    Diagnose,
    Faces,
}

fn mode(argument: Option<&str>) -> Result<RunMode<'_>, Box<dyn Error>> {
    match argument {
        None => Ok(RunMode::Solve(None)),
        Some("--audit") => Ok(RunMode::Audit),
        Some("--diagnose") => Ok(RunMode::Diagnose),
        Some("--faces") => Ok(RunMode::Faces),
        Some(option) => option
            .strip_prefix("--checkpoint=")
            .filter(|path| !path.is_empty())
            .map(|path| RunMode::Solve(Some(Path::new(path))))
            .ok_or_else(|| "expected --audit, --diagnose, --faces or --checkpoint=PATH".into()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    grid: SphereGrid,
    gas: IdealGas,
    inflow: Primitive,
    wall_temperature: f64,
    solver: SolverConfig,
    #[serde(default)]
    convection: Convection,
    #[serde(default)]
    outflow: Outflow,
    #[serde(default)]
    mesh_vertices: Option<Vec<[f64; 3]>>,
    #[serde(default)]
    curved_geometry: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let arguments: Vec<String> = std::env::args().collect();
    let input = arguments.get(1).ok_or(USAGE)?;
    let output = arguments.get(2).ok_or(USAGE)?;
    if arguments.len() > 5 {
        return Err("unexpected command arguments".into());
    }
    let third_is_option = arguments.get(3).is_some_and(|arg| arg.starts_with("--"));
    if third_is_option && arguments.len() > 4 {
        return Err("unexpected command arguments".into());
    }
    run_blocking(
        Path::new(input),
        Path::new(output),
        arguments.get(3).filter(|_| !third_is_option).map(Path::new),
        mode(
            arguments
                .get(if third_is_option { 3 } else { 4 })
                .map(String::as_str),
        )?,
    )
}

fn run_blocking(
    input: &Path,
    output: &Path,
    restart: Option<&Path>,
    mode: RunMode<'_>,
) -> Result<(), Box<dyn Error>> {
    let case: Case = serde_json::from_slice(&std::fs::read(input)?)?;
    let start = Instant::now();
    let mesh_vertices = case
        .mesh_vertices
        .map_or_else(|| case.grid.vertices(), Ok)?;
    let mesh = if case.curved_geometry {
        Mesh::curved_from_vertices(case.grid, &mesh_vertices)?
    } else {
        Mesh::from_vertices(case.grid, &mesh_vertices)?
    };
    let op = FlowOperator::new(mesh, case.gas, case.inflow, case.wall_temperature)?
        .with_convection(case.convection)
        .with_outflow(case.outflow);
    let preparation_seconds = start.elapsed().as_secs_f64();
    let start = Instant::now();
    let initial: Option<Vec<ace_cfd::Conserved>> = restart
        .map(|path| -> Result<_, Box<dyn Error>> {
            let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
            Ok(serde_json::from_value(value["solution"]["state"].clone())?)
        })
        .transpose()?;
    let checkpoint = if let RunMode::Solve(path) = mode {
        path
    } else {
        let state = initial.ok_or("diagnostics require a saved state")?;
        return audit_blocking(&op, output, &state, mode, case.curved_geometry);
    };
    let solution = solve_checkpointed(&op, case.solver, initial, |check, state| {
        info!(
            step = check.step,
            residual = check.residual,
            peak_cell = check.peak_cell,
            heat_change = check.heat_flux_change,
            nose_heat_flux = check.nose_heat_flux,
            integrated_heating = check.integrated_heating,
            "steady convergence"
        );
        if let Some(path) = checkpoint {
            checkpoint::write_blocking(path, check, state)?;
        }
        Ok::<_, checkpoint::CheckpointError>(())
    })?;
    let solve_seconds = start.elapsed().as_secs_f64();
    info!(converged = solution.converged, steps = solution.steps, solve_seconds,
        last_check = ?solution.history.last(), "sphere solve finished");
    let result = serde_json::json!({"preparation_seconds": preparation_seconds,
        "curved_geometry": case.curved_geometry,
        "solve_seconds": solve_seconds, "solution": solution, "mesh_vertices": mesh_vertices});
    std::fs::write(output, serde_json::to_vec_pretty(&result)?)?;
    if !solution.converged {
        return Err("steady and heat-flux convergence criteria were not met".into());
    }
    Ok(())
}

fn audit_blocking(
    op: &FlowOperator,
    output: &Path,
    state: &[ace_cfd::Conserved],
    mode: RunMode<'_>,
    curved_geometry: bool,
) -> Result<(), Box<dyn Error>> {
    let balance = op.energy_balance(state)?;
    let value = if matches!(mode, RunMode::Faces) {
        serde_json::json!({"balance": balance, "faces": op.face_diagnostics(state)?})
    } else if matches!(mode, RunMode::Diagnose) {
        serde_json::json!({"balance": balance, "cells": op.cell_diagnostics(state)?,
            "wall": op.wall_samples(state)?, "curved_geometry": curved_geometry})
    } else {
        serde_json::to_value(&balance)?
    };
    std::fs::write(output, serde_json::to_vec_pretty(&value)?)?;
    info!(?balance, "saved solution energy audit");
    Ok(())
}

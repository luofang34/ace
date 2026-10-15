//! CLI and MCP execute the same Rust synthesis model and registered solver.
#![cfg(test)]

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;

fn vehicle() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/synthesis/transport.json")
}

fn run(overrides: &[&str]) -> Result<Value, Box<dyn Error>> {
    let mut command = cargo_bin_cmd!("aex");
    command
        .arg("synthesize")
        .arg(vehicle())
        .args(["--format", "json"]);
    for value in overrides {
        command.args(["--set", value]);
    }
    let output = command.assert().success().get_output().clone();
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn verify_evidence(result: &Value) -> Result<(), Box<dyn Error>> {
    use sha2::{Digest, Sha256};
    let model: ace_model::VehicleModel = serde_json::from_value(result["model"].clone())?;
    let snapshot: ace_engine::Snapshot = serde_json::from_value(result["snapshot"].clone())?;
    assert_eq!(
        snapshot.model_digest,
        hex::encode(Sha256::digest(serde_json::to_vec(&model)?))
    );
    for mut run in snapshot.runs {
        let expected = std::mem::take(&mut run.id);
        assert_eq!(
            expected,
            hex::encode(Sha256::digest(serde_json::to_vec(&run)?))
        );
    }
    Ok(())
}

#[test]
fn synthesis_executes_native_weight_solver_and_closes_fuel_mass() -> Result<(), Box<dyn Error>> {
    let result = run(&[])?;
    assert_eq!(result["completed"], true);
    assert_eq!(
        result["snapshot"]["closures"]["mass_fuel"]["converged"],
        true
    );
    assert_eq!(result["margins"]["gate_span"]["status"], "WARN");
    let mtow = result["snapshot"]["values"]["weight.mtow"]["quantity"]["value"]
        .as_f64()
        .ok_or("missing takeoff mass")?;
    let fuel = result["snapshot"]["values"]["fuel.required"]["quantity"]["value"]
        .as_f64()
        .ok_or("missing required fuel")?;
    assert!((mtow - 2.0 * (25_000.0 + fuel)).abs() < 0.02);
    let runs = result["snapshot"]["runs"]
        .as_array()
        .ok_or("missing evidence")?;
    assert!(
        runs.iter()
            .any(|run| run["plugin"]["id"] == "aircraft.weight_closure")
    );
    Ok(())
}

#[test]
fn span_edit_changes_aspect_ratio_and_exposes_airport_violation() -> Result<(), Box<dyn Error>> {
    let result = run(&["wing.span=37 m"])?;
    assert_eq!(result["completed"], true);
    let ar = result["snapshot"]["values"]["wing.aspect_ratio"]["quantity"]["value"]
        .as_f64()
        .ok_or("missing aspect ratio")?;
    assert!((ar - 13.69).abs() < 1e-10);
    assert_eq!(result["margins"]["gate_span"]["status"], "FAIL");
    assert_eq!(result["margins"]["gate_span"]["absolute"]["value"], -1.0);
    assert_eq!(result["snapshot"]["values"]["wing.span"]["revision"], 1);
    Ok(())
}

#[test]
fn synthesis_rejects_edits_to_computed_outputs() {
    cargo_bin_cmd!("aex")
        .arg("synthesize")
        .arg(vehicle())
        .args(["--format", "json", "--set", "weight.mtow=10 kg"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("SYNTHESIS_ERROR"));
}

#[test]
fn synthesis_projects_domain_evidence_from_the_same_authoritative_graph()
-> Result<(), Box<dyn Error>> {
    let result = run(&[])?;
    verify_evidence(&result)?;
    let runs = result["snapshot"]["runs"]
        .as_array()
        .ok_or("missing runs")?;
    let evidence = |node: &str| {
        runs.iter()
            .rev()
            .find(|run| run["node"] == node)
            .ok_or("missing domain run")
    };
    let packing = evidence("analysis:packing")?;
    assert_eq!(packing["details"]["method"], "axis_aligned_envelopes");
    assert_eq!(packing["status"], "approximate");
    assert!(
        !packing["details"]["sections"]
            .as_array()
            .ok_or("missing sections")?
            .is_empty()
    );
    let loading = evidence("analysis:loading")?;
    assert!(loading["details"]["current"]["inertia_kg_m2"].is_null());
    assert!(loading["details"]["loadings"]["empty_tanks"]["state"]["cg_m"].is_array());
    let mission = evidence("analysis:mission")?;
    assert_eq!(mission["details"]["completed"], true);
    assert_eq!(
        mission["details"]["stages"]
            .as_array()
            .ok_or("missing stages")?
            .len(),
        7
    );
    assert_eq!(mission["details"]["branches"]["reserve"]["completed"], true);
    assert_eq!(
        result["snapshot"]["values"]["fuel.capacity"]["quantity"]["value"],
        22400.0
    );
    assert_eq!(
        evidence("analysis:economics")?["details"]["network_compatible"],
        true
    );
    Ok(())
}

#[test]
fn range_payload_and_span_drive_stateful_fuel_closure_and_trip_cost() -> Result<(), Box<dyn Error>>
{
    let base = run(&[])?;
    let short = run(&["mission.distance=1500 nmi"])?;
    let long = run(&["mission.distance=3500 nmi"])?;
    let payload = run(&["payload.mass=26000 kg"])?;
    let span = run(&["wing.span=37 m"])?;
    let value = |result: &Value, key: &str| -> f64 {
        result["snapshot"]["values"][key]["quantity"]["value"]
            .as_f64()
            .unwrap()
    };
    for result in [&base, &short, &long, &payload, &span] {
        assert_eq!(result["completed"], true);
        verify_trajectory(result)?;
        let loaded = value(result, "fuel.loaded");
        let consumed = value(result, "mission.block_fuel") + value(result, "mission.reserve_fuel");
        assert!((loaded - consumed - value(result, "mission.remaining_fuel")).abs() < 1e-7);
        assert!(
            (value(result, "mission.fuel_cost") - 1.2 * value(result, "mission.block_fuel")).abs()
                < 1e-7
        );
        assert!((value(result, "fuel.required") - loaded).abs() < 0.005);
        let closure = &result["snapshot"]["closures"]["mass_fuel"];
        assert_eq!(closure["converged"], true);
        for residual in closure["residuals"]
            .as_object()
            .ok_or("missing residuals")?
            .values()
        {
            assert!(residual["value"].as_f64().ok_or("missing residual")? < 0.005);
        }
    }
    for key in ["fuel.required", "weight.mtow", "mission.block_fuel"] {
        assert!(value(&short, key) < value(&base, key));
        assert!(value(&long, key) > value(&base, key));
        assert!(value(&payload, key) > value(&base, key));
    }
    assert!(value(&span, "mission.block_fuel") < value(&base, "mission.block_fuel"));
    assert!(value(&span, "economics.casm") < value(&base, "economics.casm"));
    assert_eq!(span["margins"]["airport_network"]["status"], "FAIL");
    Ok(())
}

#[test]
fn insufficient_capacity_revokes_closure_mission_and_economic_outputs() -> Result<(), Box<dyn Error>>
{
    let result = run(&["fuel.density=100 kg/m^3"])?;
    assert_eq!(result["completed"], false);
    assert_eq!(
        result["snapshot"]["closures"]["mass_fuel"]["converged"],
        false
    );
    for key in [
        "fuel.required",
        "weight.mtow",
        "mission.block_fuel",
        "economics.casm",
    ] {
        assert!(result["snapshot"]["values"][key]["quantity"].is_null());
    }
    assert_eq!(result["margins"]["cost"]["status"], "UNKNOWN");
    Ok(())
}

#[test]
fn mcp_exposes_the_same_synthesis_service() -> Result<(), Box<dyn Error>> {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
        command.args(["mcp", "serve"]);
        let client = ().serve(TokioChildProcess::new(command)?).await?;
        let request = serde_json::json!({ "vehicle_path": vehicle(), "overrides": { "wing.span": "37 m" } });
        let arguments = request.as_object().cloned().ok_or("invalid arguments")?;
        let result = client.call_tool(CallToolRequestParams {
            meta: None, name: "synthesize_vehicle".into(), arguments: Some(arguments), task: None,
        }).await?;
        let value = result.structured_content.ok_or("missing synthesis result")?;
        assert_eq!(value["completed"], true);
        assert_eq!(value["margins"]["gate_span"]["status"], "FAIL");
        verify_evidence(&value)?;
        client.cancel().await?;
        Ok(())
    })
}

fn verify_trajectory(result: &Value) -> Result<(), Box<dyn Error>> {
    let run = result["snapshot"]["runs"]
        .as_array()
        .ok_or("missing runs")?
        .iter()
        .rev()
        .find(|r| r["node"] == "analysis:mission")
        .ok_or("missing mission")?;
    let mission: ace_physics::MissionReport = serde_json::from_value(run["details"].clone())?;
    let target = result["snapshot"]["values"]["mission.distance"]["quantity"]["value"]
        .as_f64()
        .ok_or("missing range")?;
    assert!(
        (mission
            .final_state
            .state
            .distance
            .to(ace_model::Unit::NauticalMile)?
            .value
            - target)
            .abs()
            < 1e-8
    );
    for path in std::iter::once(&mission).chain(mission.branches.values()) {
        let mut previous = &path.initial;
        for stage in &path.stages {
            assert_eq!(
                serde_json::to_value(&stage.start)?,
                serde_json::to_value(previous)?
            );
            for point in &stage.trajectory {
                let sum: f64 = point.mass.items.values().map(|i| i.mass_kg).sum();
                let moment: f64 = point
                    .mass
                    .items
                    .values()
                    .map(|i| i.mass_kg * i.cg_m[0])
                    .sum();
                assert!((sum - point.mass.total_mass_kg).abs() < 1e-8);
                assert!((moment / sum - point.mass.cg_m[0]).abs() < 1e-10);
            }
            assert!(
                (stage.start.mass.total_mass_kg
                    - stage.end.mass.total_mass_kg
                    - stage.fuel_burn_kg)
                    .abs()
                    < 1e-7
            );
            if let Some(phase) = &stage.phase {
                for key in ["altitude", "speed"] {
                    assert_eq!(
                        phase.initial_state.conditions[key],
                        stage.start.state.conditions[key]
                    );
                }
                if ["climb", "descent", "approach"].contains(&stage.kind.as_str()) {
                    assert_eq!(phase.plugin.id, "aircraft.energy_path_phase");
                }
            }
            previous = &stage.end;
        }
        assert_eq!(path.final_state.state.conditions["altitude"].value, 0.0);
        assert_eq!(path.final_state.state.conditions["speed"].value, 0.0);
    }
    let climb = mission
        .stages
        .iter()
        .find(|s| s.id == "climb")
        .ok_or("missing climb")?;
    assert_eq!(climb.end.state.conditions["altitude"].value, 10000.0);
    assert_eq!(climb.end.state.conditions["speed"].value, 230.0);
    Ok(())
}

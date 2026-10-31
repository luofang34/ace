//! CLI and MCP evaluate vehicles probabilistically, against measurements and as fleets.
#![cfg(test)]

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/system")
        .join(name)
}

fn cli(arguments: &[&str]) -> Result<Value, Box<dyn Error>> {
    let mut command = cargo_bin_cmd!("aex");
    command.args(arguments).args(["--format", "json"]);
    let output = command.assert().success().get_output().clone();
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn text(path: PathBuf) -> Result<String, Box<dyn Error>> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "non-UTF-8 example path".into())
}

#[test]
fn uncertainty_and_calibration_run_on_a_synthesized_vehicle() -> Result<(), Box<dyn Error>> {
    let vehicle = text(example("expendable-uas.json"))?;
    let plan = text(example("uas-uncertainty.json"))?;
    let record = cli(&["uncertainty", &vehicle, "--plan", &plan])?;
    assert_eq!(record["requirements"]["endurance"]["status"], "pass");
    assert_eq!(record["feasibility"], "provisional");
    assert_eq!(record["samples"].as_array().map(Vec::len), Some(400));
    let again = cli(&["uncertainty", &vehicle, "--plan", &plan])?;
    assert_eq!(record["id"], again["id"]);

    let case = text(example("uas-calibration.json"))?;
    let calibration = cli(&["calibrate", &vehicle, "--case", &case])?;
    assert_eq!(calibration["consistent"], true);
    let prior = calibration["prior"]["residuals"]["flight.endurance"]["normalized"]
        .as_f64()
        .ok_or("missing prior residual")?;
    assert!(
        prior > 3.0,
        "the trial contradicts the uncalibrated prediction"
    );
    let flow = calibration["posterior"]["parameters"]["fuel.flow"]["value"]
        .as_f64()
        .ok_or("missing calibrated flow")?;
    assert!((flow - 12.0 / (9.1 * 3600.0)).abs() < 1e-9);
    Ok(())
}

#[test]
fn campaign_ranks_only_fleets_that_meet_the_mission() -> Result<(), Box<dyn Error>> {
    let campaign = text(example("persistent-surveillance.json"))?;
    let record = cli(&["campaign", &campaign])?;
    let outcomes = record["outcomes"].as_object().ok_or("missing outcomes")?;
    assert_eq!(outcomes.len(), 2);
    for outcome in outcomes.values() {
        assert_eq!(outcome["status"], "pass");
        assert_eq!(outcome["capability_approximate"], true);
    }
    assert_eq!(record["preferred"], "many_uas");
    let fleet = &record["campaign"]["fleets"]["many_uas"]["capability"];
    assert_eq!(fleet["vehicle_id"], "expendable_uas");
    assert!(fleet["evidence"]["unit_cost"]["run_id"].is_string());
    Ok(())
}

#[test]
fn comparison_keeps_each_vehicle_unit_verdict_and_provenance() -> Result<(), Box<dyn Error>> {
    let uas = text(example("expendable-uas.json"))?;
    let patrol = text(example("patrol-aircraft.json"))?;
    let result = cli(&[
        "compare",
        &uas,
        &patrol,
        "--metric",
        "cost.unit",
        "--metric",
        "flight.endurance",
    ])?;
    assert_eq!(
        result["metrics"],
        serde_json::json!(["cost.unit", "flight.endurance"])
    );
    let vehicles = result["vehicles"].as_array().ok_or("missing vehicles")?;
    assert_eq!(vehicles.len(), 2);
    for vehicle in vehicles {
        assert_eq!(vehicle["completed"], true);
        assert_eq!(vehicle["values"]["cost.unit"]["quantity"]["unit"], "USD");
        assert!(vehicle["values"]["cost.unit"]["run_id"].is_string());
        assert!(vehicle["margins"].is_object());
    }
    assert_eq!(
        vehicles[0]["values"]["flight.endurance"]["quantity"]["value"],
        10.0
    );
    let unknown = cargo_bin_cmd!("aex")
        .args(["compare", &uas, "--metric", "wing.span", "--format", "json"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let error: Value = serde_json::from_slice(&unknown.stdout)?;
    assert_eq!(error["error"]["code"], "UNKNOWN_COMPARISON_METRIC");
    Ok(())
}

#[test]
fn mcp_exposes_the_same_system_evaluations() -> Result<(), Box<dyn Error>> {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
            command.args(["mcp", "serve"]);
            let client = ().serve(TokioChildProcess::new(command)?).await?;
            let tools = client.list_all_tools().await?;
            for name in [
                "propagate_vehicle_uncertainty",
                "calibrate_vehicle",
                "evaluate_fleet_campaign",
                "compare_vehicles",
                "explain_vehicle_value",
                "edit_vehicle",
            ] {
                assert!(tools.iter().any(|tool| tool.name == name), "{name}");
            }
            let call = |name: &'static str, request: Value| {
                let client = &client;
                async move {
                    let arguments = request.as_object().cloned().ok_or("invalid arguments")?;
                    let result = client
                        .call_tool(CallToolRequestParams {
                            meta: None,
                            name: name.into(),
                            arguments: Some(arguments),
                            task: None,
                        })
                        .await?;
                    result
                        .structured_content
                        .ok_or_else(|| Box::<dyn Error>::from("missing structured result"))
                }
            };
            let campaign = call(
                "evaluate_fleet_campaign",
                serde_json::json!({"campaign_path": example("persistent-surveillance.json")}),
            )
            .await?;
            assert_eq!(campaign["preferred"], "many_uas");
            let comparison = call(
                "compare_vehicles",
                serde_json::json!({"vehicle_paths": [example("expendable-uas.json"),
                    example("patrol-aircraft.json")], "metrics": ["flight.endurance"]}),
            )
            .await?;
            assert_eq!(
                comparison["vehicles"][1]["values"]["flight.endurance"]["quantity"]["value"],
                24.0
            );
            let explanation = call(
                "explain_vehicle_value",
                serde_json::json!({"vehicle_path": example("expendable-uas.json"),
                    "parameter": "cost.unit"}),
            )
            .await?;
            assert_eq!(
                explanation["calculation"]["plugin"]["id"],
                "vehicle.production_cost"
            );
            let directory = tempfile::tempdir()?;
            let variant = directory.path().join("variant.json");
            let edited = call(
                "edit_vehicle",
                serde_json::json!({"vehicle_path": example("expendable-uas.json"),
                    "updates": {"build.quantity": "10 1"}, "expected_revision": 0,
                    "save_as": variant}),
            )
            .await?;
            assert_eq!(edited["revision"], 1);
            assert!(variant.exists());
            let calibration = call(
                "calibrate_vehicle",
                serde_json::json!({"vehicle_path": example("expendable-uas.json"),
                    "case_path": example("uas-calibration.json")}),
            )
            .await?;
            assert_eq!(calibration["consistent"], true);
            let failure = client
                .call_tool(CallToolRequestParams {
                    meta: None,
                    name: "propagate_vehicle_uncertainty".into(),
                    arguments: serde_json::json!({"vehicle_path": example("expendable-uas.json"),
                        "plan_path": example("missing-plan.json")})
                    .as_object()
                    .cloned(),
                    task: None,
                })
                .await?;
            assert_eq!(failure.is_error, Some(true));
            client.cancel().await?;
            Ok(())
        })
}

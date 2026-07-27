use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::{Map, Value, json};

use super::{arguments, scenario};

#[tokio::test]
async fn default_mission_is_compact_and_retrieves_full_evidence() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    command.current_dir(temporary.path());
    let client = ().serve(TokioChildProcess::new(command)?).await?;

    let compact = call_mission(&client, false).await?;
    let encoded = serde_json::to_vec(&compact)?;
    assert!(
        encoded.len() < 8 * 1024,
        "compact B777 response was {} bytes",
        encoded.len()
    );
    assert_eq!(compact["completion_status"], "completed");
    assert_eq!(compact["completed"], true);
    assert_eq!(compact["hard_requirements_passed"], true);
    assert!(compact["totals"].is_object());
    assert!(
        compact["segments"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(compact["diagnostics"].is_array());
    assert!(compact.get("assumptions").is_none());

    let report = retrieve(&client, &compact).await?;
    let detail = &report["result"];
    assert!(
        detail["assumptions"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert_eq!(detail["hard_requirements_passed"], true);
    assert_eq!(
        compact["totals"]["distance"]["value"],
        detail["total_distance"]["value"]
    );
    assert_eq!(
        compact["totals"]["fuel_burn_kg"]["value"],
        detail["total_fuel_burn_kg"]["value"]
    );
    assert_eq!(run_count(temporary.path())?, 1);
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn detail_mode_matches_the_retrieved_immutable_result() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    command.current_dir(temporary.path());
    let client = ().serve(TokioChildProcess::new(command)?).await?;

    let mut detailed = call_mission(&client, true).await?;
    assert!(
        detailed["assumptions"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(detailed["total_distance"].is_object());
    assert_eq!(
        detailed["mass_properties"]["statement"]["closure_error_kg"]["value"],
        0.0
    );
    assert!(
        detailed["mass_properties"]["states"]
            .as_array()
            .is_some_and(|states| !states.is_empty())
    );
    let report = retrieve(&client, &detailed).await?;
    let fields = detailed
        .as_object_mut()
        .ok_or_else(|| io::Error::other("detailed result was not an object"))?;
    fields.remove("run_id");
    fields.remove("retrieval");
    assert_eq!(detailed, report["result"]);
    assert_eq!(run_count(temporary.path())?, 1);
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_display_units_do_not_persist_a_run() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    command.current_dir(temporary.path());
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "simulate_mission".into(),
            arguments: Some(arguments(json!({
                "scenario_path": scenario("b777"),
                "units": "invalid"
            }))?),
            task: None,
        })
        .await?;
    assert_eq!(result.is_error, Some(true));
    assert!(!temporary.path().join("runs").exists());
    client.cancel().await?;
    Ok(())
}

async fn call_mission(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    detail: bool,
) -> Result<Value, Box<dyn Error>> {
    call(
        client,
        "simulate_mission",
        arguments(json!({
            "scenario_path": scenario("b777"),
            "detail": detail,
            "overrides": {}
        }))?,
    )
    .await
}

async fn retrieve(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    response: &Value,
) -> Result<Value, Box<dyn Error>> {
    let retrieval = response["retrieval"]["arguments"]
        .as_object()
        .cloned()
        .ok_or_else(|| io::Error::other("missing retrieval arguments"))?;
    call(client, "generate_report", retrieval).await
}

async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    arguments: Map<String, Value>,
) -> Result<Value, Box<dyn Error>> {
    client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: name.to_owned().into(),
            arguments: Some(arguments),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other(format!("{name} omitted structured content")).into())
}

fn run_count(root: &Path) -> Result<usize, io::Error> {
    fs::read_dir(root.join("runs"))?
        .collect::<Result<Vec<_>, _>>()
        .map(|entries| entries.len())
}

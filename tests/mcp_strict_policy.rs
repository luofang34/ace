//! MCP and CLI analysis adapters share the typed strict-warning policy.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::ServiceError;
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};

fn copy_atypical_c172(destination: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172");
    fs::create_dir_all(destination.join("profiles"))?;
    for relative in [
        "aircraft.yaml",
        "mission.yaml",
        "requirements.yaml",
        "scenario.yaml",
        "profiles/propeller.yaml",
    ] {
        fs::copy(source.join(relative), destination.join(relative))?;
    }
    let engine = fs::read_to_string(source.join("profiles/engine.yaml"))?
        .replace("rated_altitude: 0 ft", "rated_altitude: -1000 m");
    fs::write(destination.join("profiles/engine.yaml"), engine)?;
    Ok(destination.join("scenario.yaml"))
}

fn stock_c172() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/c172/scenario.yaml")
        .display()
        .to_string()
}

fn arguments(value: Value) -> Result<serde_json::Map<String, Value>, io::Error> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| io::Error::other("tool arguments must be an object"))
}

async fn call_tool(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    value: Value,
) -> Result<Value, Box<dyn Error>> {
    let result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: name.to_owned().into(),
            arguments: Some(arguments(value)?),
            task: None,
        })
        .await?;
    result
        .structured_content
        .ok_or_else(|| io::Error::other("tool omitted structured content").into())
}

async fn call_tool_error(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    value: Value,
) -> Result<Value, Box<dyn Error>> {
    let request = CallToolRequestParams {
        meta: None,
        name: name.to_owned().into(),
        arguments: Some(arguments(value)?),
        task: None,
    };
    match client.call_tool(request).await {
        Err(ServiceError::McpError(error)) => error
            .data
            .ok_or_else(|| io::Error::other("strict failure omitted structured data").into()),
        Err(error) => Err(io::Error::other(format!("unexpected MCP failure: {error}")).into()),
        Ok(_) => Err(io::Error::other("strict request unexpectedly succeeded").into()),
    }
}

#[tokio::test]
async fn mcp_promotes_the_same_warning_and_keeps_advisories() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let atypical = copy_atypical_c172(&temporary.path().join("atypical"))?;
    let atypical_path = atypical.display().to_string();
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    let client = ().serve(TokioChildProcess::new(command)?).await?;

    let advisory = call_tool(
        &client,
        "resolve_scenario",
        json!({"scenario_path": stock_c172(), "strict": true}),
    )
    .await?;
    assert!(advisory["warnings"].as_array().is_some_and(|warnings| {
        warnings
            .iter()
            .any(|warning| warning["code"] == "LOW_FIDELITY_MODEL")
    }));

    let non_strict = call_tool(
        &client,
        "resolve_scenario",
        json!({"scenario_path": atypical_path, "strict": false}),
    )
    .await?;
    assert!(non_strict["warnings"].as_array().is_some_and(|warnings| {
        warnings
            .iter()
            .any(|warning| warning["code"] == "PARAMETER_OUTSIDE_TYPICAL")
    }));

    for tool in ["resolve_scenario", "simulate_mission"] {
        let detail = call_tool_error(
            &client,
            tool,
            json!({"scenario_path": atypical_path, "strict": true}),
        )
        .await?;
        assert_eq!(detail["code"], "STRICT_WARNING_FAILURE");
        assert!(
            detail["message"]
                .as_str()
                .is_some_and(|message| message.contains("PARAMETER_OUTSIDE_TYPICAL"))
        );
    }
    client.cancel().await?;
    Ok(())
}

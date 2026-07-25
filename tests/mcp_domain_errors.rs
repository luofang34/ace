//! MCP model-domain failures retain the public structured error detail.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::io;
use std::path::PathBuf;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::ServiceError;
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};

fn scenario() -> String {
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

async fn domain_error(
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
            .ok_or_else(|| io::Error::other("MCP error omitted structured data").into()),
        Err(error) => Err(io::Error::other(format!("unexpected MCP failure: {error}")).into()),
        Ok(_) => Err(io::Error::other("tool unexpectedly succeeded").into()),
    }
}

#[tokio::test]
async fn resolve_point_and_mission_preserve_model_domain_violations() -> Result<(), Box<dyn Error>>
{
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let scenario_path = scenario();
    let requests = [
        (
            "resolve_scenario",
            json!({
                "scenario_path": scenario_path,
                "overrides": {"mission.segments.cruise.true_airspeed": "400 m/s"}
            }),
        ),
        (
            "calculate_point_performance",
            json!({
                "scenario_path": scenario_path,
                "condition": {"altitude": "0 m", "true_airspeed": "400 m/s"}
            }),
        ),
        (
            "simulate_mission",
            json!({
                "scenario_path": scenario_path,
                "overrides": {"mission.segments.cruise.true_airspeed": "400 m/s"}
            }),
        ),
    ];

    for (name, request) in requests {
        let detail = domain_error(&client, name, request).await?;
        assert_eq!(detail["code"], "MODEL_DOMAIN_UNSUPPORTED");
        assert!(detail["path"].is_string());
        assert!(
            detail["context"]["violations"]
                .as_array()
                .is_some_and(|violations| violations.iter().all(|violation| {
                    violation["model_id"].is_string()
                        && violation["minimum"].is_number()
                        && violation["maximum"].is_number()
                        && violation["bound_unit"].is_string()
                        && violation["basis"].is_string()
                }))
        );
    }
    client.cancel().await?;
    Ok(())
}

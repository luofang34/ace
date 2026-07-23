//! Public stdio MCP discovery and structured-response integration test.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::io;
use std::path::PathBuf;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::json;

fn scenario(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("scenario.yaml")
        .display()
        .to_string()
}

fn arguments(
    value: serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, io::Error> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| io::Error::other("tool arguments must be an object"))
}

#[tokio::test]
async fn lists_and_invokes_structured_mcp_tools() -> Result<(), Box<dyn Error>> {
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let tools = client.list_all_tools().await?;
    assert!(tools.iter().any(|tool| tool.name == "simulate_mission"));
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_payload_range")
    );
    assert!(tools.len() >= 12);

    let result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "simulate_mission".into(),
            arguments: Some(arguments(json!({
                "scenario_path": scenario("c172"),
                "overrides": {}
            }))?),
            task: None,
        })
        .await?;
    assert_eq!(result.is_error, Some(false));
    let structured = result
        .structured_content
        .ok_or_else(|| io::Error::other("missing structured MCP result"))?;
    assert_eq!(structured["completed"], true);
    assert_eq!(structured["total_distance"]["display_unit"], "nmi");
    client.cancel().await?;
    Ok(())
}

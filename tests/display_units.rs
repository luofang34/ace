//! Public CLI and MCP display-unit consistency tests.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::io;
use std::path::PathBuf;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::json;

fn scenario() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/c172/scenario.yaml")
        .display()
        .to_string()
}

#[tokio::test]
async fn cli_and_mcp_point_display_metadata_match() -> Result<(), Box<dyn Error>> {
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let arguments = json!({
        "scenario_path": scenario(),
        "condition": {
            "altitude": "8000 ft",
            "true_airspeed": "115 kt",
            "mass": "2400 lb",
            "configuration": "clean"
        },
        "overrides": {}
    })
    .as_object()
    .cloned()
    .ok_or_else(|| io::Error::other("point arguments must be an object"))?;
    let mut si_arguments = arguments.clone();
    si_arguments.insert("units".to_owned(), json!("si"));
    let mcp = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "calculate_point_performance".into(),
            arguments: Some(arguments),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing point result"))?;
    let mcp_si = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "calculate_point_performance".into(),
            arguments: Some(si_arguments),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing SI point result"))?;
    let directory = tempfile::tempdir()?;
    let output = std::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"))
        .current_dir(directory.path())
        .args([
            "analyze",
            "point",
            &scenario(),
            "--altitude",
            "8000 ft",
            "--speed",
            "115 kt",
            "--mass",
            "2400 lb",
            "--format",
            "json",
        ])
        .output()?;
    assert!(output.status.success());
    let cli: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    for field in ["altitude_m", "true_airspeed_m_s", "mass_kg"] {
        assert_eq!(mcp[field], cli["result"][field]);
    }
    assert_eq!(mcp["altitude_m"]["display_unit"], "ft");
    assert_eq!(mcp["true_airspeed_m_s"]["display_unit"], "kt");
    assert_eq!(mcp["mass_kg"]["display_unit"], "lb");
    assert_eq!(mcp_si["altitude_m"]["display_unit"], "m");
    assert_eq!(mcp_si["true_airspeed_m_s"]["display_unit"], "m/s");
    assert_eq!(mcp_si["mass_kg"]["display_unit"], "kg");
    client.cancel().await?;
    Ok(())
}

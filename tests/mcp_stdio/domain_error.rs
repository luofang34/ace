use std::error::Error;
use std::io;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::json;

use super::{arguments, scenario};

#[tokio::test]
async fn sr71_domain_failure_is_an_actionable_tool_result() -> Result<(), Box<dyn Error>> {
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "simulate_mission".into(),
            arguments: Some(arguments(json!({"scenario_path": scenario("sr71")}))?),
            task: None,
        })
        .await?;

    assert_eq!(result.is_error, Some(true));
    let detail = result
        .structured_content
        .ok_or_else(|| io::Error::other("missing structured domain failure"))?;
    assert_eq!(detail["status"], "error");
    assert_eq!(detail["code"], "MODEL_DOMAIN_UNSUPPORTED");
    let atmosphere = detail["diagnostics"]
        .as_array()
        .and_then(|diagnostics| {
            diagnostics.iter().find(|diagnostic| {
                diagnostic["valid_range"]["model_id"] == "atmosphere.isa1976"
                    && diagnostic["violating_path"] == "mission.segments.supersonic_cruise.altitude"
            })
        })
        .ok_or_else(|| io::Error::other("missing atmosphere diagnostic"))?;
    assert_eq!(atmosphere["valid_range"]["maximum"], 20_000.0);
    assert_eq!(atmosphere["suggested_override"]["value"], "20000 m");
    client.cancel().await?;
    Ok(())
}

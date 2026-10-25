use super::{call_tool, scenario};
use rmcp::service::RunningService;
use serde_json::json;
use std::error::Error;

type Client = RunningService<rmcp::RoleClient, ()>;

pub(super) async fn run(client: &Client) -> Result<(), Box<dyn Error>> {
    reports(client).await
}

async fn reports(client: &Client) -> Result<(), Box<dyn Error>> {
    let backends = call_tool(client, "list_analysis_backends", json!({})).await?;
    assert_eq!(backends["backends"][0]["id"], "native");
    assert_eq!(backends["backends"][0]["available"], true);
    assert_eq!(backends["backends"][1]["available"], false);
    let report = call_tool(
        client,
        "generate_report",
        json!({
            "scenario_path": scenario("c172"), "backend": "native", "format": "json", "sections": []
        }),
    )
    .await?;
    assert_eq!(report["report"]["charts"].as_array().map(Vec::len), Some(7));
    assert_eq!(report["report"]["charts"][0]["order"], 1);
    Ok(())
}

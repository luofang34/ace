//! Public stdio MCP discovery and structured-response integration test.

#![cfg(test)]

use std::error::Error;
use std::io;
use std::path::PathBuf;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::json;

#[path = "mcp_stdio/concept_flow.rs"]
mod concept_flow;
#[path = "mcp_stdio/domain_error.rs"]
mod domain_error;
#[path = "mcp_stdio/report_sources.rs"]
mod report_sources;

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

#[test]
fn lists_and_invokes_structured_mcp_tools() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let temporary = tempfile::tempdir()?;
            let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
            command.args(["mcp", "serve"]);
            command.current_dir(temporary.path());
            command.env(
                "ACE_OPENVSP_EXECUTABLE",
                "/path/that/does/not/contain/vspscript",
            );
            let client = ().serve(TokioChildProcess::new(command)?).await?;
            let tools = client.list_all_tools().await?;
            // Scenario constraint and payload-range tools are replaced by vehicle analyses.
            assert!(!tools.iter().any(|tool| matches!(
                tool.name.as_ref(),
                "generate_payload_range" | "generate_constraint_diagram"
            )));
            for expected in [
                "synthesize_vehicle",
                "study_vehicle",
                "edit_vehicle",
                "promote_vehicle_candidate",
                "compare_vehicles",
                "vehicle_payload_range",
                "chart_vehicle_analysis",
                "vehicle_catalog",
                "list_analysis_backends",
            ] {
                assert!(tools.iter().any(|tool| tool.name == expected));
            }
            // Scenario design studies are replaced by vehicle studies and promotion.
            for retired in [
                "load_design_study",
                "run_design_study",
                "query_design_study",
                "promote_study_candidate",
            ] {
                assert!(!tools.iter().any(|tool| tool.name == retired), "{retired}");
            }

            concept_flow::run(&client).await?;
            client.cancel().await?;
            Ok(())
        })
}

#[test]
fn vehicle_study_tools_archive_and_promote_one_candidate() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let temporary = tempfile::tempdir()?;
            let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
            command.args(["mcp", "serve"]);
            command.current_dir(temporary.path());
            let client = ().serve(TokioChildProcess::new(command)?).await?;
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172");
            let archive = call_tool(
                &client,
                "study_vehicle",
                json!({
                    "vehicle_path": root.join("vehicle.json"),
                    "plan_path": root.join("vehicle-study.json")
                }),
            )
            .await?;
            assert_eq!(archive["candidates"].as_array().map(Vec::len), Some(81));
            let candidate = archive["provisional_pareto"][0]
                .as_str()
                .ok_or_else(|| io::Error::other("missing Pareto candidate"))?
                .to_owned();
            let archive_path = temporary.path().join("archive.json");
            std::fs::write(&archive_path, serde_json::to_vec(&archive)?)?;
            let save_as = temporary.path().join("selected.json");
            let promoted = call_tool(
                &client,
                "promote_vehicle_candidate",
                json!({
                    "archive_path": archive_path,
                    "candidate_id": candidate,
                    "save_as": save_as
                }),
            )
            .await?;
            assert!(save_as.is_file(), "{promoted}");
            client.cancel().await?;
            Ok(())
        })
}

async fn call_tool(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    value: serde_json::Value,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: name.to_owned().into(),
            arguments: Some(arguments(value)?),
            task: None,
        })
        .await?;
    assert_eq!(result.is_error, Some(false));
    result
        .structured_content
        .ok_or_else(|| io::Error::other(format!("missing {name} result")).into())
}

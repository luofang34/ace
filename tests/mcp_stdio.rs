//! Public stdio MCP discovery and structured-response integration test.

#![cfg(test)]

use std::error::Error;
use std::io;
use std::path::PathBuf;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::json;

#[path = "mcp_stdio/backend_preflight.rs"]
mod backend_preflight;
#[path = "mcp_stdio/concept_flow.rs"]
mod concept_flow;
#[path = "mcp_stdio/domain_error.rs"]
mod domain_error;

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
            assert!(
                tools
                    .iter()
                    .any(|tool| tool.name == "generate_payload_range")
            );
            for expected in [
                "synthesize_vehicle",
                "study_vehicle",
                "edit_vehicle",
                "promote_vehicle_candidate",
                "compare_vehicles",
                "vehicle_payload_range",
                "list_analysis_backends",
                "load_design_study",
                "run_design_study",
                "query_design_study",
                "promote_study_candidate",
            ] {
                assert!(tools.iter().any(|tool| tool.name == expected));
            }
            assert!(tools.len() >= 18);

            concept_flow::run(&client).await?;
            client.cancel().await?;
            Ok(())
        })
}

#[test]
fn study_tools_reuse_evidence_and_promote_one_design() -> Result<(), Box<dyn Error>> {
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
            let executed = run_study_tools(&client, temporary.path()).await?;
            query_and_promote_study(&client, temporary.path(), &executed).await?;
            client.cancel().await?;
            Ok(())
        })
}

struct ExecutedStudy {
    study_path: PathBuf,
    candidate_id: String,
    evaluation_id: String,
    archive_id: String,
}

async fn run_study_tools(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    root: &std::path::Path,
) -> Result<ExecutedStudy, Box<dyn Error>> {
    let study_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/study.yaml");
    let chart_path = root.join("trade.svg");
    let loaded = call_tool(
        client,
        "load_design_study",
        json!({ "study_path": study_path }),
    )
    .await?;
    assert_eq!(loaded["study_id"], "c172-local-design-space");
    assert_eq!(loaded["variable_count"], 4);

    let first = call_tool(
        client,
        "run_design_study",
        json!({
            "study_path": study_path,
            "artifact_path": chart_path
        }),
    )
    .await?;
    assert_eq!(first["result"]["evaluated_candidates"], 81);
    assert_eq!(first["result"]["complete"], true);
    assert!(first["chart_spec"].is_object());
    assert!(chart_path.is_file());
    let candidate_id = first["result"]["selected_candidates"][0]["candidate"]["candidate_id"]
        .as_str()
        .ok_or_else(|| io::Error::other("missing selected candidate id"))?
        .to_owned();
    let evaluation_id = first["result"]["selected_candidates"][0]["evidence_id"]
        .as_str()
        .ok_or_else(|| io::Error::other("missing selected evidence id"))?
        .to_owned();
    let archive_id = first["result"]["archive_id"]
        .as_str()
        .ok_or_else(|| io::Error::other("missing archive id"))?
        .to_owned();

    let second = call_tool(
        client,
        "run_design_study",
        json!({ "study_path": study_path }),
    )
    .await?;
    assert_eq!(
        first["result"]["archive_id"],
        second["result"]["archive_id"]
    );
    assert_eq!(second["result"]["reused_evaluations"], 81);
    Ok(ExecutedStudy {
        study_path,
        candidate_id,
        evaluation_id,
        archive_id,
    })
}

async fn query_and_promote_study(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    root: &std::path::Path,
    executed: &ExecutedStudy,
) -> Result<(), Box<dyn Error>> {
    let queried = call_tool(
        client,
        "query_design_study",
        json!({
            "study_id": "c172-local-design-space",
            "archive_id": executed.archive_id,
            "limit": 3
        }),
    )
    .await?;
    assert_eq!(queried["result"]["archive_id"], executed.archive_id);
    assert!(queried["chart_spec"].is_object());
    let evidence = call_tool(
        client,
        "query_design_study",
        json!({
            "study_id": "c172-local-design-space",
            "archive_id": executed.archive_id,
            "candidate_id": &executed.candidate_id
        }),
    )
    .await?;
    assert_eq!(evidence["evaluation_id"], executed.evaluation_id);

    let design_root = root.join("designs");
    let promoted = call_tool(
        client,
        "promote_study_candidate",
        json!({
            "study_path": &executed.study_path,
            "candidate_id": &executed.candidate_id,
            "design_id": "mcp-study-selection",
            "display_name": "MCP Study Selection",
            "design_root": design_root
        }),
    )
    .await?;
    assert_eq!(promoted["source_candidate_id"], executed.candidate_id);
    assert!(
        design_root
            .join("mcp-study-selection/scenario.yaml")
            .is_file()
    );
    assert!(!root.join(".ace/studies/candidates").exists());
    Ok(())
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

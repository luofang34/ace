//! Public stdio MCP discovery and structured-response integration test.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::io;
use std::path::PathBuf;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::json;

#[path = "mcp_stdio/compact_mission.rs"]
mod compact_mission;
#[path = "mcp_stdio/fixtures.rs"]
mod fixtures;

use fixtures::{fuel_exhaustion_scenario, no_cruise_scenario};

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
    assert!(tools.iter().any(|tool| tool.name == "simulate_mission"));
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "generate_payload_range")
    );
    for expected in [
        "create_design",
        "update_design_parameters",
        "evaluate_feasibility",
        "auto_refine_design",
        "run_parameter_sweep",
        "compare_designs",
        "list_analysis_backends",
        "load_design_study",
        "run_design_study",
        "query_design_study",
        "promote_study_candidate",
    ] {
        assert!(tools.iter().any(|tool| tool.name == expected));
    }
    assert!(tools.len() >= 18);

    let result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "simulate_mission".into(),
            arguments: Some(arguments(json!({
                "scenario_path": scenario("c172"),
                "detail": true,
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
    assert_eq!(structured["landing_fuel"]["unit"], "kg");
    assert_eq!(structured["landing_fuel"]["display_unit"], "lb");
    let backends = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "list_analysis_backends".into(),
            arguments: Some(arguments(json!({}))?),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing backend result"))?;
    assert_eq!(backends["backends"][0]["id"], "native");
    assert_eq!(backends["backends"][0]["available"], true);
    assert_eq!(backends["backends"][1]["available"], false);

    let report = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "generate_report".into(),
            arguments: Some(arguments(json!({
                "scenario_path": scenario("c172"),
                "backend": "native",
                "format": "json",
                "sections": []
            }))?),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing concept report"))?;
    assert_eq!(report["report"]["charts"].as_array().map(Vec::len), Some(7));
    assert_eq!(report["report"]["charts"][0]["order"], 1);

    let design_root = tempfile::tempdir()?;
    let created = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "create_design".into(),
            arguments: Some(arguments(json!({
                "design_id": "mcp_c172_experiment",
                "display_name": "MCP C172 Experiment",
                "design_root": design_root.path(),
                "baseline": "c172",
                "parameters": {
                    "aircraft.geometry.wing.aspect_ratio": "8.1",
                    "aircraft.geometry.wing.span": format!("{} m", (16.17_f64 * 8.1).sqrt())
                }
            }))?),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing create_design result"))?;
    let scenario_path = created["design"]["scenario_path"]
        .as_str()
        .ok_or_else(|| io::Error::other("missing scenario_path"))?;
    let updated = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "update_design_parameters".into(),
            arguments: Some(arguments(json!({
                "scenario_path": scenario_path,
                "updates": {
                    "aircraft.geometry.wing.area": "17.2 m^2",
                    "aircraft.geometry.wing.span": format!("{} m", (17.2_f64 * 8.1).sqrt()),
                    "aircraft.aerodynamics.clean.oswald_efficiency": "0.82"
                }
            }))?),
            task: None,
        })
        .await?;
    assert_eq!(updated.is_error, Some(false));

    let feasibility = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "evaluate_feasibility".into(),
            arguments: Some(arguments(json!({
                "scenario_path": scenario_path,
                "backend": "native"
            }))?),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing feasibility result"))?;
    assert!(feasibility["feasible"].is_boolean());
    assert_eq!(
        feasibility["baseline"]["analysis"]["provenance"]["backend"],
        "native"
    );
    assert!(feasibility["baseline"]["analysis"]["metrics"].is_object());
    assert_eq!(feasibility["refinement"], serde_json::Value::Null);

    let sweep = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "run_parameter_sweep".into(),
            arguments: Some(arguments(json!({
                "scenario_path": scenario_path,
                "variables": [{
                    "path": "aircraft.geometry.wing.aspect_ratio",
                    "values": ["7.5", "8.5"]
                }],
                "metrics": [
                    "aerodynamics.maximum_lift_to_drag_ratio",
                    "feasibility.hard_constraints_passed"
                ]
            }))?),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing sweep result"))?;
    assert_eq!(sweep["result"]["rows"].as_array().map(Vec::len), Some(2));
    assert_eq!(sweep["result"]["provenance"]["backend"], "native");
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn mission_verdict_is_completion_gated_in_mcp() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    command.current_dir(temporary.path());
    let client = ().serve(TokioChildProcess::new(command)?).await?;

    let incomplete = fuel_exhaustion_scenario(&temporary.path().join("incomplete"))?;
    for (fixture, completed, hard_passed) in
        [(scenario("c172"), true, true), (incomplete, false, false)]
    {
        let result = call_tool(
            &client,
            "simulate_mission",
            json!({
                "scenario_path": fixture,
                "detail": true,
                "overrides": {}
            }),
        )
        .await?;
        assert_eq!(result["completed"], completed);
        assert_eq!(result["hard_requirements_passed"], hard_passed);
        assert_eq!(result.get("landing_fuel").is_some(), completed);
        if !completed {
            assert!(result["warnings"].as_array().is_some_and(|warnings| {
                warnings
                    .iter()
                    .all(|warning| warning["code"] != "LOW_LANDING_FUEL")
            }));
        }
    }

    let no_cruise = no_cruise_scenario(&temporary.path().join("no-cruise"))?;
    let result = call_tool(
        &client,
        "simulate_mission",
        json!({ "scenario_path": no_cruise, "detail": true, "overrides": {} }),
    )
    .await?;
    assert_eq!(result["completed"], true);
    assert_eq!(result["hard_requirements_passed"], false);
    assert!(
        result["segments"]
            .as_array()
            .is_some_and(|segments| !segments.is_empty())
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn study_tools_reuse_evidence_and_promote_one_design() -> Result<(), Box<dyn Error>> {
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

#[tokio::test]
async fn unavailable_openvsp_preflights_before_mutation() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let design_root = temporary.path().join("designs");
    let artifact = temporary.path().join("rejected.vsp3");
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    command.env(
        "ACE_OPENVSP_EXECUTABLE",
        "/path/that/does/not/contain/vspscript",
    );
    let client = ().serve(TokioChildProcess::new(command)?).await?;

    let response = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "auto_refine_design".into(),
            arguments: Some(arguments(json!({
                "scenario_path": scenario("c172"),
                "output_design_id": "must_not_exist",
                "display_name": "Rejected Design",
                "design_root": design_root,
                "backend": "openvsp",
                "artifact_path": artifact,
                "max_iterations": 12
            }))?),
            task: None,
        })
        .await?;

    assert_eq!(response.is_error, Some(true));
    assert_eq!(
        response
            .structured_content
            .as_ref()
            .map(|content| &content["code"]),
        Some(&serde_json::Value::String("BACKEND_UNAVAILABLE".to_owned()))
    );
    assert!(!design_root.exists());
    assert!(!artifact.exists());
    client.cancel().await?;
    Ok(())
}

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

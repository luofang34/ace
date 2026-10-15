use super::{call_tool, scenario};
use rmcp::service::RunningService;
use serde_json::json;
use std::{error::Error, io, path::Path};

type Client = RunningService<rmcp::RoleClient, ()>;

pub(super) async fn run(client: &Client) -> Result<(), Box<dyn Error>> {
    reports(client).await?;
    let directory = tempfile::tempdir()?;
    let path = create_and_edit(client, directory.path()).await?;
    let feasibility = call_tool(
        client,
        "evaluate_feasibility",
        json!({
            "scenario_path": path, "backend": "native"
        }),
    )
    .await?;
    assert!(feasibility["feasible"].is_boolean());
    assert_eq!(
        feasibility["baseline"]["analysis"]["provenance"]["backend"],
        "native"
    );
    assert!(feasibility["baseline"]["analysis"]["metrics"].is_object());
    assert_eq!(feasibility["refinement"], serde_json::Value::Null);
    sweep(client, &path).await
}

async fn reports(client: &Client) -> Result<(), Box<dyn Error>> {
    let mission = call_tool(
        client,
        "simulate_mission",
        json!({
            "scenario_path": scenario("c172"), "detail": true, "overrides": {}
        }),
    )
    .await?;
    assert_eq!(mission["completed"], true);
    assert_eq!(mission["total_distance"]["display_unit"], "nmi");
    assert_eq!(mission["landing_fuel"]["unit"], "kg");
    assert_eq!(mission["landing_fuel"]["display_unit"], "lb");
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

async fn create_and_edit(client: &Client, directory: &Path) -> Result<String, Box<dyn Error>> {
    let created = call_tool(
        client,
        "create_design",
        json!({
            "design_id": "mcp_c172_experiment", "display_name": "MCP C172 Experiment",
            "design_root": directory, "baseline": "c172",
            "parameters": {
                "aircraft.geometry.wing.aspect_ratio": "8.1",
                "aircraft.geometry.wing.span": format!("{} m", (16.17_f64 * 8.1).sqrt())
            }
        }),
    )
    .await?;
    let path = created["design"]["scenario_path"]
        .as_str()
        .ok_or_else(|| io::Error::other("missing scenario_path"))?
        .to_owned();
    call_tool(
        client,
        "update_design_parameters",
        json!({
            "scenario_path": path,
            "updates": {
                "aircraft.geometry.wing.area": "17.2 m^2",
                "aircraft.geometry.wing.span": format!("{} m", (17.2_f64 * 8.1).sqrt()),
                "aircraft.aerodynamics.clean.oswald_efficiency": "0.82"
            }
        }),
    )
    .await?;
    Ok(path)
}

async fn sweep(client: &Client, path: &str) -> Result<(), Box<dyn Error>> {
    let sweep = call_tool(client, "run_parameter_sweep", json!({
        "scenario_path": path,
        "variables": [
            {"path": "aircraft.geometry.wing.aspect_ratio", "values": ["7.5", "8.5"]},
            {"path": "aircraft.propulsion.sizing_factor", "values": ["1.0", "1.1"]}
        ],
        "metrics": ["aerodynamics.maximum_lift_to_drag_ratio", "feasibility.hard_constraints_passed"]
    })).await?;
    assert_eq!(sweep["result"]["rows"].as_array().map(Vec::len), Some(4));
    assert_eq!(sweep["result"]["provenance"]["backend"], "native");
    assert_eq!(sweep["chart_spec"]["chart_type"], "carpet");
    assert_eq!(
        sweep["chart_spec"]["surface"]["values"]
            .as_array()
            .map(Vec::len),
        Some(4)
    );
    assert_eq!(
        sweep["chart_spec"]["surface"]["feasible_mask"]
            .as_array()
            .map(Vec::len),
        Some(4)
    );
    Ok(())
}

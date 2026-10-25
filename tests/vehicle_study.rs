//! Vehicle studies reuse the same model, solvers, constraints, and sheet evidence.
#![cfg(test)]

use ace_study::{Feasibility, StopReason, StudyArchive};
use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::json;
use std::error::Error;
use std::path::PathBuf;

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/synthesis")
        .join(name)
}

fn run_blocking() -> Result<StudyArchive, Box<dyn Error>> {
    let output = cargo_bin_cmd!("aex")
        .arg("study")
        .arg(example("transport.json"))
        .arg("--plan")
        .arg(example("span-study.json"))
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn verify(archive: &StudyArchive) -> Result<(), Box<dyn Error>> {
    assert_eq!(archive.candidates.len(), 4);
    assert_eq!(archive.stop_reason, StopReason::GridExhausted);
    assert!(archive.pareto.is_empty());
    assert_eq!(archive.provisional_pareto.len(), 1);
    let best = archive
        .candidates
        .iter()
        .find(|c| archive.provisional_pareto.contains(&c.id))
        .ok_or("missing provisional optimum")?;
    assert_eq!(best.updates["wing.span"].value, 36.0);
    for candidate in &archive.candidates {
        let snapshot = archive.snapshot(&candidate.id)?;
        assert!(snapshot.complete());
        assert!(snapshot.closures["mass_fuel"].converged);
        assert!(
            snapshot
                .executed
                .iter()
                .any(|node| node == "analysis:fuel_requirement")
        );
        assert!(!snapshot.executed.iter().any(|node| node == "analysis:taxi"));
        let sheet = ace_sheet::project(&candidate.model, &snapshot)?;
        assert!(
            sheet
                .panels
                .iter()
                .any(|panel| panel.plugin == "vehicle.mission_sequence")
        );
        assert_eq!(sheet.snapshot.model_digest, candidate.state.model_digest);
    }
    let rejected = archive
        .candidates
        .iter()
        .find(|c| c.updates["wing.span"].value == 37.0)
        .ok_or("missing sample")?;
    assert_eq!(rejected.feasibility, Feasibility::Violated);
    assert_eq!(
        rejected.margins["gate_span"]
            .absolute
            .ok_or("missing margin")?
            .value,
        -1.0
    );
    assert_eq!(
        rejected.margins["remaining_fuel"].status,
        ace_engine::MarginStatus::Warn
    );
    Ok(())
}

#[test]
fn vehicle_grid_preserves_closure_provenance_and_candidate_sheet_projection()
-> Result<(), Box<dyn Error>> {
    verify(&run_blocking()?)
}

#[test]
fn cli_refinement_honors_its_budget_and_invalid_plans_are_errors() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("refine.json");
    std::fs::write(
        &path,
        serde_json::to_vec(
            &json!({"schema_version":1,"max_evaluations":3,"method":{"kind":"refine","levels":5}}),
        )?,
    )?;
    let output = cargo_bin_cmd!("aex")
        .arg("study")
        .arg(example("transport.json"))
        .arg("--plan")
        .arg(&path)
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let archive: StudyArchive = serde_json::from_slice(&output.stdout)?;
    assert_eq!(archive.candidates.len(), 3);
    assert_eq!(archive.stop_reason, StopReason::EvaluationBudget);
    std::fs::write(
        &path,
        serde_json::to_vec(
            &json!({"schema_version":1,"max_evaluations":0,"method":{"kind":"refine","levels":5}}),
        )?,
    )?;
    cargo_bin_cmd!("aex")
        .arg("study")
        .arg(example("transport.json"))
        .arg("--plan")
        .arg(&path)
        .args(["--format", "json"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("VEHICLE_STUDY_ERROR"));
    Ok(())
}

#[test]
fn mcp_vehicle_study_matches_cli_and_preserves_hashes() -> Result<(), Box<dyn Error>> {
    use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
    let cli = run_blocking()?;
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let mut command=tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));command.args(["mcp","serve"]);
        let client=().serve(TokioChildProcess::new(command)?).await?;
        let request=json!({"vehicle_path":example("transport.json"),"plan_path":example("span-study.json")});
        let result=client.call_tool(CallToolRequestParams {meta:None,name:"study_vehicle".into(),
            arguments:request.as_object().cloned(),task:None}).await?;
        let archive:StudyArchive=serde_json::from_value(result.structured_content.ok_or("missing study result")?)?;
        verify(&archive)?;assert_eq!(archive.id,cli.id);
        client.cancel().await?;
        Ok(())
    })
}

#[test]
fn generated_two_input_grids_chart_an_output_over_the_complete_grid() -> Result<(), Box<dyn Error>>
{
    let directory = tempfile::tempdir()?;
    let plan = directory.path().join("grid.json");
    let artifact = directory.path().join("casm.svg");
    std::fs::write(
        &plan,
        serde_json::to_vec(
            &json!({"schema_version":1,"max_evaluations":4,"method":{"kind":"grid","axes":{
            "wing.span":{"start":{"value":34,"unit":"m"},"stop":{"value":36,"unit":"m"},"count":2},
            "payload.mass":{"start":{"value":18000,"unit":"kg"},"stop":{"value":24000,"unit":"kg"},
                "count":2,"spacing":"logarithmic"}}}}),
        )?,
    )?;
    let output = cargo_bin_cmd!("aex")
        .arg("study")
        .arg(example("transport.json"))
        .arg("--plan")
        .arg(&plan)
        .args(["--chart", "economics.casm", "--artifact"])
        .arg(&artifact)
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["candidates"].as_array().map(Vec::len), Some(4));
    let spec = &result["chart"]["spec"];
    assert_eq!(spec["chart_type"], "carpet");
    assert_eq!(spec["x"]["label"], "payload.mass");
    assert_eq!(spec["y"]["values"], json!([34.0, 36.0]));
    assert_eq!(spec["surface"]["values"].as_array().map(Vec::len), Some(4));
    assert_eq!(spec["surface"]["unit"], "USD/nmi");
    assert!(std::fs::metadata(&artifact)?.len() > 100);
    Ok(())
}

//! Study candidates become editable vehicles only from verified, feasible evidence.
#![cfg(test)]

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use std::error::Error;
use std::path::Path;

fn text(path: &Path) -> Result<String, Box<dyn Error>> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "non-UTF-8 path".into())
}

/// Grid over span on a vehicle whose requirement fails above 13.3 m.
fn archive(directory: &Path) -> Result<(String, Value), Box<dyn Error>> {
    let source = json!({"kind":"test","reference":"promotion fixture"});
    let model = json!({"schema_version":1,"id":"promoted","name":"Promotion fixture","domain":"vehicle",
        "parameters":{
            "span":{"unit":"m","value":{"value":10,"unit":"m"},"source":source,"owner":"test"},
            "area":{"unit":"m^2","source":source,"owner":"test","expression":{"op":"multiply",
                "left":{"op":"reference","parameter":"span"},"right":{"op":"constant","value":{"value":3,"unit":"m"}}}}},
        "requirements":{"limit":{"name":"Area","metric":"area","relation":"le",
            "target":{"value":40,"unit":"m^2"},"priority":"hard","source":source}},
        "design_variables":[{"parameter":"span","lower":{"value":5,"unit":"m"},"upper":{"value":20,"unit":"m"}}]});
    let vehicle = directory.join("vehicle.json");
    std::fs::write(&vehicle, serde_json::to_vec(&model)?)?;
    let plan = directory.join("plan.json");
    std::fs::write(
        &plan,
        serde_json::to_vec(
            &json!({"schema_version":1,"max_evaluations":2,"method":{"kind":"grid",
            "axes":{"span":[{"value":12,"unit":"m"},{"value":15,"unit":"m"}]}}}),
        )?,
    )?;
    let archive = directory.join("archive.json");
    cargo_bin_cmd!("aex")
        .args([
            "study",
            &text(&vehicle)?,
            "--plan",
            &text(&plan)?,
            "--format",
            "json",
        ])
        .args(["--output", &text(&archive)?])
        .assert()
        .success();
    let value = serde_json::from_slice(&std::fs::read(&archive)?)?;
    Ok((text(&archive)?, value))
}

fn candidate(archive: &Value, span: f64) -> Result<String, Box<dyn Error>> {
    archive["candidates"]
        .as_array()
        .and_then(|candidates| {
            candidates
                .iter()
                .find(|candidate| candidate["updates"]["span"]["value"] == span)
        })
        .and_then(|candidate| candidate["id"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| "missing candidate".into())
}

fn promote(archive: &str, candidate: &str, target: &Path) -> Result<Value, Box<dyn Error>> {
    let output = cargo_bin_cmd!("aex")
        .args([
            "promote",
            archive,
            candidate,
            "--save-as",
            &text(target)?,
            "--format",
            "json",
        ])
        .output()?;
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn feasible_candidates_promote_to_new_vehicle_files() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let (path, archive) = archive(directory.path())?;
    let feasible = candidate(&archive, 12.0)?;
    let target = directory.path().join("chosen.json");
    let promotion = promote(&path, &feasible, &target)?;
    assert_eq!(promotion["candidate_id"], feasible.as_str());
    assert_eq!(promotion["updates"]["span"]["value"], 12.0);
    let saved: Value = serde_json::from_slice(&std::fs::read(&target)?)?;
    assert_eq!(saved["parameters"]["span"]["value"]["value"], 12.0);

    let again = promote(&path, &feasible, &target)?;
    assert!(again["error"]["code"].is_string());
    let rejected = promote(
        &path,
        &candidate(&archive, 15.0)?,
        &directory.path().join("no.json"),
    )?;
    assert_eq!(rejected["error"]["code"], "CANDIDATE_NOT_FEASIBLE");
    assert!(!directory.path().join("no.json").exists());
    Ok(())
}

#[test]
fn tampered_archives_are_rejected_before_writing() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let (path, mut archive) = archive(directory.path())?;
    let feasible = candidate(&archive, 12.0)?;
    archive["candidates"][0]["model"]["name"] = json!("Edited outside the study");
    archive["candidates"][1]["model"]["name"] = json!("Edited outside the study");
    std::fs::write(&path, serde_json::to_vec(&archive)?)?;
    let target = directory.path().join("chosen.json");
    let rejected = promote(&path, &feasible, &target)?;
    assert!(rejected["error"]["code"].is_string());
    assert!(!target.exists());
    Ok(())
}

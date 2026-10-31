//! Vehicle edits are validated, revision-checked revisions of the model file.
#![cfg(test)]

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use std::error::Error;
use std::path::{Path, PathBuf};

fn copy_example(directory: &Path) -> Result<String, Box<dyn Error>> {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/system/expendable-uas.json");
    let target = directory.join("vehicle.json");
    std::fs::copy(source, &target)?;
    target
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "non-UTF-8 path".into())
}

fn edit(arguments: &[&str]) -> assert_cmd::assert::Assert {
    cargo_bin_cmd!("aex")
        .arg("edit")
        .args(arguments)
        .args(["--format", "json"])
        .assert()
}

fn read(path: &str) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

#[test]
fn typed_edits_and_assignments_save_one_checked_revision() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = copy_example(directory.path())?;
    let edits = directory.path().join("edits.json");
    let requirement = json!([{"kind":"requirement","id":"cheap","value":{"name":"Cheap airframe",
        "metric":"cost.unit","relation":"le","target":{"value":1,"unit":"USD"},"priority":"soft",
        "source":{"kind":"test","reference":"edit test"}}}]);
    std::fs::write(&edits, serde_json::to_vec(&requirement)?)?;
    let output = edit(&[
        &path,
        "--edits",
        &edits.to_string_lossy(),
        "--set",
        "build.quantity=2000 1",
        "--expected-revision",
        "0",
    ])
    .success()
    .get_output()
    .clone();
    let outcome: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(outcome["revision"], 1);
    assert_eq!(outcome["margins"]["cheap"]["status"], "FAIL");
    assert!(
        outcome["affected"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node == "analysis:production"))
    );
    let saved = read(&path)?;
    assert_eq!(saved["revision"], 1);
    assert_eq!(
        saved["parameters"]["build.quantity"]["value"]["value"],
        2000.0
    );
    assert!(saved["requirements"]["cheap"].is_object());

    let conflict = edit(&[
        &path,
        "--set",
        "build.quantity=10 1",
        "--expected-revision",
        "0",
    ])
    .failure()
    .get_output()
    .clone();
    let error: Value = serde_json::from_slice(&conflict.stdout)?;
    assert_eq!(error["error"]["code"], "REVISION_CONFLICT");
    assert_eq!(read(&path)?["revision"], 1);
    Ok(())
}

#[test]
fn save_as_writes_a_new_vehicle_and_never_replaces_one() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = copy_example(directory.path())?;
    let original = std::fs::read(&path)?;
    let copy = directory.path().join("variant.json");
    let copy_text = copy.to_string_lossy().into_owned();
    edit(&[
        &path,
        "--set",
        "build.quantity=10 1",
        "--save-as",
        &copy_text,
    ])
    .success();
    assert_eq!(std::fs::read(&path)?, original);
    assert_eq!(
        read(&copy_text)?["parameters"]["build.quantity"]["value"]["value"],
        10.0
    );
    edit(&[
        &path,
        "--set",
        "build.quantity=20 1",
        "--save-as",
        &copy_text,
    ])
    .failure();
    assert_eq!(
        read(&copy_text)?["parameters"]["build.quantity"]["value"]["value"],
        10.0
    );
    Ok(())
}

//! Explanations trace a vehicle value through its frozen calculation evidence.
#![cfg(test)]

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use std::error::Error;
use std::path::Path;

fn reference(parameter: &str) -> Value {
    json!({"op":"reference","parameter":parameter})
}

fn chain_vehicle(directory: &Path) -> Result<String, Box<dyn Error>> {
    let source = json!({"kind":"test","reference":"explanation fixture"});
    let model = json!({"schema_version":1,"id":"explained","name":"Explanation fixture","domain":"vehicle",
        "parameters":{
            "span":{"unit":"m","value":{"value":10,"unit":"m"},"source":source,"owner":"test"},
            "chord":{"unit":"m","value":{"value":2,"unit":"m"},"source":source,"owner":"test"},
            "area":{"unit":"m^2","source":source,"owner":"test",
                "expression":{"op":"multiply","left":reference("span"),"right":reference("chord")}},
            "loading":{"unit":"m^2","source":source,"owner":"test",
                "expression":{"op":"multiply","left":reference("area"),"right":{"op":"constant","value":{"value":1.5,"unit":"1"}}}}},
        "requirements":{"limit":{"name":"Loading area","metric":"loading","relation":"le",
            "target":{"value":40,"unit":"m^2"},"priority":"hard","source":source}}});
    let path = directory.join("explained.json");
    std::fs::write(&path, serde_json::to_vec(&model)?)?;
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "non-UTF-8 path".into())
}

fn explain(path: &str, parameter: &str, depth: &str) -> Result<Value, Box<dyn Error>> {
    let output = cargo_bin_cmd!("aex")
        .args([
            "explain", path, parameter, "--depth", depth, "--format", "json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn explanations_follow_producers_to_the_requested_depth() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = chain_vehicle(directory.path())?;
    let loading = explain(&path, "loading", "2")?;
    assert_eq!(loading["value"]["quantity"]["value"], 30.0);
    assert_eq!(loading["calculation"]["node"], "expression:loading");
    assert_eq!(
        loading["calculation"]["inputs"]["area"]["quantity"]["value"],
        20.0
    );
    let upstream = loading["upstream"].as_array().ok_or("missing upstream")?;
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0]["parameter"], "area");
    assert_eq!(upstream[0]["depth"], 1);
    assert_eq!(loading["margins"]["limit"]["status"], "PASS");
    assert!(
        explain(&path, "loading", "0")?["upstream"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );

    let span = explain(&path, "span", "2")?;
    assert!(span["calculation"].is_null());
    assert_eq!(span["declaration"]["unit"], "m");
    assert_eq!(span["consumers"]["expressions"], json!(["area"]));
    Ok(())
}

#[test]
fn unknown_parameters_and_excessive_depth_are_structured_errors() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = chain_vehicle(directory.path())?;
    for (parameter, depth, code) in [
        ("missing", "1", "UNKNOWN_EXPLANATION_PARAMETER"),
        ("area", "17", "INVALID_EXPLANATION_DEPTH"),
    ] {
        let output = cargo_bin_cmd!("aex")
            .args([
                "explain", &path, parameter, "--depth", depth, "--format", "json",
            ])
            .assert()
            .failure()
            .get_output()
            .clone();
        let error: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(error["error"]["code"], code);
    }
    Ok(())
}

//! Mission phases and performance analyses see the same installed shaft power.
//!
//! Performance and constraint analyses multiply the rated power by the engine
//! count themselves; mission phases read `engine.shaft_power`. A vehicle whose
//! `engine.shaft_power` leaves the count out flies its mission on one engine.
#![cfg(test)]

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::error::Error;
use std::path::{Path, PathBuf};

fn propeller_vehicles() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut vehicles = Vec::new();
    for entry in std::fs::read_dir(examples)? {
        let path = entry?.path().join("vehicle.json");
        if !path.is_file() {
            continue;
        }
        let document: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        if document["parameters"]["engine.shaft_power"].is_object() {
            vehicles.push(path);
        }
    }
    vehicles.sort();
    Ok(vehicles)
}

fn synthesized_values(path: &Path, engine_count: u32) -> Result<Value, Box<dyn Error>> {
    let output = cargo_bin_cmd!("aex")
        .arg("synthesize")
        .arg(path)
        .args(["--set", &format!("engine.count={engine_count} 1")])
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let result: Value = serde_json::from_slice(&output.stdout)?;
    Ok(result["snapshot"]["values"].clone())
}

fn value(values: &Value, parameter: &str) -> Result<f64, Box<dyn Error>> {
    values[parameter]["quantity"]["value"]
        .as_f64()
        .ok_or_else(|| format!("{parameter} is unavailable").into())
}

#[test]
fn shaft_power_is_the_rated_power_of_every_engine() -> Result<(), Box<dyn Error>> {
    let vehicles = propeller_vehicles()?;
    assert!(
        !vehicles.is_empty(),
        "no example declares engine.shaft_power"
    );
    for path in vehicles {
        for engine_count in [1_u32, 2, 4] {
            let values = synthesized_values(&path, engine_count)?;
            let installed = value(&values, "engine.rated_power")?
                * f64::from(engine_count)
                * value(&values, "engine.sizing_factor")?;
            let shaft = value(&values, "engine.shaft_power")?;
            assert!(
                (shaft - installed).abs() <= 1e-9 * installed,
                "{}: {engine_count} engines give shaft power {shaft} W, installed {installed} W",
                path.display()
            );
        }
    }
    Ok(())
}

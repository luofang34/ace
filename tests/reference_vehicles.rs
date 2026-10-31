//! Real-aircraft reference vehicles keep plausible, legacy-consistent results.
#![cfg(test)]

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;

fn synthesize(name: &str) -> Result<Value, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("vehicle.json");
    let output = cargo_bin_cmd!("aex")
        .arg("synthesize")
        .arg(path)
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn value(result: &Value, parameter: &str) -> Result<f64, Box<dyn Error>> {
    result["snapshot"]["values"][parameter]["quantity"]["value"]
        .as_f64()
        .ok_or_else(|| format!("{parameter} is unavailable").into())
}

#[test]
fn c172_vehicle_flies_its_mission_with_reference_performance() -> Result<(), Box<dyn Error>> {
    let result = synthesize("c172")?;
    assert_eq!(result["completed"], true);
    assert_eq!(result["assessment"]["feasibility"], "provisional");
    assert!((value(&result, "mission.flown_distance")? - 350.0 * 1852.0).abs() < 1.0);
    // The scenario mission burns 86 kg including reserve with power-fraction schedules;
    // constant-efficiency vehicle phases must stay within that class of result.
    let fuel = value(&result, "mission.block_fuel")? + value(&result, "mission.reserve_fuel")?;
    assert!((60.0..=100.0).contains(&fuel), "{fuel}");
    assert!(value(&result, "mission.remaining_fuel")? > 0.0);
    let stall_kt = value(&result, "performance.stall_speed_landing")? / (1852.0 / 3600.0);
    assert!((40.0..=50.0).contains(&stall_kt), "{stall_kt}");
    let ceiling_ft = value(&result, "performance.service_ceiling")? / 0.3048;
    assert!((11_000.0..=14_000.0).contains(&ceiling_ft), "{ceiling_ft}");
    assert_eq!(result["margins"]["stall_speed_landing"]["status"], "PASS");
    assert_eq!(result["margins"]["service_ceiling"]["status"], "FAIL");
    Ok(())
}

#[test]
fn c172_vehicle_study_explores_the_reference_trade_grid() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172");
    let output = cargo_bin_cmd!("aex")
        .arg("study")
        .arg(root.join("vehicle.json"))
        .arg("--plan")
        .arg(root.join("vehicle-study.json"))
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let archive: ace_study::StudyArchive = serde_json::from_slice(&output.stdout)?;
    assert_eq!(archive.candidates.len(), 81);
    assert_eq!(archive.stop_reason, ace_study::StopReason::GridExhausted);
    assert!(!archive.provisional_pareto.is_empty());
    assert!(
        archive
            .candidates
            .iter()
            .all(|candidate| candidate.feasibility != ace_study::Feasibility::Unknown)
    );
    Ok(())
}

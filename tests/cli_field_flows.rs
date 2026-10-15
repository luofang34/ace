//! Public CLI flows for field conditions and performance mass inputs.

#![cfg(test)]

use std::error::Error;
use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;

fn scenario(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("scenario.yaml")
}

fn command(directory: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aex");
    command.current_dir(directory);
    command
}

#[test]
fn performance_mass_flags_reach_field_lengths() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let path = scenario("c172");
    let field = |extra: &[&str]| -> Result<(f64, f64), Box<dyn Error>> {
        let mut arguments = vec![
            "analyze".to_owned(),
            "performance".to_owned(),
            path.to_string_lossy().into_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ];
        arguments.extend(extra.iter().map(|item| (*item).to_owned()));
        let output = command(temporary.path())
            .args(&arguments)
            .assert()
            .success();
        let result: Value = serde_json::from_slice(&output.get_output().stdout)?;
        let value = |key: &str| {
            result["result"][key]["value"]
                .as_f64()
                .ok_or_else(|| format!("missing {key}"))
        };
        Ok((
            value("takeoff_field_length_m")?,
            value("landing_field_length_m")?,
        ))
    };
    let (takeoff, landing) = field(&[])?;
    let (light_takeoff, light_landing) = field(&["--mass", "900 kg", "--landing-mass", "800 kg"])?;
    assert!(light_takeoff < takeoff);
    assert!(light_landing < landing);
    let (hot_takeoff, _) = field(&[
        "--set",
        "mission.field.elevation=3000 m",
        "--set",
        "mission.atmosphere.isa_offset=20 degC",
    ])?;
    assert!(hot_takeoff > takeoff);
    Ok(())
}

#[test]
fn point_below_stall_reports_minimum_flyable_speed() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let path = scenario("c172");
    let output = command(temporary.path())
        .args([
            "analyze",
            "point",
            &path.to_string_lossy(),
            "--altitude",
            "3000 m",
            "--speed",
            "10 m/s",
            "--format",
            "json",
        ])
        .assert()
        .failure();
    let error: Value = serde_json::from_slice(&output.get_output().stdout)?;
    assert_eq!(error["error"]["code"], "SPEED_BELOW_STALL");
    let minimum = error["error"]["context"]["minimum_flyable_speed_m_s"]
        .as_f64()
        .ok_or("missing minimum flyable speed")?;
    assert!(minimum > 10.0);
    assert_eq!(error["error"]["context"]["altitude_m"], 3000.0);
    Ok(())
}

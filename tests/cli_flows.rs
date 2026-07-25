//! Public CLI flow and calibration acceptance tests.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
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

fn study(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("study.yaml")
}

fn model501_scenario(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("designs")
        .join("model501")
        .join("candidates")
        .join(name)
        .join("scenario.yaml")
}

fn command(directory: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aex");
    command.current_dir(directory);
    command
}

fn json_output(arguments: &[&str]) -> Result<Value, Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let output = command(temporary.path()).args(arguments).assert().success();
    Ok(serde_json::from_slice(&output.get_output().stdout)?)
}

#[test]
fn validates_both_reference_projects() -> Result<(), Box<dyn Error>> {
    for name in ["c172", "b777"] {
        for document_path in [scenario(name), study(name)] {
            let path = document_path.to_string_lossy();
            let result = json_output(&["validate", &path, "--format", "json"])?;
            assert_eq!(result["valid"], true);
        }
    }
    Ok(())
}

#[test]
fn model501_engine_trade_examples_resolve() {
    let temporary = TempDir::new();
    assert!(temporary.is_ok());
    if let Ok(directory) = temporary {
        for name in [
            "model501_pw812d",
            "model501_twin_pw306d1",
            "model501_passport20",
        ] {
            command(directory.path())
                .args([
                    "resolve",
                    &model501_scenario(name).to_string_lossy(),
                    "--format",
                    "json",
                ])
                .assert()
                .success();
        }
    }
}

#[test]
fn mission_outputs_use_nautical_mile_display() -> Result<(), Box<dyn Error>> {
    for name in ["c172", "b777"] {
        let scenario_path = scenario(name);
        let path = scenario_path.to_string_lossy();
        let result = json_output(&["analyze", "mission", &path, "--format", "json"])?;
        assert_eq!(result["mission"]["completed"], true);
        assert_eq!(result["hard_requirements_passed"], true);
        assert_eq!(result["mission"]["total_distance"]["display_unit"], "nmi");
        assert!(
            result["mission"]["segments"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
    }
    Ok(())
}

#[test]
fn calibration_bands_cover_both_aircraft_classes() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let c172_path = scenario("c172");
    let c172 = command(temporary.path())
        .args([
            "analyze",
            "performance",
            &c172_path.to_string_lossy(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let c172_result: Value = serde_json::from_slice(&c172.get_output().stdout)?;
    let stall_m_s = c172_result["result"]["stall_speed_clean_m_s"]["value"]
        .as_f64()
        .ok_or("missing C172 stall speed")?;
    let ceiling_m = c172_result["result"]["service_ceiling_m"]["value"]
        .as_f64()
        .ok_or("missing C172 ceiling")?;
    assert!((45.0..=60.0).contains(&(stall_m_s / 0.514_444)));
    assert!((11_000.0..=16_000.0).contains(&(ceiling_m / 0.3048)));

    let b777_path = scenario("b777");
    let b777 = command(temporary.path())
        .args([
            "analyze",
            "performance",
            &b777_path.to_string_lossy(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let b777_result: Value = serde_json::from_slice(&b777.get_output().stdout)?;
    let ceiling_m = b777_result["result"]["service_ceiling_m"]["value"]
        .as_f64()
        .ok_or("missing B777 ceiling")?;
    let lift_drag = b777_result["result"]["maximum_lift_to_drag_ratio"]
        .as_f64()
        .ok_or("missing B777 lift-to-drag ratio")?;
    assert!((39_000.0..=45_000.0).contains(&(ceiling_m / 0.3048)));
    assert!((16.0..=22.0).contains(&lift_drag));
    Ok(())
}

#[test]
fn plots_and_two_dimensional_sweep_execute() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let artifact = temporary.path().join("payload-range.svg");
    let b777_path = scenario("b777");
    command(temporary.path())
        .args([
            "plot",
            "payload-range",
            &b777_path.to_string_lossy(),
            "--output",
            &artifact.to_string_lossy(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    assert!(fs::metadata(&artifact)?.len() > 100);

    let c172_path = scenario("c172");
    let sweep = command(temporary.path())
        .args([
            "sweep",
            &c172_path.to_string_lossy(),
            "--var",
            "aircraft.geometry.wing.area=14 m^2:18 m^2:3",
            "--var",
            "aircraft.geometry.wing.aspect_ratio=7:8:3",
            "--metric",
            "performance.stall_speed_landing",
            "--metric",
            "geometry.wing_area",
            "--metric",
            "geometry.wing_span",
            "--metric",
            "geometry.aspect_ratio",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let sweep_result: Value = serde_json::from_slice(&sweep.get_output().stdout)?;
    let rows = sweep_result["result"]["rows"]
        .as_array()
        .ok_or("missing sweep rows")?;
    assert_eq!(rows.len(), 9);
    for row in rows {
        let metrics = &row["metrics"];
        let area = metrics["geometry.wing_area"]
            .as_f64()
            .ok_or("missing sweep wing area")?;
        let span = metrics["geometry.wing_span"]
            .as_f64()
            .ok_or("missing sweep wing span")?;
        let aspect_ratio = metrics["geometry.aspect_ratio"]
            .as_f64()
            .ok_or("missing sweep aspect ratio")?;
        assert!((span.powi(2) / area - aspect_ratio).abs() < 1.0e-9);
    }
    Ok(())
}

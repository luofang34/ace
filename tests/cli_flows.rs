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
        assert!(
            result["requirements"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| {
                    item["metric"]
                        .as_str()
                        .is_some_and(|metric| metric.starts_with("performance.achieved_cruise_"))
                }))
        );
        assert!(
            result["report_markdown"]
                .as_str()
                .is_some_and(|report| report.contains("Achieved cruise"))
        );
    }
    Ok(())
}

#[test]
fn project_display_units_and_si_override_preserve_canonical_values() -> Result<(), Box<dyn Error>> {
    let scenario_path = scenario("c172");
    let path = scenario_path.to_string_lossy();
    let arguments = [
        "analyze",
        "point",
        &path,
        "--altitude",
        "8000 ft",
        "--speed",
        "115 kt",
        "--mass",
        "2400 lb",
        "--format",
        "json",
    ];
    let aviation = json_output(&arguments)?;
    let mut si_arguments = arguments.to_vec();
    si_arguments.extend(["--units", "si"]);
    let si = json_output(&si_arguments)?;

    for (field, aviation_unit, si_unit) in [
        ("altitude_m", "ft", "m"),
        ("true_airspeed_m_s", "kt", "m/s"),
        ("mass_kg", "lb", "kg"),
    ] {
        assert_eq!(aviation["result"][field]["display_unit"], aviation_unit);
        assert_eq!(si["result"][field]["display_unit"], si_unit);
        assert_eq!(
            aviation["result"][field]["value"],
            si["result"][field]["value"]
        );
        assert_eq!(
            aviation["result"][field]["unit"],
            si["result"][field]["unit"]
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
    let achieved_cruise_m_s = c172_result["result"]["achieved_cruise_true_airspeed_m_s"]["value"]
        .as_f64()
        .ok_or("missing C172 achieved cruise speed")?;
    assert!((45.0..=60.0).contains(&(stall_m_s / 0.514_444)));
    assert!((11_000.0..=16_000.0).contains(&(ceiling_m / 0.3048)));
    assert!((110.0..=120.0).contains(&(achieved_cruise_m_s / 0.514_444)));
    assert_eq!(c172_result["result"]["cruise_feasible"], true);

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
    let achieved_cruise_mach = b777_result["result"]["achieved_cruise_mach"]
        .as_f64()
        .ok_or("missing B777 achieved cruise Mach")?;
    assert!((39_000.0..=45_000.0).contains(&(ceiling_m / 0.3048)));
    assert!((16.0..=22.0).contains(&lift_drag));
    assert!((0.82..=0.85).contains(&achieved_cruise_mach));
    assert_eq!(b777_result["result"]["cruise_feasible"], true);
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
            "--metric",
            "performance.achieved_cruise_true_airspeed",
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
        assert!(
            metrics["performance.achieved_cruise_true_airspeed"]["value"]
                .as_f64()
                .is_some_and(|speed| speed > 0.0)
        );
        assert_eq!(
            row["metric_validity"]["performance.achieved_cruise_true_airspeed"]["status"],
            "valid"
        );
        assert!((span.powi(2) / area - aspect_ratio).abs() < 1.0e-9);
    }
    Ok(())
}

#[test]
fn profile_sanity_warnings_are_advisory_unless_strict() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let sr71_path = scenario("sr71");
    let path = sr71_path.to_string_lossy();
    let profile_path = sr71_path
        .parent()
        .ok_or("missing SR-71 example directory")?
        .join("profiles/j58.yaml");
    let validated = command(temporary.path())
        .args([
            "profile",
            "validate",
            &profile_path.to_string_lossy(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let validation: Value = serde_json::from_slice(&validated.get_output().stdout)?;
    assert!(validation["warnings"].as_array().is_some_and(|warnings| {
        warnings
            .iter()
            .any(|warning| warning["code"] == "PARAMETER_OUTSIDE_TYPICAL")
    }));
    let resolved = command(temporary.path())
        .args(["resolve", &path, "--format", "json"])
        .assert()
        .success();
    let document: Value = serde_json::from_slice(&resolved.get_output().stdout)?;
    let warnings = document["warnings"]
        .as_array()
        .ok_or("missing resolved warnings")?;
    assert!(
        warnings
            .iter()
            .any(|warning| warning["code"] == "PARAMETER_OUTSIDE_TYPICAL")
    );
    let analyzed = command(temporary.path())
        .args(["analyze", "performance", &path, "--format", "json"])
        .assert()
        .success();
    let analysis: Value = serde_json::from_slice(&analyzed.get_output().stdout)?;
    assert!(
        analysis["result"]["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings
                .iter()
                .any(|warning| warning["code"] == "PARAMETER_OUTSIDE_TYPICAL"))
    );

    let strict = command(temporary.path())
        .args(["resolve", &path, "--strict", "--format", "json"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&strict.get_output().stderr);
    assert!(stderr.contains("STRICT_WARNING_FAILURE"));
    assert!(stderr.contains("PARAMETER_OUTSIDE_TYPICAL"));
    let strict_analysis = command(temporary.path())
        .args([
            "analyze",
            "performance",
            &path,
            "--strict",
            "--format",
            "json",
        ])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&strict_analysis.get_output().stderr);
    assert!(stderr.contains("PARAMETER_OUTSIDE_TYPICAL"));
    Ok(())
}

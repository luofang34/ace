//! Profile-sanity diagnostics across composed CLI workflows.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;

fn command(directory: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aex");
    command.current_dir(directory);
    command
}

fn create_atypical_c172(directory: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172");
    let target = directory.join("atypical-c172");
    fs::create_dir_all(target.join("profiles"))?;
    for relative in [
        "aircraft.yaml",
        "mission.yaml",
        "requirements.yaml",
        "study.yaml",
        "profiles/propeller.yaml",
    ] {
        fs::copy(source.join(relative), target.join(relative))?;
    }
    let scenario = fs::read_to_string(source.join("scenario.yaml"))?
        .replace("id: c172-normal-mission", "id: c172-atypical-profile");
    fs::write(target.join("scenario.yaml"), scenario)?;
    let engine = fs::read_to_string(source.join("profiles/engine.yaml"))?
        .replace("rated_altitude: 0 ft", "rated_altitude: -1000 m");
    fs::write(target.join("profiles/engine.yaml"), engine)?;
    Ok(target)
}

fn warning_present(value: &Value) -> bool {
    value.as_array().is_some_and(|warnings| {
        warnings
            .iter()
            .any(|warning| warning["code"] == "PARAMETER_OUTSIDE_TYPICAL")
    })
}

#[test]
fn sweep_propagates_profile_warnings_and_promotes_them_in_strict_mode() -> Result<(), Box<dyn Error>>
{
    let temporary = TempDir::new()?;
    let project = create_atypical_c172(temporary.path())?;
    let scenario = project.join("scenario.yaml");
    let scenario_text = scenario.to_string_lossy().into_owned();
    let arguments = [
        "sweep",
        scenario_text.as_str(),
        "--var",
        "aircraft.geometry.wing.area=15 m^2:16 m^2:2",
        "--metric",
        "geometry.wing_area",
        "--format",
        "json",
    ];
    let output = command(temporary.path()).args(arguments).assert().success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout)?;
    assert!(warning_present(&result["result"]["warnings"]));
    assert!(
        result["result"]["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| warning_present(&row["warnings"])))
    );

    let strict = command(temporary.path())
        .args(arguments)
        .arg("--strict")
        .assert()
        .failure();
    assert!(
        String::from_utf8_lossy(&strict.get_output().stderr).contains("PARAMETER_OUTSIDE_TYPICAL")
    );
    Ok(())
}

#[test]
fn plots_share_strict_policy_and_retain_requirement_warnings() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let project = create_atypical_c172(temporary.path())?;
    let scenario = project.join("scenario.yaml");
    let artifact = temporary.path().join("requirements.svg");
    let output = command(temporary.path())
        .args([
            "plot",
            "requirement-margins",
            &scenario.to_string_lossy(),
            "--output",
            &artifact.to_string_lossy(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&output.get_output().stdout)?;
    assert!(warning_present(&result["chart"]["warnings"]));

    let strict_artifact = temporary.path().join("strict.svg");
    let strict = command(temporary.path())
        .args([
            "plot",
            "drag-polar",
            &scenario.to_string_lossy(),
            "--output",
            &strict_artifact.to_string_lossy(),
            "--strict",
        ])
        .assert()
        .failure();
    assert!(
        String::from_utf8_lossy(&strict.get_output().stderr).contains("PARAMETER_OUTSIDE_TYPICAL")
    );
    assert!(!strict_artifact.exists());
    Ok(())
}

#[test]
fn study_validation_and_comparison_retain_profile_warnings() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let project = create_atypical_c172(temporary.path())?;
    let validation = command(temporary.path())
        .args([
            "validate",
            &project.join("study.yaml").to_string_lossy(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&validation.get_output().stdout)?;
    assert!(warning_present(&result["warnings"]));

    let stock = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let comparison = command(temporary.path())
        .args([
            "compare",
            &project.join("scenario.yaml").to_string_lossy(),
            &stock.to_string_lossy(),
            "--metric",
            "performance.wing_loading",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&comparison.get_output().stdout)?;
    let warning = result["warnings"]
        .as_array()
        .and_then(|warnings| {
            warnings
                .iter()
                .find(|warning| warning["code"] == "PARAMETER_OUTSIDE_TYPICAL")
        })
        .ok_or("comparison dropped profile warning")?;
    assert_eq!(warning["context"]["scenario_id"], "c172-atypical-profile");
    Ok(())
}

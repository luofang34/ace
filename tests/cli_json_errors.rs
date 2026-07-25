//! Structured CLI failure contract tests.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;

fn repository_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn failed_json(arguments: &[&str]) -> Result<Value, Box<dyn Error>> {
    let output = cargo_bin_cmd!("aex")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(arguments)
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "JSON failures must not write stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let error = value["error"].as_object().ok_or("missing error envelope")?;
    assert_eq!(error.len(), 4);
    for field in ["code", "message", "path", "context"] {
        assert!(error.contains_key(field), "missing error field {field}");
    }
    assert!(value["error"]["context"].is_object());
    Ok(value)
}

#[test]
fn analysis_failure_is_one_structured_json_object() -> Result<(), Box<dyn Error>> {
    let scenario = repository_path("examples/sr71/scenario.yaml");
    let result = failed_json(&[
        "analyze",
        "mission",
        &scenario.to_string_lossy(),
        "--format",
        "json",
    ])?;

    assert_eq!(result["error"]["code"], "MODEL_DOMAIN_UNSUPPORTED");
    assert!(
        result["error"]["context"]["violations"]
            .as_array()
            .is_some_and(|violations| violations.len() > 1)
    );
    Ok(())
}

#[test]
fn speed_based_point_preflights_its_effective_mach() -> Result<(), Box<dyn Error>> {
    let scenario = repository_path("examples/c172/scenario.yaml");
    let result = failed_json(&[
        "analyze",
        "point",
        &scenario.to_string_lossy(),
        "--altitude",
        "0 m",
        "--speed",
        "400 m/s",
        "--format",
        "json",
    ])?;

    assert_eq!(result["error"]["code"], "MODEL_DOMAIN_UNSUPPORTED");
    assert!(
        result["error"]["context"]["violations"]
            .as_array()
            .is_some_and(|violations| violations.iter().any(|violation| {
                violation["path"] == "condition.true_airspeed"
                    && violation["variable"] == "mach"
                    && violation["declared_value"]
                        .as_f64()
                        .is_some_and(|mach| mach > 0.9)
            }))
    );
    Ok(())
}

#[test]
fn mission_speed_representations_preflight_effective_mach() -> Result<(), Box<dyn Error>> {
    let scenario = repository_path("examples/c172/scenario.yaml");
    for (path, field) in [
        (
            "mission.segments.cruise.true_airspeed=400 m/s",
            "mission.segments.cruise.true_airspeed",
        ),
        (
            "mission.segments.climb.indicated_airspeed=400 m/s",
            "mission.segments.climb.indicated_airspeed",
        ),
    ] {
        let result = failed_json(&[
            "resolve",
            &scenario.to_string_lossy(),
            "--set",
            path,
            "--format",
            "json",
        ])?;
        assert_eq!(result["error"]["code"], "MODEL_DOMAIN_UNSUPPORTED");
        assert!(
            result["error"]["context"]["violations"]
                .as_array()
                .is_some_and(|violations| violations.iter().any(|violation| {
                    violation["path"] == field
                        && violation["variable"] == "mach"
                        && violation["declared_value"]
                            .as_f64()
                            .is_some_and(|mach| mach > 0.9)
                }))
        );
    }
    Ok(())
}

#[test]
fn non_finite_conditions_fail_before_analysis_and_serialize_cleanly() -> Result<(), Box<dyn Error>>
{
    let scenario = repository_path("examples/b777/scenario.yaml");
    for value in [".nan", ".inf", "-.inf"] {
        let result = failed_json(&[
            "analyze",
            "mission",
            &scenario.to_string_lossy(),
            "--set",
            &format!("mission.segments.cruise_1.mach={value}"),
            "--format",
            "json",
        ])?;
        assert_eq!(result["error"]["code"], "NON_FINITE_VALUE");
        assert_eq!(result["error"]["path"], "mission.segments.cruise_1.mach");
        assert!(
            !String::from_utf8(serde_json::to_vec(&result)?)?.contains("\"declared_value\":null")
        );
    }
    Ok(())
}

#[test]
fn validation_failure_is_one_structured_json_object() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let profile = temporary.path().join("unsupported-profile.yaml");
    fs::write(
        &profile,
        "schema_version: 1\nprofile:\n  id: engine.unsupported\n  version: 1\n  type: warp_drive\n  model: none\n  parameters: {}\n",
    )?;
    let requested_output = temporary.path().join("result.json");
    let result = failed_json(&[
        "validate",
        &profile.to_string_lossy(),
        "--format",
        "json",
        "--output",
        &requested_output.to_string_lossy(),
    ])?;

    assert_eq!(result["error"]["code"], "UNSUPPORTED_PROPULSION_PROFILE");
    assert_eq!(result["error"]["path"], "profile.type");
    assert!(!requested_output.exists());
    Ok(())
}

#[test]
fn parse_quantity_profile_and_filesystem_failures_share_the_contract() -> Result<(), Box<dyn Error>>
{
    let c172 = repository_path("examples/c172/scenario.yaml");
    let quantity = failed_json(&[
        "analyze",
        "point",
        &c172.to_string_lossy(),
        "--altitude",
        "banana",
        "--format",
        "json",
    ])?;
    assert_eq!(quantity["error"]["code"], "INVALID_QUANTITY");
    assert!(quantity["error"]["path"].is_null());
    assert_eq!(quantity["error"]["context"]["target_unit"], "m");

    let parse = failed_json(&["analyze", "point", "--format=json"])?;
    assert_eq!(parse["error"]["code"], "CLI_MISSING_ARGUMENT");

    let profiles = repository_path("profiles");
    let profile = failed_json(&[
        "profile",
        "show",
        "does.not.exist",
        "--directory",
        &profiles.to_string_lossy(),
        "--format",
        "json",
    ])?;
    assert_eq!(profile["error"]["code"], "PROFILE_NOT_FOUND");

    let temporary = TempDir::new()?;
    let missing = temporary.path().join("missing.yaml");
    let filesystem = failed_json(&["validate", &missing.to_string_lossy(), "--format", "json"])?;
    assert_eq!(filesystem["error"]["code"], "FILE_READ_FAILED");
    assert_eq!(filesystem["error"]["context"]["io_kind"], "NotFound");
    Ok(())
}

#[test]
fn human_errors_and_help_preserve_stream_and_exit_behavior() {
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("missing-human.yaml");
    let failure = cargo_bin_cmd!("aex")
        .args(["validate", &missing.to_string_lossy()])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(failure.stdout.is_empty());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("failed to read"));

    cargo_bin_cmd!("aex")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("Usage:"));
}

#[test]
fn output_failure_does_not_leak_an_earlier_success_log() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let scenario = repository_path("examples/c172/scenario.yaml");
    let artifact = temporary.path().join("drag-polar.svg");
    let output = cargo_bin_cmd!("aex")
        .env("RUST_LOG", "info")
        .args([
            "plot",
            "drag-polar",
            &scenario.to_string_lossy(),
            "--output",
            &artifact.to_string_lossy(),
            "--format",
            "json",
            "--spec-output",
            &temporary.path().to_string_lossy(),
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    assert!(output.stderr.is_empty());
    let error: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(error["error"]["code"], "FILE_WRITE_FAILED");
    assert!(artifact.exists());
    Ok(())
}

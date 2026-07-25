//! Cross-path mission diagnostic propagation tests.

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

fn create_sr71_probe(directory: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/sr71");
    let target = directory.join("sr71-probe");
    fs::create_dir_all(target.join("profiles"))?;
    for relative in ["aircraft.yaml", "requirements.yaml", "scenario.yaml"] {
        fs::copy(source.join(relative), target.join(relative))?;
    }
    let profile = fs::read_to_string(source.join("profiles/j58.yaml"))?
        .replace("bypass_ratio: 0.0", "bypass_ratio: 0.2")
        .replace(
            "mach_linear_coefficient: -0.35",
            "mach_linear_coefficient: 0.05",
        )
        .replace("cruise_reference_mach: 3.2", "cruise_reference_mach: 1.0")
        .replace("nacelle_drag_area: 0.0 m^2", "nacelle_drag_area: 0.05 m^2")
        .replace("maximum_mach: 3.3", "maximum_mach: 1.2")
        .replace("maximum_altitude: 85000 ft", "maximum_altitude: 20000 m");
    fs::write(target.join("profiles/j58.yaml"), profile)?;
    fs::write(
        target.join("mission.yaml"),
        "schema_version: 1\nmission:\n  id: sr71_strict_probe\n  name: SR-71 strict diagnostic probe\n  payload:\n    mass: 1200 kg\n  segments:\n    - id: strict_probe\n      type: cruise\n      distance: 1 nmi\n      altitude: 64000 ft\n      mach: 3.2\n",
    )?;
    Ok(target.join("scenario.yaml"))
}

fn strict_stderr(directory: &Path, arguments: &[&str]) -> String {
    let output = command(directory).args(arguments).assert().failure();
    String::from_utf8_lossy(&output.get_output().stderr).into_owned()
}

#[test]
fn strict_point_and_mission_fail_on_the_same_sr71_model_warning() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let scenario = create_sr71_probe(temporary.path())?;
    let path = scenario.to_string_lossy().into_owned();
    let mission = command(temporary.path())
        .args(["analyze", "mission", &path, "--format", "json"])
        .assert()
        .success();
    let result: Value = serde_json::from_slice(&mission.get_output().stdout)?;
    assert_eq!(result["mission"]["completed"], true);
    assert!(
        result["mission"]["segments"][0]["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings.iter().any(|warning| {
                warning["code"] == "MODEL_EXTRAPOLATION"
                    && warning["path"] == "mission.segments.strict_probe.condition.mach"
            }))
    );

    let point_error = strict_stderr(
        temporary.path(),
        &[
            "analyze",
            "point",
            &path,
            "--altitude",
            "64000 ft",
            "--mach",
            "3.2",
            "--strict",
        ],
    );
    let mission_error = strict_stderr(temporary.path(), &["analyze", "mission", &path, "--strict"]);
    for error in [point_error, mission_error] {
        assert!(error.contains("STRICT_WARNING_FAILURE"));
        assert!(error.contains("MODEL_EXTRAPOLATION"));
        assert!(error.contains("Parabolic polar evaluated at Mach 3.200"));
    }
    Ok(())
}

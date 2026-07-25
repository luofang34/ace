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
    let aircraft = fs::read_to_string(source.join("aircraft.yaml"))?
        .replace("span: 16.94 m", "span: 29.16 m")
        .replace("aspect_ratio: 1.69", "aspect_ratio: 5.0")
        .replace(
            "mach: [0.0, 0.85, 1.15, 2.0, 3.2, 3.3]",
            "mach: [0.0, 0.4, 0.6, 0.8, 1.0, 1.2]",
        )
        .replace("maximum_operating_mach: 3.3", "maximum_operating_mach: 0.9")
        .replace(
            "maximum_operating_altitude: 85000 ft",
            "maximum_operating_altitude: 20000 m",
        );
    fs::write(target.join("aircraft.yaml"), aircraft)?;
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

fn json_error(directory: &Path, arguments: &[&str]) -> Result<Value, Box<dyn Error>> {
    let output = command(directory).args(arguments).assert().failure();
    assert!(output.get_output().stderr.is_empty());
    Ok(serde_json::from_slice(&output.get_output().stdout)?)
}

#[test]
fn point_and_mission_share_resolve_time_domain_rejection() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let scenario = create_sr71_probe(temporary.path())?;
    let path = scenario.to_string_lossy().into_owned();
    let point_error = json_error(
        temporary.path(),
        &[
            "analyze",
            "point",
            &path,
            "--altitude",
            "64000 ft",
            "--mach",
            "3.2",
            "--format",
            "json",
        ],
    )?;
    let mission_error = json_error(
        temporary.path(),
        &["analyze", "mission", &path, "--format", "json"],
    )?;

    assert_eq!(point_error["error"], mission_error["error"]);
    assert_eq!(mission_error["error"]["code"], "MODEL_DOMAIN_UNSUPPORTED");
    let violations = mission_error["error"]["context"]["violations"]
        .as_array()
        .ok_or("missing model-domain violations")?;
    assert_eq!(violations.len(), 2);
    assert!(violations.iter().all(|violation| {
        violation["path"] == "mission.segments.strict_probe.mach"
            && violation["declared_value"] == 3.2
    }));
    Ok(())
}

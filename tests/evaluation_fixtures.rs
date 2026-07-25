//! Structural guardrails for model-envelope evaluation fixtures.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value as JsonValue;
use serde_yaml::Value;
use tempfile::TempDir;

const YAML_FIXTURES: [(&str, &str); 12] = [
    ("sr71/aircraft-explorer.yaml", "project"),
    ("sr71/aircraft.yaml", "aircraft"),
    ("sr71/mission.yaml", "mission"),
    ("sr71/profiles/j58.yaml", "profile"),
    ("sr71/requirements.yaml", "requirements"),
    ("sr71/scenario.yaml", "scenario"),
    ("x15/aircraft-explorer.yaml", "project"),
    ("x15/aircraft.yaml", "aircraft"),
    ("x15/mission.yaml", "mission"),
    ("x15/profiles/xlr99.yaml", "profile"),
    ("x15/requirements.yaml", "requirements"),
    ("x15/scenario.yaml", "scenario"),
];

fn examples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn command(directory: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aex");
    command.current_dir(directory);
    command
}

fn successful_json(directory: &Path, arguments: &[&str]) -> Result<JsonValue, Box<dyn Error>> {
    let output = command(directory).args(arguments).assert().success();
    Ok(serde_json::from_slice(&output.get_output().stdout)?)
}

fn fixture_path(name: &str, document: &str) -> PathBuf {
    examples_root().join(name).join(document)
}

#[test]
fn envelope_fixture_documents_remain_schema_v1_yaml() -> Result<(), Box<dyn Error>> {
    for (relative_path, envelope) in YAML_FIXTURES {
        let path = examples_root().join(relative_path);
        let document: Value = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
        let version = document
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or("missing numeric schema_version")?;

        assert_eq!(
            version,
            1,
            "unexpected schema version in {}",
            path.display()
        );
        assert!(
            document.get(envelope).is_some(),
            "missing {envelope} envelope in {}",
            path.display()
        );
    }
    Ok(())
}

#[test]
fn typed_fixture_documents_validate_and_scenarios_resolve() -> Result<(), Box<dyn Error>> {
    const DOCUMENTS: [(&str, &str, &str); 8] = [
        ("sr71", "aircraft.yaml", "aircraft"),
        ("sr71", "mission.yaml", "mission"),
        ("sr71", "profiles/j58.yaml", "profile"),
        ("sr71", "requirements.yaml", "requirements"),
        ("x15", "aircraft.yaml", "aircraft"),
        ("x15", "mission.yaml", "mission"),
        ("x15", "profiles/xlr99.yaml", "profile"),
        ("x15", "requirements.yaml", "requirements"),
    ];
    const SCENARIOS: [(&str, &str, &str); 2] = [
        ("sr71", "sr71-operational-sortie", "engine.pw_j58_class"),
        ("x15", "x15-speed-mission", "engine.xlr99_class"),
    ];

    let temporary = TempDir::new()?;
    for (name, document, expected_type) in DOCUMENTS {
        let path = fixture_path(name, document);
        let result = successful_json(
            temporary.path(),
            &["validate", &path.to_string_lossy(), "--format", "json"],
        )?;
        assert_eq!(
            result["valid"],
            true,
            "failed to validate {}",
            path.display()
        );
        assert_eq!(result["document_type"], expected_type);
    }
    for (name, expected_id, expected_engine) in SCENARIOS {
        let scenario = fixture_path(name, "scenario.yaml");
        let resolved = successful_json(
            temporary.path(),
            &["resolve", &scenario.to_string_lossy(), "--format", "json"],
        )?;
        assert_eq!(resolved["engine"]["id"], expected_engine);
        assert_eq!(resolved["id"], expected_id);
    }
    Ok(())
}

#[test]
fn workflows_reproduce_the_documented_envelope_failures() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    for name in ["sr71", "x15"] {
        let scenario = fixture_path(name, "scenario.yaml");
        let result = successful_json(
            temporary.path(),
            &["validate", &scenario.to_string_lossy(), "--format", "json"],
        )?;
        assert_eq!(result["valid"], true);
    }

    let sr71 = fixture_path("sr71", "scenario.yaml");
    let performance = successful_json(
        temporary.path(),
        &[
            "analyze",
            "performance",
            &sr71.to_string_lossy(),
            "--format",
            "json",
        ],
    )?;
    assert_eq!(
        performance["result"]["metric_validity"]["performance.service_ceiling"]["status"],
        "boundary_limited"
    );
    assert_eq!(
        performance["result"]["metric_validity"]["performance.maximum_level_speed"]["status"],
        "extrapolated"
    );
    assert_eq!(
        performance["result"]["model"]["validity_status"],
        "boundary_limited"
    );
    assert_eq!(performance["result"]["cruise_feasible"], false);
    assert!(
        performance["result"]["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings.iter().any(|warning| {
                warning["code"] == "CRUISE_CONDITION_UNSUPPORTED"
                    && warning["path"] == "mission.segments.supersonic_cruise"
            }))
    );
    let output = command(temporary.path())
        .args([
            "analyze",
            "mission",
            &sr71.to_string_lossy(),
            "--format",
            "json",
        ])
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("ATMOSPHERE_OUTSIDE_VALIDITY"));

    let x15 = fixture_path("x15", "scenario.yaml");
    let result = successful_json(
        temporary.path(),
        &[
            "analyze",
            "mission",
            &x15.to_string_lossy(),
            "--format",
            "json",
        ],
    )?;
    assert_eq!(result["completion_status"], "incomplete");
    assert_eq!(result["mission"]["completed"], false);
    assert_eq!(result["mission"]["fuel_exhausted"], true);
    assert_eq!(result["mission"]["fuel_capacity_violation"], false);
    assert_eq!(result["hard_requirements_passed"], false);
    assert_eq!(
        result["requirements"][0]["metric"],
        "performance.achieved_cruise_mach"
    );
    assert_eq!(result["performance"]["cruise_feasible"], true);
    assert!(
        result["report_markdown"]
            .as_str()
            .is_some_and(|report| report.contains("Hard requirements passed: false"))
    );
    let warnings = result["mission"]["warnings"]
        .as_array()
        .ok_or("missing mission warnings")?;
    assert!(warnings.iter().any(|warning| {
        warning["code"] == "FUEL_EXHAUSTED" && warning["path"] == "mission.segments.glide_descent"
    }));
    Ok(())
}

#[test]
fn fixture_readmes_disclose_their_evaluation_surrogates() -> Result<(), Box<dyn Error>> {
    for name in ["sr71", "x15"] {
        let path = examples_root().join(name).join("README.md");
        let readme = fs::read_to_string(&path)?;
        assert!(
            readme.contains("evaluation fixture"),
            "{} must identify itself as an evaluation fixture",
            path.display()
        );
        assert!(
            readme.contains("unsupported"),
            "{} must require explicit unsupported outcomes",
            path.display()
        );
    }
    let sr71 = fs::read_to_string(fixture_path("sr71", "README.md"))?;
    assert!(sr71.contains("80,280 lb"));
    assert!(sr71.contains("post-takeoff refueling"));
    Ok(())
}

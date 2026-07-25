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

fn failed_json(directory: &Path, arguments: &[&str]) -> Result<JsonValue, Box<dyn Error>> {
    let output = command(directory).args(arguments).assert().failure();
    assert!(output.get_output().stderr.is_empty());
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
fn typed_fixture_documents_validate_independently() -> Result<(), Box<dyn Error>> {
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
    Ok(())
}

#[test]
fn workflows_reproduce_the_documented_envelope_failures() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    for (name, expected_count, required_path) in [
        ("sr71", 4, "mission.segments.supersonic_cruise.altitude"),
        ("x15", 6, "mission.segments.boost_climb.schedule.2.mach"),
    ] {
        let scenario = fixture_path(name, "scenario.yaml");
        let validation = failed_json(
            temporary.path(),
            &["validate", &scenario.to_string_lossy(), "--format", "json"],
        )?;
        let analysis = failed_json(
            temporary.path(),
            &[
                "analyze",
                "mission",
                &scenario.to_string_lossy(),
                "--format",
                "json",
            ],
        )?;
        assert_eq!(validation, analysis);
        assert_eq!(validation["error"]["code"], "MODEL_DOMAIN_UNSUPPORTED");
        let violations = validation["error"]["context"]["violations"]
            .as_array()
            .ok_or("missing model-domain violations")?;
        assert_eq!(violations.len(), expected_count);
        assert!(
            violations
                .iter()
                .any(|violation| violation["path"] == required_path)
        );
        if name == "sr71" {
            assert!(
                violations
                    .iter()
                    .all(|violation| violation["model_id"] != "aero.polar_table")
            );
        }
        assert!(violations.iter().all(|violation| {
            violation["declared_value"].is_number()
                && violation["declared_unit"].is_string()
                && violation["bound_unit"].is_string()
                && violation["basis"].is_string()
        }));
    }
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

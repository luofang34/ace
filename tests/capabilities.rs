//! Public capability-manifest discovery and vocabulary round-trip tests.

#![allow(clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};

#[path = "capabilities/initial_state.rs"]
mod initial_state;

const MISSING_OPENVSP: &str = "/path/that/does/not/contain/vspscript";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn arguments(value: Value) -> Result<serde_json::Map<String, Value>, io::Error> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| io::Error::other("tool arguments must be an object"))
}

fn yaml_document(path: &Path) -> Result<Value, Box<dyn Error>> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    Ok(serde_json::to_value(yaml)?)
}

fn segment_field_value(name: &str) -> Option<Value> {
    match name {
        "duration" => Some(json!("1 min")),
        "distance" => Some(json!("1 nmi")),
        "target_altitude" | "altitude" => Some(json!("1000 ft")),
        "indicated_airspeed" | "true_airspeed" => Some(json!("100 kt")),
        "mach" | "power_fraction" | "thrust_fraction" | "fuel_fraction" => Some(json!(0.5)),
        "fuel_mass" | "payload_mass" => Some(json!("1 kg")),
        _ => None,
    }
}

#[tokio::test]
async fn cli_and_mcp_manifest_match_and_advertised_vocabulary_validates()
-> Result<(), Box<dyn Error>> {
    let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
    command.args(["mcp", "serve"]);
    command.env("ACE_OPENVSP_EXECUTABLE", MISSING_OPENVSP);
    let client = ().serve(TokioChildProcess::new(command)?).await?;

    let mcp = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "get_capabilities".into(),
            arguments: Some(arguments(json!({}))?),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing capability manifest"))?;
    let cli = std::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"))
        .env("ACE_OPENVSP_EXECUTABLE", MISSING_OPENVSP)
        .args(["capabilities", "--format", "json"])
        .output()?;
    assert!(cli.status.success());
    let cli: Value = serde_json::from_slice(&cli.stdout)?;
    assert_eq!(mcp, cli);
    assert_eq!(mcp["schema_version"], 1);

    validate_document_types(&client, &mcp).await?;
    validate_profile_types(&client, &mcp).await?;
    validate_segment_types(&client, &mcp).await?;
    initial_state::validate(&client, &mcp).await?;
    validate_exclusive_segment_fields(&client).await?;
    validate_engine_off_fractions(&client).await?;
    validate_unadvertised_segment_fields(&client).await?;
    validate_requirement_metrics(&client, &mcp).await?;
    validate_requirement_qualifiers(&client, &mcp).await?;
    validate_configurations(&client, &mcp).await?;
    assert_manifest_sections(&mcp)?;

    client.cancel().await?;
    Ok(())
}

async fn validate_document_types(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    let c172 = root().join("examples/c172");
    for item in manifest["document_types"]
        .as_array()
        .ok_or_else(|| io::Error::other("document types must be an array"))?
    {
        let kind = item["id"]
            .as_str()
            .ok_or_else(|| io::Error::other("document type must be a string"))?;
        let path = match kind {
            "aircraft" => c172.join("aircraft.yaml"),
            "mission" => c172.join("mission.yaml"),
            "requirements" => c172.join("requirements.yaml"),
            "profile" => c172.join("profiles/engine.yaml"),
            "scenario" => c172.join("scenario.yaml"),
            "study" => c172.join("study.yaml"),
            other => return Err(format!("unmapped document capability {other}").into()),
        };
        let result = validate_document(client, kind, yaml_document(&path)?).await?;
        assert_eq!(result["valid"], true, "{kind}: {result}");
    }
    Ok(())
}

async fn validate_engine_off_fractions(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
) -> Result<(), Box<dyn Error>> {
    for field in ["power_fraction", "thrust_fraction"] {
        let document = json!({
            "schema_version": 1,
            "mission": {
                "id": "engine_off_test",
                "name": "Engine-off test",
                "payload": { "mass": "10 kg" },
                "segments": [{
                    "id": "segment",
                    "type": "fixed_time",
                    "duration": "1 min",
                    (field): 0
                }]
            }
        });
        let result = validate_document(client, "mission", document).await?;
        assert_eq!(result["valid"], true, "{field}: {result}");
    }
    Ok(())
}

async fn validate_exclusive_segment_fields(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
) -> Result<(), Box<dyn Error>> {
    for (fields, group) in [
        (
            json!({"indicated_airspeed": "100 kt", "mach": 0.5}),
            "speed",
        ),
        (
            json!({"power_fraction": 0.5, "thrust_fraction": 0.5}),
            "throttle",
        ),
    ] {
        let mut segment = serde_json::Map::from_iter([
            ("id".to_owned(), json!("segment")),
            ("type".to_owned(), json!("fixed_time")),
            ("duration".to_owned(), json!("1 min")),
        ]);
        segment.extend(
            fields
                .as_object()
                .ok_or_else(|| io::Error::other("exclusive fields must be an object"))?
                .clone(),
        );
        let document = json!({
            "schema_version": 1,
            "mission": {
                "id": "exclusive_field_test",
                "name": "Exclusive field test",
                "payload": { "mass": "10 kg" },
                "segments": [Value::Object(segment)]
            }
        });
        let result = validate_document(client, "mission", document).await?;
        assert_eq!(result["valid"], false, "{result}");
        let serialized = result.to_string();
        assert!(serialized.contains("SEGMENT_FIELD_EXCLUSIVITY"));
        assert!(serialized.contains("mission.segments.0"));
        assert!(serialized.contains(group));
    }
    Ok(())
}

async fn validate_unadvertised_segment_fields(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
) -> Result<(), Box<dyn Error>> {
    let document = json!({
        "schema_version": 1,
        "mission": {
            "id": "unknown_field_test",
            "name": "Unknown field test",
            "payload": { "mass": "10 kg" },
            "segments": [{
                "id": "segment",
                "type": "cruise",
                "distance": "1 nmi",
                "typo_field": 123
            }]
        }
    });
    let result = validate_document(client, "mission", document).await?;
    assert_eq!(result["valid"], false, "{result}");
    let serialized = result.to_string();
    assert!(serialized.contains("UNSUPPORTED_SEGMENT_FIELD"));
    assert!(serialized.contains("mission.segments.0.typo_field"));
    assert!(serialized.contains("cruise"));
    Ok(())
}

async fn validate_profile_types(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    let profiles = [
        (
            "piston_engine",
            root().join("examples/c172/profiles/engine.yaml"),
        ),
        (
            "turbofan_engine",
            root().join("examples/b777/profiles/engine.yaml"),
        ),
        (
            "propeller",
            root().join("examples/c172/profiles/propeller.yaml"),
        ),
    ];
    let advertised = manifest["profile_types"]
        .as_array()
        .ok_or_else(|| io::Error::other("profile types must be an array"))?;
    for (kind, path) in profiles {
        assert!(advertised.iter().any(|item| item["id"] == kind));
        let result = validate_document(client, "profile", yaml_document(&path)?).await?;
        assert_eq!(result["valid"], true, "{kind}: {result}");
    }
    Ok(())
}

async fn validate_segment_types(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    for capability in manifest["mission_segments"]
        .as_array()
        .ok_or_else(|| io::Error::other("mission segments must be an array"))?
    {
        let segment_type = capability["segment_type"]
            .as_str()
            .ok_or_else(|| io::Error::other("segment type must be a string"))?;
        let fields = capability["fields"]
            .as_array()
            .ok_or_else(|| io::Error::other("segment fields must be an array"))?;
        for field in fields {
            let document = segment_document(segment_type, fields, field)?;
            let result = validate_document(client, "mission", document).await?;
            assert_eq!(result["valid"], true, "{segment_type}/{field}: {result}");
        }
    }
    Ok(())
}

fn segment_document(
    segment_type: &str,
    fields: &[Value],
    exercised: &Value,
) -> Result<Value, Box<dyn Error>> {
    let mut segment = serde_json::Map::from_iter([
        ("id".to_owned(), json!("segment")),
        ("type".to_owned(), json!(segment_type)),
    ]);
    for field in fields {
        if field["requirement"] == "required"
            || field["requirement"] == "exactly_one"
                && !segment.values().any(|value| value == &json!("1 kg"))
        {
            insert_segment_field(&mut segment, field)?;
        }
    }
    if exercised["requirement"] == "exactly_one" {
        for field in fields
            .iter()
            .filter(|field| field["alternative_group"] == exercised["alternative_group"])
        {
            if let Some(name) = field["name"].as_str() {
                segment.remove(name);
            }
        }
    }
    insert_segment_field(&mut segment, exercised)?;
    Ok(json!({
        "schema_version": 1,
        "mission": {
            "id": "capability_test",
            "name": "Capability test",
            "payload": { "mass": "10 kg" },
            "segments": [Value::Object(segment)]
        }
    }))
}

fn insert_segment_field(
    segment: &mut serde_json::Map<String, Value>,
    field: &Value,
) -> Result<(), Box<dyn Error>> {
    let name = field["name"]
        .as_str()
        .ok_or_else(|| io::Error::other("field name must be a string"))?;
    let value = segment_field_value(name)
        .ok_or_else(|| io::Error::other(format!("unmapped segment field {name}")))?;
    segment.insert(name.to_owned(), value);
    Ok(())
}

async fn validate_requirement_metrics(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    for capability in manifest["requirement_metrics"]
        .as_array()
        .ok_or_else(|| io::Error::other("requirement metrics must be an array"))?
    {
        let metric = capability["id"]
            .as_str()
            .ok_or_else(|| io::Error::other("metric id must be a string"))?;
        let unit = capability["canonical_unit"]
            .as_str()
            .ok_or_else(|| io::Error::other("metric unit must be a string"))?;
        let value = if unit == "1" {
            json!(1.0)
        } else {
            json!(format!("1 {unit}"))
        };
        let document = json!({
            "schema_version": 1,
            "requirements": {
                "id": "capability_test",
                "items": [{
                    "id": "metric",
                    "metric": metric,
                    "operator": "ge",
                    "value": value,
                    "severity": "hard"
                }]
            }
        });
        let result = validate_document(client, "requirements", document).await?;
        if capability["bindable"] == true {
            assert_eq!(result["valid"], true, "{metric}: {result}");
        } else {
            assert_eq!(result["valid"], false, "{metric}: {result}");
            let replacement = capability["replacement"]
                .as_str()
                .ok_or_else(|| io::Error::other("declared metric needs a replacement"))?;
            assert!(result.to_string().contains(replacement));
        }
    }
    Ok(())
}

async fn validate_requirement_qualifiers(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    let operators = manifest["requirement_operators"]
        .as_array()
        .ok_or_else(|| io::Error::other("requirement operators must be an array"))?;
    let severities = manifest["requirement_severities"]
        .as_array()
        .ok_or_else(|| io::Error::other("requirement severities must be an array"))?;
    for operator in operators {
        for severity in severities {
            let document = json!({
                "schema_version": 1,
                "requirements": {
                    "id": "capability_test",
                    "items": [{
                        "id": "qualifier",
                        "metric": "mission.payload_mass",
                        "operator": operator,
                        "value": "1 kg",
                        "severity": severity
                    }]
                }
            });
            let result = validate_document(client, "requirements", document).await?;
            assert_eq!(result["valid"], true, "{operator}/{severity}: {result}");
        }
    }
    Ok(())
}

async fn validate_configurations(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    let scenario = root().join("examples/c172/scenario.yaml");
    for capability in manifest["configurations"]
        .as_array()
        .ok_or_else(|| io::Error::other("configurations must be an array"))?
    {
        let configuration = capability["id"]
            .as_str()
            .ok_or_else(|| io::Error::other("configuration id must be a string"))?;
        let result = client
            .call_tool(CallToolRequestParams {
                meta: None,
                name: "calculate_point_performance".into(),
                arguments: Some(arguments(json!({
                    "scenario_path": scenario,
                    "condition": {
                        "altitude": "3000 ft",
                        "true_airspeed": "115 kt",
                        "mass": "1000 kg",
                        "configuration": configuration
                    },
                    "overrides": {}
                }))?),
                task: None,
            })
            .await?;
        assert_eq!(result.is_error, Some(false), "{configuration}");
    }
    Ok(())
}

fn assert_manifest_sections(manifest: &Value) -> Result<(), Box<dyn Error>> {
    assert_eq!(manifest["backends"][0]["id"], "native");
    assert_eq!(manifest["backends"][1]["id"], "openvsp");
    assert!(
        manifest["model_domains"]
            .as_array()
            .is_some_and(|domains| domains.iter().all(|domain| {
                domain["model_id"].is_string()
                    && domain["bounds"]
                        .as_array()
                        .is_some_and(|bounds| !bounds.is_empty())
            }))
    );
    assert!(
        manifest["strict_warning_policy"]
            .as_array()
            .is_some_and(|warnings| warnings.iter().all(|warning| {
                warning["code"]
                    .as_str()
                    .is_some_and(|code| code == code.to_ascii_uppercase())
            }))
    );
    Ok(())
}

async fn validate_document(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    document_type: &str,
    document: Value,
) -> Result<Value, Box<dyn Error>> {
    client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "validate_document".into(),
            arguments: Some(arguments(json!({
                "document_type": document_type,
                "document": document
            }))?),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing validation result").into())
}

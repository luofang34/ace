use std::error::Error;
use std::io;

use serde_json::{Value, json};

use super::{segment_field_value, validate_document};

pub(super) async fn validate(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    let fields = manifest["mission_initial_state_fields"]
        .as_array()
        .ok_or_else(|| io::Error::other("mission initial-state fields must be an array"))?;
    for field in fields {
        let name = field["name"]
            .as_str()
            .ok_or_else(|| io::Error::other("initial-state field name must be a string"))?;
        let value = segment_field_value(name)
            .ok_or_else(|| io::Error::other(format!("unmapped initial-state field {name}")))?;
        let document =
            mission_with_initial_state(serde_json::Map::from_iter([(name.to_owned(), value)]));
        let result = validate_document(client, "mission", document).await?;
        assert_eq!(result["valid"], true, "{name}: {result}");
    }
    validate_exclusive_groups(client).await
}

async fn validate_exclusive_groups(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
) -> Result<(), Box<dyn Error>> {
    for (state, group) in [
        (
            serde_json::Map::from_iter([
                ("true_airspeed".to_owned(), json!("100 kt")),
                ("mach".to_owned(), json!(0.5)),
            ]),
            "speed",
        ),
        (
            serde_json::Map::from_iter([
                ("fuel_mass".to_owned(), json!("10 kg")),
                ("fuel_fraction".to_owned(), json!(0.5)),
            ]),
            "fuel",
        ),
    ] {
        let result =
            validate_document(client, "mission", mission_with_initial_state(state)).await?;
        assert_eq!(result["valid"], false, "{result}");
        let serialized = result.to_string();
        assert!(serialized.contains("INITIAL_STATE_FIELD_EXCLUSIVITY"));
        assert!(serialized.contains(group));
    }
    Ok(())
}

fn mission_with_initial_state(initial_state: serde_json::Map<String, Value>) -> Value {
    json!({
        "schema_version": 1,
        "mission": {
            "id": "initial_state_capability",
            "name": "Initial-state capability",
            "payload": { "mass": "10 kg" },
            "initial_state": Value::Object(initial_state),
            "segments": [{
                "id": "segment",
                "type": "fixed_time",
                "duration": "1 min",
                "power_fraction": 0
            }]
        }
    })
}

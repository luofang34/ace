use std::error::Error;
use std::io;

use serde_json::{Value, json};

pub(super) async fn validate(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    let fields = manifest["energy_schedule_fields"]
        .as_array()
        .ok_or_else(|| io::Error::other("energy schedule fields must be an array"))?;
    assert_eq!(
        fields
            .iter()
            .filter_map(|field| field["name"].as_str())
            .collect::<Vec<_>>(),
        ["altitude", "indicated_airspeed", "true_airspeed", "mach"]
    );
    for (schedule, expected_code) in [
        (
            json!([
                {"altitude": "0 ft", "true_airspeed": "100 kt"},
                {"altitude": "1000 ft"}
            ]),
            "ENERGY_SCHEDULE_FIELD_EXCLUSIVITY",
        ),
        (
            json!([
                {"altitude": "0 ft", "true_airspeed": "100 kt"},
                {"altitude": "1000 ft", "true_airspeed": "90 kt"}
            ]),
            "NONMONOTONIC_ENERGY_SCHEDULE",
        ),
    ] {
        let result = super::validate_document(client, "mission", document(schedule)).await?;
        assert_eq!(result["valid"], false, "{result}");
        assert!(result.to_string().contains(expected_code), "{result}");
    }
    Ok(())
}

fn document(schedule: Value) -> Value {
    json!({
        "schema_version": 1,
        "mission": {
            "id": "energy_climb_test",
            "name": "Energy climb test",
            "payload": {"mass": "10 kg"},
            "segments": [{
                "id": "climb",
                "type": "energy_climb",
                "thrust_fraction": 0.8,
                "schedule": schedule
            }]
        }
    })
}

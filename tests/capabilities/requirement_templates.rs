use std::error::Error;
use std::io;

use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};

use super::{arguments, validate_document};

pub(super) async fn validate(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    let templates = manifest["requirement_templates"]
        .as_array()
        .ok_or_else(|| io::Error::other("requirement templates must be an array"))?;
    assert_eq!(templates.len(), 2);
    for template in templates {
        let id = template["id"]
            .as_str()
            .ok_or_else(|| io::Error::other("template id must be a string"))?;
        let version = template["version"]
            .as_u64()
            .ok_or_else(|| io::Error::other("template version must be an integer"))?;
        let mut reference = serde_json::Map::from_iter([
            ("id".to_owned(), json!(id)),
            ("version".to_owned(), json!(version)),
        ]);
        if template["parameter"]["name"] == "engine_count" {
            reference.insert("engine_count".to_owned(), json!(2));
        }
        let document = json!({
            "schema_version": 1,
            "requirements": {
                "id": "template_round_trip",
                "template": reference,
                "items": []
            }
        });
        let result = validate_document(client, "requirements", document).await?;
        assert_eq!(result["valid"], true, "{id}: {result}");
        assert!(
            template["items"]
                .as_array()
                .is_some_and(|items| items.iter().all(|item| {
                    item["metric"].is_string()
                        && item["operator"].is_string()
                        && item["provenance"]["kind"].is_string()
                }))
        );
    }
    assert_transport_oei_values(templates)?;
    reject_template_metric_override(client).await
}

fn assert_transport_oei_values(templates: &[Value]) -> Result<(), Box<dyn Error>> {
    let transport = templates
        .iter()
        .find(|template| template["id"] == "transport_conceptual")
        .ok_or_else(|| io::Error::other("transport template was not advertised"))?;
    let oei = transport["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["id"] == "oei_second_segment_climb_gradient")
        })
        .ok_or_else(|| io::Error::other("transport OEI item was not advertised"))?;
    let values = oei["values_by_engine_count"]
        .as_array()
        .ok_or_else(|| io::Error::other("OEI values must be an array"))?;
    for (engine_count, value) in [(2, 0.024), (3, 0.027), (4, 0.030)] {
        assert!(
            values
                .iter()
                .any(|entry| { entry["engine_count"] == engine_count && entry["value"] == value })
        );
    }
    assert!(
        oei["provenance"]["citation"]
            .as_str()
            .is_some_and(|citation| citation.contains("25.121"))
    );
    Ok(())
}

async fn reject_template_metric_override(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
) -> Result<(), Box<dyn Error>> {
    let result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "validate_document".into(),
            arguments: Some(arguments(json!({
                "document_type": "requirements",
                "document": {
                    "schema_version": 1,
                    "requirements": {
                        "id": "immutable_template",
                        "template": {
                            "id": "light_aircraft_conceptual",
                            "version": 1
                        },
                        "items": [{
                            "id": "stall_speed_landing",
                            "metric": "mission.payload_mass"
                        }]
                    }
                }
            }))?),
            task: None,
        })
        .await?
        .structured_content
        .ok_or_else(|| io::Error::other("missing template validation result"))?;
    assert_eq!(result["valid"], false);
    assert!(
        result
            .to_string()
            .contains("TEMPLATE_REQUIREMENT_IMMUTABLE_FIELD")
    );
    Ok(())
}

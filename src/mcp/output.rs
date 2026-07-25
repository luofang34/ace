use std::collections::BTreeMap;
use std::path::Path;

use rmcp::ErrorData;
use rmcp::handler::server::wrapper::Json;
use rmcp::schemars;
use serde::Serialize;
use serde_json::Value;

use crate::domain::presentation::DisplayUnitSystem;
use crate::storage::project_store::display_unit_system_blocking;

use super::{mcp_error, mcp_serialization_error, serialization};

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(super) struct ObjectOutput {
    #[serde(flatten)]
    fields: BTreeMap<String, Value>,
}

pub(super) fn json_output<T: Serialize>(value: T) -> Result<Json<ObjectOutput>, ErrorData> {
    json_output_with_system(value, DisplayUnitSystem::Si)
}

pub(super) fn json_output_for_scenario<T: Serialize>(
    value: T,
    scenario_path: &Path,
    explicit_units: Option<&str>,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let system = display_unit_system_blocking(scenario_path, explicit_units).map_err(mcp_error)?;
    json_output_with_system(value, system)
}

pub(super) fn json_output_for_units<T: Serialize>(
    value: T,
    explicit_units: Option<&str>,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let system = explicit_units
        .map(|units| DisplayUnitSystem::parse(units, "units"))
        .transpose()
        .map_err(mcp_error)?
        .unwrap_or_default();
    json_output_with_system(value, system)
}

pub(super) fn json_output_with_system<T: Serialize>(
    value: T,
    system: DisplayUnitSystem,
) -> Result<Json<ObjectOutput>, ErrorData> {
    let serialized = serde_json::to_value(value).map_err(mcp_serialization_error)?;
    let interface_value = serialization::attach_units_for(serialized, system);
    let fields = match interface_value {
        Value::Object(mapping) => mapping.into_iter().collect(),
        scalar => BTreeMap::from([("result".to_owned(), scalar)]),
    };
    Ok(Json(ObjectOutput { fields }))
}

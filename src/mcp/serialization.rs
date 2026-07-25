use serde_json::{Map, Value, json};

const SUFFIX_UNITS: [(&str, &str); 12] = [
    ("_kg_m3", "kg/m^3"),
    ("_m2_s", "m^2/s"),
    ("_pa_s", "Pa*s"),
    ("_n_m2", "N/m^2"),
    ("_w_kg", "W/kg"),
    ("_kg_s", "kg/s"),
    ("_m_s", "m/s"),
    ("_kg", "kg"),
    ("_pa", "Pa"),
    ("_n", "N"),
    ("_w", "W"),
    ("_m", "m"),
];

pub(crate) fn attach_units(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(attach_units).collect()),
        Value::Object(mapping) => attach_mapping(mapping),
        scalar => scalar,
    }
}

fn attach_mapping(mapping: Map<String, Value>) -> Value {
    let converted = mapping
        .into_iter()
        .map(|(key, value)| {
            let result = match unit_for_key(&key).filter(|_| numeric_or_numeric_array(&value)) {
                Some(unit) => quantity(&key, value, unit),
                None => attach_units(value),
            };
            (key, result)
        })
        .collect();
    Value::Object(converted)
}

fn unit_for_key(key: &str) -> Option<&'static str> {
    match key {
        "performance.stall_speed"
        | "performance.stall_speed_landing"
        | "performance.maximum_level_speed" => Some("m/s"),
        "performance.service_ceiling" | "performance.absolute_ceiling" => Some("m"),
        "performance.wing_loading" => Some("N/m^2"),
        "performance.thrust_or_power_loading" => Some("W/kg or N/N"),
        "mission.total_fuel" => Some("kg"),
        "mission.completed_distance"
        | "mission.range"
        | "performance.full_payload_range"
        | "performance.zero_payload_ferry_range" => Some("m"),
        _ => SUFFIX_UNITS
            .iter()
            .find_map(|(suffix, unit)| key.ends_with(suffix).then_some(*unit)),
    }
}

fn numeric_or_numeric_array(value: &Value) -> bool {
    value.is_number()
        || value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_number))
}

fn quantity(key: &str, value: Value, unit: &str) -> Value {
    let range = matches!(
        key,
        "distance_m"
            | "range_m"
            | "total_distance_m"
            | "mission.completed_distance"
            | "mission.range"
            | "performance.full_payload_range"
            | "performance.zero_payload_ferry_range"
    );
    let display_value = if range {
        convert_value(&value, 1852.0)
    } else {
        value.clone()
    };
    json!({
        "value": value,
        "unit": unit,
        "display_value": display_value,
        "display_unit": if range { "nmi" } else { unit },
    })
}

fn convert_value(value: &Value, divisor: f64) -> Value {
    match value {
        Value::Number(number) => number
            .as_f64()
            .and_then(|item| serde_json::Number::from_f64(item / divisor))
            .map_or(Value::Null, Value::Number),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| convert_value(item, divisor))
                .collect(),
        ),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests;

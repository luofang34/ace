use serde_json::{Map, Value, json};

use crate::domain::presentation::DisplayUnitSystem;
use crate::domain::quantity::{FOOT_M, KNOT_M_S, NAUTICAL_MILE_M};

const POUND_KG: f64 = 0.453_592_37;

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

pub(crate) fn attach_units_for(value: Value, system: DisplayUnitSystem) -> Value {
    attach_value(value, None, system)
}

fn attach_value(value: Value, key: Option<&str>, system: DisplayUnitSystem) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| attach_value(item, key, system))
                .collect(),
        ),
        Value::Object(mapping) => attach_mapping(mapping, key, system),
        scalar => scalar,
    }
}

fn attach_mapping(
    mut mapping: Map<String, Value>,
    key: Option<&str>,
    system: DisplayUnitSystem,
) -> Value {
    if is_quantity(&mapping) {
        apply_display(&mut mapping, key.unwrap_or_default(), system);
        return Value::Object(mapping);
    }
    let metric = mapping
        .get("metric")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let converted = mapping
        .into_iter()
        .map(|(key, value)| {
            let semantic_key = semantic_key(&key, metric.as_deref());
            let result =
                match unit_for_key(semantic_key).filter(|_| numeric_or_numeric_array(&value)) {
                    Some(unit) => quantity(semantic_key, value, unit, system),
                    None => attach_value(value, Some(semantic_key), system),
                };
            (key, result)
        })
        .collect();
    Value::Object(converted)
}

fn semantic_key<'a>(field: &'a str, metric: Option<&'a str>) -> &'a str {
    if matches!(field, "actual" | "required") {
        metric.unwrap_or(field)
    } else {
        field
    }
}

fn is_quantity(mapping: &Map<String, Value>) -> bool {
    mapping.get("value").is_some_and(Value::is_number)
        && mapping.get("unit").is_some_and(Value::is_string)
}

fn apply_display(mapping: &mut Map<String, Value>, key: &str, system: DisplayUnitSystem) {
    let Some(value) = mapping.get("value").cloned() else {
        return;
    };
    let Some(unit) = mapping
        .get("unit")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return;
    };
    let display = display_spec(key, &unit, system);
    let divisor = display.divisor;
    let display_unit = display.unit.to_owned();
    mapping.insert("display_value".to_owned(), convert_value(&value, divisor));
    mapping.insert("display_unit".to_owned(), Value::String(display_unit));
}

fn unit_for_key(key: &str) -> Option<&'static str> {
    match key {
        "performance.stall_speed"
        | "performance.stall_speed_landing"
        | "performance.maximum_level_speed"
        | "performance.declared_cruise_true_airspeed"
        | "performance.achieved_cruise_true_airspeed" => Some("m/s"),
        "performance.service_ceiling" | "performance.absolute_ceiling" => Some("m"),
        "performance.minimum_cruise_excess_power" => Some("W"),
        "performance.wing_loading" => Some("N/m^2"),
        "performance.thrust_or_power_loading" => Some("W/kg or N/N"),
        "mission.total_fuel" | "mission.landing_fuel" => Some("kg"),
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

fn quantity(key: &str, value: Value, unit: &str, system: DisplayUnitSystem) -> Value {
    let display = display_spec(key, unit, system);
    let display_value = convert_value(&value, display.divisor);
    json!({
        "value": value,
        "unit": unit,
        "display_value": display_value,
        "display_unit": display.unit,
    })
}

struct DisplaySpec<'a> {
    divisor: f64,
    unit: &'a str,
}

fn display_spec<'a>(
    key: &str,
    canonical_unit: &'a str,
    system: DisplayUnitSystem,
) -> DisplaySpec<'a> {
    if system == DisplayUnitSystem::Si {
        return DisplaySpec {
            divisor: 1.0,
            unit: canonical_unit,
        };
    }
    match canonical_unit {
        "m/s" => DisplaySpec {
            divisor: KNOT_M_S,
            unit: "kt",
        },
        "kg" => DisplaySpec {
            divisor: POUND_KG,
            unit: "lb",
        },
        "m" if is_distance(key) => DisplaySpec {
            divisor: NAUTICAL_MILE_M,
            unit: "nmi",
        },
        "m" => DisplaySpec {
            divisor: FOOT_M,
            unit: "ft",
        },
        _ => DisplaySpec {
            divisor: 1.0,
            unit: canonical_unit,
        },
    }
}

fn is_distance(key: &str) -> bool {
    key.contains("distance") || key.contains("range")
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

use std::collections::BTreeMap;

use serde_yaml::{Mapping, Value};

use crate::domain::diagnostic::{AexError, AexResult};

pub(crate) fn apply_overrides(
    aircraft: &mut Value,
    mission: &mut Value,
    requirements: &mut Value,
    overrides: &BTreeMap<String, String>,
) -> AexResult<()> {
    for (path, raw_value) in overrides {
        let parts: Vec<&str> = path.split('.').collect();
        let root = parts.first().copied().ok_or_else(|| {
            AexError::validation("INVALID_OVERRIDE", path, "override path is empty")
        })?;
        let document = match root {
            "aircraft" => &mut *aircraft,
            "mission" => &mut *mission,
            "requirements" => &mut *requirements,
            _ => {
                return Err(AexError::validation(
                    "INVALID_OVERRIDE",
                    path,
                    "path must begin with aircraft, mission, or requirements",
                ));
            }
        };
        set_path(document, &parts, raw_value, path)?;
    }
    Ok(())
}

fn set_path(target: &mut Value, parts: &[&str], raw_value: &str, full_path: &str) -> AexResult<()> {
    if parts.is_empty() {
        return Err(AexError::validation(
            "INVALID_OVERRIDE",
            full_path,
            "override must address a field",
        ));
    }
    if target.is_sequence() {
        return set_sequence_path(target, parts, raw_value, full_path);
    }
    let mapping = target.as_mapping_mut().ok_or_else(|| {
        AexError::validation(
            "INVALID_OVERRIDE",
            full_path,
            "path is neither a mapping nor an ID-addressable sequence",
        )
    })?;
    let key = Value::String(parts[0].to_owned());
    if parts.len() == 1 {
        let value = mapping.get(&key).map_or_else(
            || optional_override_value(raw_value, full_path),
            |current| override_value(current, raw_value, full_path).map(Some),
        )?;
        let value = value.ok_or_else(|| {
            AexError::validation("INVALID_OVERRIDE", full_path, "field does not exist")
        })?;
        mapping.insert(key, value);
        return Ok(());
    }
    if is_optional_geometry_mapping(full_path, parts) {
        let nested = mapping
            .entry(key)
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        if nested.is_null() {
            *nested = Value::Mapping(Mapping::new());
        }
        return set_path(nested, &parts[1..], raw_value, full_path);
    }
    let nested = mapping.get_mut(&key).ok_or_else(|| {
        AexError::validation("INVALID_OVERRIDE", full_path, "field does not exist")
    })?;
    set_path(nested, &parts[1..], raw_value, full_path)
}

fn is_optional_geometry_mapping(full_path: &str, remaining: &[&str]) -> bool {
    remaining.len() == 2
        && matches!(
            remaining[0],
            "fuselage" | "horizontal_tail" | "vertical_tail"
        )
        && full_path.starts_with("aircraft.geometry.")
}

fn set_sequence_path(
    target: &mut Value,
    parts: &[&str],
    raw_value: &str,
    full_path: &str,
) -> AexResult<()> {
    let selector = parts[0];
    let sequence = target.as_sequence_mut().ok_or_else(|| {
        AexError::validation("INVALID_OVERRIDE", full_path, "path is not a sequence")
    })?;
    let item = sequence
        .iter_mut()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(selector))
        .ok_or_else(|| {
            AexError::validation(
                "INVALID_OVERRIDE",
                full_path,
                format!("sequence has no item with id {selector}"),
            )
        })?;
    if parts.len() == 1 {
        return Err(AexError::validation(
            "INVALID_OVERRIDE",
            full_path,
            "override must address a field within the selected item",
        ));
    }
    set_path(item, &parts[1..], raw_value, full_path)
}

fn override_value(current: &Value, raw: &str, path: &str) -> AexResult<Value> {
    match current {
        Value::String(_) => Ok(Value::String(raw.to_owned())),
        Value::Bool(_) => raw
            .parse::<bool>()
            .map(Value::Bool)
            .map_err(|source| AexError::validation("INVALID_OVERRIDE", path, source.to_string())),
        Value::Number(_) => numeric_override(raw, path),
        Value::Null => serde_yaml::from_str::<Value>(raw).map_err(|source| AexError::Yaml {
            path: path.into(),
            source,
        }),
        _ => Err(AexError::validation(
            "INVALID_OVERRIDE",
            path,
            "override must address a scalar field",
        )),
    }
}

fn numeric_override(raw: &str, path: &str) -> AexResult<Value> {
    let value = serde_yaml::from_str::<Value>(raw).map_err(|source| AexError::Yaml {
        path: path.into(),
        source,
    })?;
    if value.is_number() {
        Ok(value)
    } else {
        Err(AexError::validation(
            "INVALID_OVERRIDE",
            path,
            "numeric field requires a numeric value",
        ))
    }
}

fn optional_override_value(raw: &str, path: &str) -> AexResult<Option<Value>> {
    match path {
        "aircraft.geometry.wing.area"
        | "aircraft.geometry.wing.span"
        | "aircraft.geometry.fuselage.length"
        | "aircraft.geometry.fuselage.diameter"
        | "aircraft.geometry.horizontal_tail.area"
        | "aircraft.geometry.horizontal_tail.arm"
        | "aircraft.geometry.vertical_tail.area"
        | "aircraft.geometry.vertical_tail.arm" => Ok(Some(Value::String(raw.to_owned()))),
        "aircraft.geometry.wing.aspect_ratio" => numeric_override(raw, path).map(Some),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests;

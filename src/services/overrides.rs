use std::collections::BTreeMap;

use serde_yaml::Value;

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
    let mapping = target.as_mapping_mut().ok_or_else(|| {
        AexError::validation("INVALID_OVERRIDE", full_path, "path is not a mapping")
    })?;
    let key = Value::String(parts[0].to_owned());
    if parts.len() == 1 {
        let current = mapping.get(&key).ok_or_else(|| {
            AexError::validation("INVALID_OVERRIDE", full_path, "field does not exist")
        })?;
        mapping.insert(key, override_value(current, raw_value, full_path)?);
        return Ok(());
    }
    let nested = mapping.get_mut(&key).ok_or_else(|| {
        AexError::validation("INVALID_OVERRIDE", full_path, "field does not exist")
    })?;
    set_path(nested, &parts[1..], raw_value, full_path)
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

#[cfg(test)]
mod tests;

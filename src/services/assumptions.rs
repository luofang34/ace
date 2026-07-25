use serde_yaml::{Mapping, Value};

use crate::domain::schema::{AssumptionEntry, EngineProfile, PropellerProfile};

pub(crate) fn collect_all_assumptions(
    aircraft: &Value,
    mission: &Value,
    requirements: &Value,
    engine: &EngineProfile,
    propeller: Option<&PropellerProfile>,
) -> Vec<AssumptionEntry> {
    let mut entries = Vec::new();
    collect_assumptions("aircraft", aircraft.get("aircraft"), false, &mut entries);
    collect_assumptions("mission", mission.get("mission"), false, &mut entries);
    collect_assumptions(
        "requirements",
        requirements.get("requirements"),
        false,
        &mut entries,
    );
    entries.push(profile_assumption(
        "aircraft.propulsion.profile",
        engine.profile_id(),
    ));
    if let Some(profile) = propeller {
        entries.push(profile_assumption(
            "aircraft.propulsion.propeller_profile",
            &profile.id,
        ));
    }
    entries
}

fn collect_assumptions(
    path: &str,
    value: Option<&Value>,
    inherited: bool,
    entries: &mut Vec<AssumptionEntry>,
) {
    let Some(current) = value else {
        return;
    };
    match current {
        Value::Mapping(mapping) => collect_mapping(path, mapping, inherited, entries),
        Value::Sequence(sequence) => {
            for (index, item) in sequence.iter().enumerate() {
                collect_assumptions(&format!("{path}.{index}"), Some(item), inherited, entries);
            }
        }
        Value::Null => {}
        scalar => entries.push(AssumptionEntry {
            parameter_path: path.to_owned(),
            resolved_value: yaml_to_json(scalar),
            unit: scalar
                .as_str()
                .and_then(|value| declared_quantity_unit(path, value)),
            provenance_kind: if inherited { "reference" } else { "user" }.to_owned(),
            source: if inherited {
                "selected profile"
            } else {
                "scenario document"
            }
            .to_owned(),
            confidence: if inherited { "medium" } else { "high" }.to_owned(),
            explicitly_provided: !inherited,
            inherited_from_profile: inherited,
            supplied_by_default: false,
        }),
    }
}

fn collect_mapping(
    path: &str,
    mapping: &Mapping,
    inherited: bool,
    entries: &mut Vec<AssumptionEntry>,
) {
    for (key, value) in mapping {
        if let Some(name) = key.as_str() {
            collect_assumptions(&format!("{path}.{name}"), Some(value), inherited, entries);
        }
    }
}

fn profile_assumption(path: &str, profile_id: &str) -> AssumptionEntry {
    AssumptionEntry {
        parameter_path: path.to_owned(),
        resolved_value: serde_json::Value::String(profile_id.to_owned()),
        unit: None,
        provenance_kind: "reference".to_owned(),
        source: "selected profile".to_owned(),
        confidence: "medium".to_owned(),
        explicitly_provided: true,
        inherited_from_profile: true,
        supplied_by_default: false,
    }
}

fn yaml_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Bool(item) => serde_json::Value::Bool(*item),
        Value::Number(item) => match serde_json::to_value(item) {
            Ok(number) => number,
            Err(_) => serde_json::Value::String(item.to_string()),
        },
        Value::String(item) => serde_json::Value::String(item.clone()),
        _ => serde_json::Value::Null,
    }
}

fn declared_quantity_unit(path: &str, value: &str) -> Option<String> {
    if !is_quantity_path(path) {
        return None;
    }
    let mut parts = value.trim().splitn(2, char::is_whitespace);
    parts.next()?.parse::<f64>().ok()?;
    parts
        .next()
        .map(str::trim)
        .filter(|unit| !unit.is_empty())
        .map(str::to_owned)
}

fn is_quantity_path(path: &str) -> bool {
    matches!(
        path,
        "aircraft.mass.maximum_takeoff_mass"
            | "aircraft.mass.operating_empty_mass"
            | "aircraft.mass.maximum_payload_mass"
            | "aircraft.mass.maximum_fuel_mass"
            | "aircraft.geometry.wing.area"
            | "aircraft.geometry.wing.span"
            | "aircraft.geometry.wing.sweep_quarter_chord"
            | "aircraft.geometry.wing.center_body_edge_sweep"
            | "aircraft.limits.maximum_operating_speed"
            | "aircraft.limits.maximum_operating_altitude"
            | "mission.payload.mass"
            | "mission.initial_state.altitude"
            | "mission.initial_state.indicated_airspeed"
            | "mission.initial_state.true_airspeed"
            | "mission.initial_state.fuel_mass"
    ) || sequence_quantity_path(
        path,
        "mission.segments.",
        &[
            "duration",
            "distance",
            "target_altitude",
            "altitude",
            "indicated_airspeed",
            "true_airspeed",
            "fuel_mass",
            "payload_mass",
        ],
    ) || sequence_quantity_path(path, "requirements.items.", &["value"])
}

fn sequence_quantity_path(path: &str, prefix: &str, fields: &[&str]) -> bool {
    let Some(remainder) = path.strip_prefix(prefix) else {
        return false;
    };
    let Some((index, field)) = remainder.split_once('.') else {
        return false;
    };
    index.bytes().all(|byte| byte.is_ascii_digit())
        && !index.is_empty()
        && !field.contains('.')
        && fields.contains(&field)
}

#[cfg(test)]
mod tests;

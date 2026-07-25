use std::collections::BTreeSet;

use crate::domain::capabilities::{
    MissionSegmentCapability, SegmentFieldRequirement, mission_segment,
};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::Dimension;
use crate::domain::schema::{MissionSegment, RawMissionSegment};

use super::{optional_fraction, optional_positive_quantity, optional_quantity};

pub(super) fn resolve_segment(raw: RawMissionSegment, index: usize) -> AexResult<MissionSegment> {
    let path = format!("mission.segments.{index}");
    let capability = mission_segment(&raw.kind).ok_or_else(|| {
        AexError::validation(
            "UNSUPPORTED_SEGMENT_TYPE",
            &path,
            format!("unsupported segment type {}", raw.kind),
        )
    })?;
    validate_legal_fields(capability, &raw, &path)?;
    validate_required_fields(capability, &raw, &path)?;
    Ok(MissionSegment {
        id: raw.id,
        kind: capability.kind,
        duration_s: optional_quantity(raw.duration.as_deref(), Dimension::Time)?,
        distance_m: optional_quantity(raw.distance.as_deref(), Dimension::Length)?,
        target_altitude_m: optional_quantity(raw.target_altitude.as_deref(), Dimension::Length)?,
        altitude_m: optional_quantity(raw.altitude.as_deref(), Dimension::Length)?,
        indicated_airspeed_m_s: optional_quantity(
            raw.indicated_airspeed.as_deref(),
            Dimension::Speed,
        )?,
        true_airspeed_m_s: optional_quantity(raw.true_airspeed.as_deref(), Dimension::Speed)?,
        mach: raw.mach,
        power_fraction: optional_fraction(raw.power_fraction, &format!("{path}.power_fraction"))?,
        thrust_fraction: optional_fraction(
            raw.thrust_fraction,
            &format!("{path}.thrust_fraction"),
        )?,
        fuel_fraction: optional_fraction(raw.fuel_fraction, &format!("{path}.fuel_fraction"))?,
        fuel_mass_kg: optional_quantity(raw.fuel_mass.as_deref(), Dimension::Mass)?,
        payload_mass_kg: optional_positive_quantity(
            raw.payload_mass.as_deref(),
            Dimension::Mass,
            &format!("{path}.payload_mass"),
        )?,
    })
}

fn validate_required_fields(
    capability: MissionSegmentCapability,
    raw: &RawMissionSegment,
    path: &str,
) -> AexResult<()> {
    for field in capability
        .fields
        .iter()
        .filter(|field| field.requirement == SegmentFieldRequirement::Required)
    {
        if !field_is_present(raw, field.name) {
            return Err(missing_field(capability.segment_type, field.name, path));
        }
    }
    let groups = capability
        .fields
        .iter()
        .filter_map(|field| field.alternative_group)
        .collect::<BTreeSet<_>>();
    for group in groups {
        let requires_one = capability.fields.iter().any(|field| {
            field.alternative_group == Some(group)
                && field.requirement == SegmentFieldRequirement::ExactlyOne
        });
        let present = capability
            .fields
            .iter()
            .filter(|field| {
                field.alternative_group == Some(group) && field_is_present(raw, field.name)
            })
            .count();
        if present == 0 && requires_one {
            return Err(missing_alternative(capability.segment_type, group, path));
        }
        if present > 1 {
            return Err(duplicate_alternative(
                capability.segment_type,
                group,
                requires_one,
                path,
            ));
        }
    }
    Ok(())
}

fn validate_legal_fields(
    capability: MissionSegmentCapability,
    raw: &RawMissionSegment,
    path: &str,
) -> AexResult<()> {
    if let Some(field) = raw.additional_fields.keys().next() {
        return Err(unsupported_field(capability.segment_type, field, path));
    }
    for field in RAW_SEGMENT_FIELDS {
        if field_is_present(raw, field)
            && !capability.fields.iter().any(|legal| legal.name == field)
        {
            return Err(unsupported_field(capability.segment_type, field, path));
        }
    }
    Ok(())
}

fn unsupported_field(segment_type: &str, field: &str, path: &str) -> AexError {
    AexError::validation(
        "UNSUPPORTED_SEGMENT_FIELD",
        format!("{path}.{field}"),
        format!("{field} is not supported for {segment_type}"),
    )
}

const RAW_SEGMENT_FIELDS: [&str; 12] = [
    "duration",
    "distance",
    "target_altitude",
    "altitude",
    "indicated_airspeed",
    "true_airspeed",
    "mach",
    "power_fraction",
    "thrust_fraction",
    "fuel_fraction",
    "fuel_mass",
    "payload_mass",
];

fn field_is_present(raw: &RawMissionSegment, field: &str) -> bool {
    match field {
        "duration" => raw.duration.is_some(),
        "distance" => raw.distance.is_some(),
        "target_altitude" => raw.target_altitude.is_some(),
        "altitude" => raw.altitude.is_some(),
        "indicated_airspeed" => raw.indicated_airspeed.is_some(),
        "true_airspeed" => raw.true_airspeed.is_some(),
        "mach" => raw.mach.is_some(),
        "power_fraction" => raw.power_fraction.is_some(),
        "thrust_fraction" => raw.thrust_fraction.is_some(),
        "fuel_fraction" => raw.fuel_fraction.is_some(),
        "fuel_mass" => raw.fuel_mass.is_some(),
        "payload_mass" => raw.payload_mass.is_some(),
        _ => false,
    }
}

fn missing_field(segment_type: &str, field: &str, path: &str) -> AexError {
    let (code, message) = match (segment_type, field) {
        ("cruise", "distance") => ("MISSING_SEGMENT_DISTANCE", "cruise requires distance"),
        ("payload_drop", "payload_mass") => {
            ("MISSING_PAYLOAD_MASS", "payload-drop requires payload_mass")
        }
        ("climb", "target_altitude") => {
            ("MISSING_CLIMB_ALTITUDE", "climb requires target_altitude")
        }
        (_, "duration") => ("MISSING_SEGMENT_DURATION", "segment requires duration"),
        _ => (
            "MISSING_SEGMENT_FIELD",
            "segment requires the declared field",
        ),
    };
    AexError::validation(code, format!("{path}.{field}"), message)
}

fn missing_alternative(segment_type: &str, group: &str, path: &str) -> AexError {
    if segment_type == "fixed_fuel" && group == "fuel" {
        AexError::validation(
            "MISSING_FIXED_FUEL",
            path,
            "fixed-fuel segment requires fuel_mass or fuel_fraction",
        )
    } else {
        AexError::validation(
            "MISSING_SEGMENT_FIELD",
            path,
            format!("segment requires one field from alternative group {group}"),
        )
    }
}

fn duplicate_alternative(
    segment_type: &str,
    group: &str,
    requires_one: bool,
    path: &str,
) -> AexError {
    let rule = if requires_one {
        "requires exactly one field from"
    } else {
        "accepts at most one field from"
    };
    AexError::validation(
        "SEGMENT_FIELD_EXCLUSIVITY",
        path,
        format!("{segment_type} {rule} alternative group {group}"),
    )
}

#[cfg(test)]
mod tests;

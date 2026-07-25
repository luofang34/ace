//! Mission segment vocabulary shared by validation and discovery.

use serde::Serialize;

use crate::domain::schema::SegmentKind;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SegmentFieldRequirement {
    Required,
    Optional,
    ExactlyOne,
    AtMostOne,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct SegmentFieldCapability {
    pub(crate) name: &'static str,
    pub(crate) requirement: SegmentFieldRequirement,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) alternative_group: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct MissionSegmentCapability {
    pub(crate) segment_type: &'static str,
    pub(crate) fields: &'static [SegmentFieldCapability],
    #[serde(skip)]
    pub(crate) kind: SegmentKind,
}

const TIMED_FIELDS: &[SegmentFieldCapability] = &[
    required("duration"),
    optional("altitude"),
    at_most_one("indicated_airspeed", "speed"),
    at_most_one("true_airspeed", "speed"),
    at_most_one("mach", "speed"),
    at_most_one("power_fraction", "throttle"),
    at_most_one("thrust_fraction", "throttle"),
];

const LANDING_FIELDS: &[SegmentFieldCapability] = &[
    optional("duration"),
    at_most_one("indicated_airspeed", "speed"),
    at_most_one("true_airspeed", "speed"),
    at_most_one("mach", "speed"),
    at_most_one("power_fraction", "throttle"),
    at_most_one("thrust_fraction", "throttle"),
];

const FIXED_FUEL_FIELDS: &[SegmentFieldCapability] = &[
    exactly_one("fuel_fraction", "fuel"),
    exactly_one("fuel_mass", "fuel"),
];

const PAYLOAD_DROP_FIELDS: &[SegmentFieldCapability] = &[required("payload_mass")];

const CLIMB_FIELDS: &[SegmentFieldCapability] = &[
    required("target_altitude"),
    at_most_one("indicated_airspeed", "speed"),
    at_most_one("true_airspeed", "speed"),
    at_most_one("mach", "speed"),
    at_most_one("power_fraction", "throttle"),
    at_most_one("thrust_fraction", "throttle"),
];

const CRUISE_FIELDS: &[SegmentFieldCapability] = &[
    required("distance"),
    optional("altitude"),
    at_most_one("indicated_airspeed", "speed"),
    at_most_one("true_airspeed", "speed"),
    at_most_one("mach", "speed"),
    at_most_one("power_fraction", "throttle"),
    at_most_one("thrust_fraction", "throttle"),
];

const LOITER_FIELDS: &[SegmentFieldCapability] = &[
    required("duration"),
    optional("altitude"),
    at_most_one("indicated_airspeed", "speed"),
    at_most_one("true_airspeed", "speed"),
    at_most_one("mach", "speed"),
    at_most_one("power_fraction", "throttle"),
    at_most_one("thrust_fraction", "throttle"),
];

const DESCENT_FIELDS: &[SegmentFieldCapability] = &[
    optional("target_altitude"),
    at_most_one("indicated_airspeed", "speed"),
    at_most_one("true_airspeed", "speed"),
    at_most_one("mach", "speed"),
    at_most_one("power_fraction", "throttle"),
    at_most_one("thrust_fraction", "throttle"),
];

const MISSION_SEGMENTS: [MissionSegmentCapability; 11] = [
    segment("start_and_taxi", SegmentKind::StartAndTaxi, TIMED_FIELDS),
    segment("fixed_time", SegmentKind::FixedTime, TIMED_FIELDS),
    segment("fixed_fuel", SegmentKind::FixedFuel, FIXED_FUEL_FIELDS),
    segment(
        "payload_drop",
        SegmentKind::PayloadDrop,
        PAYLOAD_DROP_FIELDS,
    ),
    segment("takeoff", SegmentKind::Takeoff, TIMED_FIELDS),
    segment("climb", SegmentKind::Climb, CLIMB_FIELDS),
    segment("cruise", SegmentKind::Cruise, CRUISE_FIELDS),
    segment("loiter", SegmentKind::Loiter, LOITER_FIELDS),
    segment("descent", SegmentKind::Descent, DESCENT_FIELDS),
    segment("landing", SegmentKind::Landing, LANDING_FIELDS),
    segment("reserve", SegmentKind::Reserve, LOITER_FIELDS),
];

pub(crate) const fn mission_segments() -> &'static [MissionSegmentCapability] {
    &MISSION_SEGMENTS
}

pub(crate) fn mission_segment(id: &str) -> Option<MissionSegmentCapability> {
    MISSION_SEGMENTS
        .iter()
        .copied()
        .find(|item| item.segment_type == id)
}

const fn required(name: &'static str) -> SegmentFieldCapability {
    field(name, SegmentFieldRequirement::Required, None)
}

const fn optional(name: &'static str) -> SegmentFieldCapability {
    field(name, SegmentFieldRequirement::Optional, None)
}

const fn exactly_one(name: &'static str, group: &'static str) -> SegmentFieldCapability {
    field(name, SegmentFieldRequirement::ExactlyOne, Some(group))
}

const fn at_most_one(name: &'static str, group: &'static str) -> SegmentFieldCapability {
    field(name, SegmentFieldRequirement::AtMostOne, Some(group))
}

const fn field(
    name: &'static str,
    requirement: SegmentFieldRequirement,
    alternative_group: Option<&'static str>,
) -> SegmentFieldCapability {
    SegmentFieldCapability {
        name,
        requirement,
        alternative_group,
    }
}

const fn segment(
    segment_type: &'static str,
    kind: SegmentKind,
    fields: &'static [SegmentFieldCapability],
) -> MissionSegmentCapability {
    MissionSegmentCapability {
        segment_type,
        fields,
        kind,
    }
}

//! Raw and resolved mission schema types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct MissionDocument {
    pub(crate) schema_version: u32,
    pub(crate) mission: RawMission,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawMission {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) payload: RawPayload,
    pub(crate) segments: Vec<RawMissionSegment>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawPayload {
    pub(crate) mass: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawMissionSegment {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) duration: Option<String>,
    pub(crate) distance: Option<String>,
    pub(crate) target_altitude: Option<String>,
    pub(crate) altitude: Option<String>,
    pub(crate) indicated_airspeed: Option<String>,
    pub(crate) true_airspeed: Option<String>,
    pub(crate) mach: Option<f64>,
    pub(crate) power_fraction: Option<f64>,
    pub(crate) thrust_fraction: Option<f64>,
    pub(crate) fuel_fraction: Option<f64>,
    pub(crate) fuel_mass: Option<String>,
    pub(crate) payload_mass: Option<String>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) additional_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Mission {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) payload_mass_kg: f64,
    pub(crate) segments: Vec<MissionSegment>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MissionSegment {
    pub(crate) id: String,
    pub(crate) kind: SegmentKind,
    pub(crate) duration_s: Option<f64>,
    pub(crate) distance_m: Option<f64>,
    pub(crate) target_altitude_m: Option<f64>,
    pub(crate) altitude_m: Option<f64>,
    pub(crate) indicated_airspeed_m_s: Option<f64>,
    pub(crate) true_airspeed_m_s: Option<f64>,
    pub(crate) mach: Option<f64>,
    pub(crate) power_fraction: Option<f64>,
    pub(crate) thrust_fraction: Option<f64>,
    pub(crate) fuel_fraction: Option<f64>,
    pub(crate) fuel_mass_kg: Option<f64>,
    pub(crate) payload_mass_kg: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SegmentKind {
    StartAndTaxi,
    FixedTime,
    FixedFuel,
    PayloadDrop,
    Takeoff,
    Climb,
    Cruise,
    Loiter,
    Descent,
    Landing,
    Reserve,
}

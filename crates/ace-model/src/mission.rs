use crate::{Binding, Quantity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A point on a trajectory retaining individual tank and payload masses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VehicleState {
    /// Elapsed mission time.
    pub time: Quantity,
    /// Achieved mission distance.
    pub distance: Quantity,
    /// Mass per item, including fuel sequencing and released payload.
    pub masses: BTreeMap<String, Quantity>,
    /// Altitude, speed, thrust, aerodynamic state, and domain-specific values.
    pub conditions: BTreeMap<String, Quantity>,
    /// Accumulated operating cost.
    pub cost: Quantity,
}

/// Plugin-defined phase usable by civil and military mission engines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MissionStage {
    /// Stage identifier unique within the mission.
    pub id: String,
    /// Domain phase name such as taxi, cruise, combat, or weapons_release.
    pub kind: String,
    /// Analysis declaration implementing this phase.
    pub analysis: String,
    /// Stage conditions referencing canonical parameters.
    #[serde(default)]
    pub inputs: BTreeMap<String, Binding>,
    /// Tank depletion order.
    #[serde(default)]
    pub fuel_sequence: Vec<String>,
    /// Payload released during the stage, keyed by mass item.
    #[serde(default)]
    pub payload_release: BTreeMap<String, Binding>,
    /// Whether the stage belongs to the reserve policy.
    #[serde(default)]
    pub reserve: bool,
}

/// Ordered state transitions for a vehicle mission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mission {
    /// Display label.
    pub name: String,
    /// Initial loading-case identifier.
    pub loading: Option<String>,
    /// Initial conditions in explicit units.
    #[serde(default)]
    pub conditions: BTreeMap<String, Binding>,
    /// Ordered phases.
    pub stages: Vec<MissionStage>,
    /// Alternative or reserve paths branching from a nominal stage boundary.
    #[serde(default)]
    pub branches: BTreeMap<String, MissionBranch>,
    /// Piecewise-linear allowable longitudinal CG interval versus mass, in vehicle axes.
    #[serde(default)]
    pub cg_envelope: Vec<CgLimit>,
}

/// One mass station of a longitudinal CG envelope; extrapolation is not permitted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CgLimit {
    /// Total vehicle mass.
    pub mass: Binding,
    /// Forward allowable CG coordinate.
    pub minimum: Binding,
    /// Aft allowable CG coordinate.
    pub maximum: Binding,
}

/// Independent contingency path; its fuel and cost do not accumulate into the normal trip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MissionBranch {
    /// Completed nominal stage to branch from, or `None` for the initial state.
    pub from_stage: Option<String>,
    /// Ordered state transitions for this scenario.
    pub stages: Vec<MissionStage>,
}

impl Mission {
    /// All declared phases, including independent contingency scenarios.
    pub fn all_stages(&self) -> impl Iterator<Item = &MissionStage> {
        self.stages
            .iter()
            .chain(self.branches.values().flat_map(|branch| &branch.stages))
    }
}

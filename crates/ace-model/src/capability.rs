use crate::{ModelResult, VehicleModel};
use serde::{Deserialize, Serialize};

/// Canonical vehicle parameters that supply each fleet-level capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityMap {
    /// Average production cost per vehicle.
    pub unit_cost: String,
    /// Flight time available per sortie.
    pub endurance: String,
    /// Transit speed to and from the station.
    pub cruise_speed: String,
    /// Operating cost per flight hour, excluding crew stations.
    pub operating_cost_per_hour: String,
    /// Probability that a sortie ends with the vehicle lost.
    pub loss_per_sortie: String,
    /// Ground time between sorties.
    pub turnaround: String,
}

impl CapabilityMap {
    /// Every mapped canonical parameter with its capability name.
    pub fn entries(&self) -> [(&'static str, &str); 6] {
        [
            ("unit_cost", &self.unit_cost),
            ("endurance", &self.endurance),
            ("cruise_speed", &self.cruise_speed),
            ("operating_cost_per_hour", &self.operating_cost_per_hour),
            ("loss_per_sortie", &self.loss_per_sortie),
            ("turnaround", &self.turnaround),
        ]
    }

    /// Check that every mapped parameter is declared by the vehicle.
    pub fn validate(&self, model: &VehicleModel) -> ModelResult<()> {
        for (_, parameter) in self.entries() {
            model.parameter(parameter)?;
        }
        Ok(())
    }
}

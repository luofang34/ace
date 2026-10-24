use crate::{StudyError, StudyResult};
use ace_engine::{Snapshot, ValueRecord, ValueStatus};
pub use ace_model::CapabilityMap;
use ace_model::{Quantity, Unit, VehicleModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Fleet-level capability values in canonical units.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityValues {
    /// Average production cost per vehicle.
    pub unit_cost: Quantity,
    /// Flight time available per sortie.
    pub endurance: Quantity,
    /// Transit speed.
    pub cruise_speed: Quantity,
    /// Operating cost per flight hour.
    pub operating_cost_per_hour: Quantity,
    /// Probability that a sortie ends with the vehicle lost.
    pub loss_per_sortie: Quantity,
    /// Ground time between sorties.
    pub turnaround: Quantity,
}

/// Content-addressed statement of what one exact vehicle revision can do and cost,
/// traceable to the calculations that produced each value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VehicleCapability {
    /// SHA-256 of this record with this field empty.
    pub id: String,
    /// Vehicle identifier.
    pub vehicle_id: String,
    /// Model revision.
    pub revision: u64,
    /// Digest of the authoritative model declarations.
    pub model_digest: String,
    /// Build that produced the snapshot.
    pub build: ace_engine::BuildIdentity,
    /// Parameter mapping used to read the snapshot.
    pub map: CapabilityMap,
    /// Capability values.
    pub values: CapabilityValues,
    /// Source value record per capability, including producing run identifiers.
    pub evidence: BTreeMap<String, ValueRecord>,
    /// Whether any capability rests on approximate evidence.
    pub approximate: bool,
}

impl VehicleCapability {
    /// Read the mapped values from a snapshot of this model; every value must be usable.
    pub fn from_snapshot(
        model: &VehicleModel,
        snapshot: &Snapshot,
        map: &CapabilityMap,
    ) -> StudyResult<Self> {
        if snapshot.vehicle_id != model.id || snapshot.revision != model.revision {
            return Err(StudyError::invalid(
                "capability",
                "snapshot does not belong to this model revision",
            ));
        }
        let mut evidence = BTreeMap::new();
        let mut read = |field: &str, parameter: &str, unit: Unit| -> StudyResult<Quantity> {
            let record = snapshot
                .values
                .get(parameter)
                .filter(|record| record.usable())
                .ok_or_else(|| {
                    StudyError::invalid(field, format!("{parameter} has no usable value"))
                })?;
            let value = record
                .quantity
                .ok_or_else(|| StudyError::invalid(field, "missing quantity"))?
                .to(unit)?;
            evidence.insert(field.to_owned(), record.clone());
            Ok(value)
        };
        let values = CapabilityValues {
            unit_cost: read("unit_cost", &map.unit_cost, Unit::Dollar)?,
            endurance: read("endurance", &map.endurance, Unit::Second)?,
            cruise_speed: read("cruise_speed", &map.cruise_speed, Unit::MeterPerSecond)?,
            operating_cost_per_hour: read(
                "operating_cost_per_hour",
                &map.operating_cost_per_hour,
                Unit::DollarPerHour,
            )?,
            loss_per_sortie: read("loss_per_sortie", &map.loss_per_sortie, Unit::Ratio)?,
            turnaround: read("turnaround", &map.turnaround, Unit::Second)?,
        };
        let approximate = evidence
            .values()
            .any(|record| record.status == ValueStatus::Approximate);
        let mut capability = Self {
            id: String::new(),
            vehicle_id: model.id.clone(),
            revision: model.revision,
            model_digest: snapshot.model_digest.clone(),
            build: snapshot.build.clone(),
            map: map.clone(),
            values,
            evidence,
            approximate,
        };
        capability.validate()?;
        capability.id = crate::record::digest(&capability)?;
        Ok(capability)
    }

    /// Read the capability map the vehicle model declares for itself.
    pub fn from_declared(model: &VehicleModel, snapshot: &Snapshot) -> StudyResult<Self> {
        let map = model.capabilities.as_ref().ok_or_else(|| {
            StudyError::invalid(&model.id, "the vehicle declares no capability map")
        })?;
        Self::from_snapshot(model, snapshot, map)
    }

    /// Check physical ranges and the content address.
    pub fn verify(&self) -> StudyResult<()> {
        self.validate()?;
        let mut copy = self.clone();
        copy.id.clear();
        if crate::record::digest(&copy)? != self.id {
            return Err(StudyError::invalid(
                &self.vehicle_id,
                "capability record digest does not match its data",
            ));
        }
        Ok(())
    }

    fn validate(&self) -> StudyResult<()> {
        let values = &self.values;
        let loss = values.loss_per_sortie.to(Unit::Ratio)?.value;
        let positive = [values.endurance, values.cruise_speed]
            .iter()
            .all(|value| value.value > 0.0);
        let nonnegative = [
            values.unit_cost,
            values.operating_cost_per_hour,
            values.turnaround,
        ]
        .iter()
        .all(|value| value.value >= 0.0);
        if !positive || !nonnegative || !(0.0..=1.0).contains(&loss) {
            return Err(StudyError::invalid(
                &self.vehicle_id,
                "capability needs positive endurance and speed, nonnegative costs and turnaround, and a loss probability in [0, 1]",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;

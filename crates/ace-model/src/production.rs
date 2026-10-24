use crate::{Binding, ModelError, ModelResult, Unit, VehicleModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Purchased raw material and the ratio of bought to finished mass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Material {
    /// Price per purchased kilogram.
    pub cost_per_kg: Binding,
    /// Purchased mass divided by finished mass; at least one.
    pub buy_to_fly: Binding,
}

/// Fabrication process with first-unit labor content, tooling and learning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Process {
    /// First-unit touch labor per finished kilogram.
    pub hours_per_kg: Binding,
    /// Non-recurring tooling and fixtures, amortized over the production quantity.
    pub tooling: Binding,
    /// Wright learning rate: labor of unit 2n divided by labor of unit n, in (0, 1].
    pub learning: Binding,
}

/// Fabricated part whose finished mass comes from a declared mass item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Part {
    /// Mass item supplying the finished mass of one part.
    pub mass_item: String,
    /// Material catalog entry.
    pub material: String,
    /// Process catalog entry.
    pub process: String,
    /// Parts per vehicle.
    #[serde(default = "one")]
    pub count: u32,
}

/// Bought-in component such as an engine, battery, sensor or flight computer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PurchasedItem {
    /// Price per component at the declared production quantity.
    pub unit_cost: Binding,
    /// Components per vehicle.
    #[serde(default = "one")]
    pub count: u32,
}

/// Bill of materials, processes and quantity used to estimate unit production cost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Production {
    /// Material catalog.
    pub materials: BTreeMap<String, Material>,
    /// Process catalog.
    pub processes: BTreeMap<String, Process>,
    /// Fabricated parts.
    pub parts: BTreeMap<String, Part>,
    /// Bought-in components.
    #[serde(default)]
    pub purchased: BTreeMap<String, PurchasedItem>,
    /// Fully burdened labor cost per hour.
    pub labor_rate: Binding,
    /// Vehicles produced over which learning and tooling are evaluated; a whole number.
    pub quantity: Binding,
}

fn one() -> u32 {
    1
}

impl Production {
    /// Every dimensional dependency with its required unit.
    pub fn bindings(&self) -> Vec<(&Binding, Unit)> {
        use Unit::*;
        let mut values = vec![(&self.labor_rate, DollarPerHour), (&self.quantity, Ratio)];
        for material in self.materials.values() {
            values.extend([
                (&material.cost_per_kg, DollarPerKilogram),
                (&material.buy_to_fly, Ratio),
            ]);
        }
        for process in self.processes.values() {
            values.extend([
                (&process.hours_per_kg, HourPerKilogram),
                (&process.tooling, Dollar),
                (&process.learning, Ratio),
            ]);
        }
        values.extend(
            self.purchased
                .values()
                .map(|item| (&item.unit_cost, Dollar)),
        );
        values
    }

    /// Check catalog references and counts against the vehicle declarations.
    pub fn validate(&self, model: &VehicleModel) -> ModelResult<()> {
        if self.parts.is_empty() && self.purchased.is_empty() {
            return Err(ModelError::invalid(
                "production",
                "declare at least one part or purchased item",
            ));
        }
        for (id, part) in &self.parts {
            if !model.mass_items.contains_key(&part.mass_item)
                || !self.materials.contains_key(&part.material)
                || !self.processes.contains_key(&part.process)
            {
                return Err(ModelError::invalid(
                    id,
                    "part references an unknown mass item, material or process",
                ));
            }
        }
        if self
            .parts
            .values()
            .map(|part| part.count)
            .chain(self.purchased.values().map(|item| item.count))
            .any(|count| count == 0)
        {
            return Err(ModelError::invalid(
                "production",
                "part and purchased counts must be positive",
            ));
        }
        Ok(())
    }

    /// Mass items whose bindings determine fabricated mass.
    pub fn mass_items(&self) -> impl Iterator<Item = &str> {
        self.parts.values().map(|part| part.mass_item.as_str())
    }
}

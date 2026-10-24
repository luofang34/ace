use crate::bindings::{nonnegative, scalar};
use crate::{PhysicsError, PhysicsResult};
use ace_model::{MassItem, Production, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Largest quantity summed unit by unit; larger programs need a closed-form estimator.
const MAX_QUANTITY: f64 = 1_000_000.0;

/// Modelling scope retained with every estimate.
const ASSUMPTIONS: &str = "Wright cumulative-unit learning per process at a constant burdened labor rate; tooling amortized over the declared quantity; scrap, overhead, profit and schedule effects only as included in declared rates";

/// Material, labor and tooling attributed to one fabricated part line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PartCost {
    /// Finished mass per vehicle, all instances.
    pub finished_mass_kg: f64,
    /// Purchased material mass per vehicle.
    pub raw_mass_kg: f64,
    /// Material cost per vehicle.
    pub material_usd: f64,
    /// Touch labor of the first vehicle.
    pub first_unit_hours: f64,
    /// Average touch labor per vehicle over the production quantity.
    pub average_hours: f64,
}

/// Unit production cost with the bill-of-materials evidence behind it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProductionReport {
    /// Estimator scope; results outside it are not claimed.
    pub assumptions: String,
    /// Vehicles produced.
    pub quantity: u64,
    /// Per-part material and labor.
    pub parts: BTreeMap<String, PartCost>,
    /// Bought-in cost per vehicle by item.
    pub purchased_usd: BTreeMap<String, f64>,
    /// Tooling per process, counted once per process used.
    pub tooling_usd: BTreeMap<String, f64>,
    /// Fabricated finished mass per vehicle.
    pub structure_mass_kg: f64,
    /// Average touch labor per vehicle.
    pub labor_hours: f64,
    /// First vehicle's recurring cost, excluding tooling.
    pub first_unit_cost_usd: f64,
    /// Average recurring cost per vehicle, excluding tooling.
    pub recurring_unit_cost_usd: f64,
    /// Average cost per vehicle including amortized tooling.
    pub unit_cost_usd: f64,
}

/// Estimate unit cost from parts, materials, processes, learning and quantity.
pub fn evaluate_production(
    production: &Production,
    mass_items: &BTreeMap<String, MassItem>,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<ProductionReport> {
    let quantity = scalar(&production.quantity, values, Unit::Ratio)?;
    if !(1.0..=MAX_QUANTITY).contains(&quantity) || quantity.fract() != 0.0 {
        return Err(PhysicsError::invalid(
            "production.quantity",
            "must be a whole number from 1 through 1000000",
        ));
    }
    let rate = nonnegative(
        "production.labor_rate",
        scalar(&production.labor_rate, values, Unit::DollarPerHour)?,
    )?;
    let mut report = ProductionReport {
        assumptions: ASSUMPTIONS.into(),
        quantity: quantity as u64,
        parts: BTreeMap::new(),
        purchased_usd: BTreeMap::new(),
        tooling_usd: BTreeMap::new(),
        structure_mass_kg: 0.0,
        labor_hours: 0.0,
        first_unit_cost_usd: 0.0,
        recurring_unit_cost_usd: 0.0,
        unit_cost_usd: 0.0,
    };
    for (id, part) in &production.parts {
        let cost = part_cost(production, id, part, mass_items, values, report.quantity)?;
        let process = production
            .processes
            .get(&part.process)
            .ok_or_else(|| PhysicsError::invalid(id, "part references an unknown process"))?;
        let tooling = nonnegative(id, scalar(&process.tooling, values, Unit::Dollar)?)?;
        report.tooling_usd.insert(part.process.clone(), tooling);
        report.parts.insert(id.clone(), cost);
    }
    for (id, item) in &production.purchased {
        let price = nonnegative(id, scalar(&item.unit_cost, values, Unit::Dollar)?)?;
        report
            .purchased_usd
            .insert(id.clone(), price * f64::from(item.count));
    }
    let parts = report.parts.values();
    let material: f64 = parts.clone().map(|part| part.material_usd).sum();
    let bought: f64 = report.purchased_usd.values().sum();
    report.structure_mass_kg = parts.clone().map(|part| part.finished_mass_kg).sum();
    report.labor_hours = parts.clone().map(|part| part.average_hours).sum();
    let first_hours: f64 = parts.map(|part| part.first_unit_hours).sum();
    report.first_unit_cost_usd = material + bought + first_hours * rate;
    report.recurring_unit_cost_usd = material + bought + report.labor_hours * rate;
    report.unit_cost_usd =
        report.recurring_unit_cost_usd + report.tooling_usd.values().sum::<f64>() / quantity;
    Ok(report)
}

fn part_cost(
    production: &Production,
    id: &str,
    part: &ace_model::Part,
    mass_items: &BTreeMap<String, MassItem>,
    values: &BTreeMap<String, Quantity>,
    quantity: u64,
) -> PhysicsResult<PartCost> {
    let unknown = || PhysicsError::invalid(id, "part references an unknown catalog entry");
    let item = mass_items.get(&part.mass_item).ok_or_else(unknown)?;
    let material = production
        .materials
        .get(&part.material)
        .ok_or_else(unknown)?;
    let process = production
        .processes
        .get(&part.process)
        .ok_or_else(unknown)?;
    let finished =
        nonnegative(id, scalar(&item.mass, values, Unit::Kilogram)?)? * f64::from(part.count);
    let buy_to_fly = scalar(&material.buy_to_fly, values, Unit::Ratio)?;
    let learning = scalar(&process.learning, values, Unit::Ratio)?;
    if !(buy_to_fly >= 1.0 && buy_to_fly.is_finite()) || !(learning > 0.0 && learning <= 1.0) {
        return Err(PhysicsError::invalid(
            id,
            "buy-to-fly must be at least one and learning must lie in (0, 1]",
        ));
    }
    let raw = finished * buy_to_fly;
    let first = finished * scalar(&process.hours_per_kg, values, Unit::HourPerKilogram)?;
    Ok(PartCost {
        finished_mass_kg: finished,
        raw_mass_kg: raw,
        material_usd: raw
            * nonnegative(
                id,
                scalar(&material.cost_per_kg, values, Unit::DollarPerKilogram)?,
            )?,
        first_unit_hours: nonnegative(id, first)?,
        average_hours: first * average_learning_factor(learning, quantity),
    })
}

/// Mean of `n^log2(learning)` over units 1..=quantity (Wright cumulative-unit model).
pub(crate) fn average_learning_factor(learning: f64, quantity: u64) -> f64 {
    let exponent = learning.log2();
    let total: f64 = (1..=quantity)
        .map(|unit| (unit as f64).powf(exponent))
        .sum();
    total / quantity as f64
}

#[cfg(test)]
mod tests;

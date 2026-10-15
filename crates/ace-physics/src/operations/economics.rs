use super::{OperationsReport, evaluate_trip};
use crate::{
    PhysicsError, PhysicsResult,
    bindings::{nonnegative, scalar},
};
use ace_model::{Binding, CostItem, EconomicModel, OperatingScenario, Operations, Quantity, Unit};
use std::collections::BTreeMap;

pub(super) fn evaluate(
    operations: &Operations,
    model: &EconomicModel,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<OperationsReport> {
    let mut scenarios = BTreeMap::new();
    for (id, scenario) in &model.scenarios {
        let (costs, annual_flights) = costs(model, scenario, values)?;
        let mut route = operations.clone();
        route.economics = None;
        route.distance = scenario.distance.clone();
        route.block_duration = Some(scenario.block_duration.clone());
        if let Some(factor) = &scenario.load_factor {
            route.load_factor = Some(factor.clone());
        }
        if let Some(airports) = &scenario.airports {
            route.airports = airports.clone();
        }
        route.costs.extend(costs);
        let mut report = evaluate_trip(&route, values)?;
        report.annual_flights = Some(annual_flights);
        report.scenario = Some(id.clone());
        scenarios.insert(id.clone(), report);
    }
    let mut selected = scenarios.get(&model.selected).cloned().ok_or_else(|| {
        PhysicsError::invalid(
            &model.selected,
            "selected operating scenario is unavailable",
        )
    })?;
    selected.network_compatible = scenarios.values().all(|report| report.network_compatible);
    selected.scenarios = scenarios;
    Ok(selected)
}

fn costs(
    model: &EconomicModel,
    scenario: &OperatingScenario,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<(Vec<CostItem>, f64)> {
    let duration = positive(
        "block duration",
        &scenario.block_duration,
        values,
        Unit::Hour,
    )?;
    let turnaround = read(&scenario.turnaround, values, Unit::Hour)?;
    let fuel = read(&scenario.block_fuel, values, Unit::Kilogram)?;
    let flights = annual_flights(model, values, duration, turnaround)?;
    let rates = &model.rates;
    let amounts = [
        (
            "fuel",
            fuel * read(&rates.fuel, values, Unit::DollarPerKilogram)?,
        ),
        (
            "crew",
            duration * read(&rates.crew_flight_hour, values, Unit::DollarPerHour)?,
        ),
        (
            "maintenance_flight",
            duration * read(&rates.maintenance_flight_hour, values, Unit::DollarPerHour)?,
        ),
        (
            "maintenance_cycle",
            read(&rates.maintenance_cycle, values, Unit::Dollar)?,
        ),
        (
            "turnaround",
            turnaround * read(&rates.turnaround_hour, values, Unit::DollarPerHour)?,
        ),
        (
            "ownership",
            read(&rates.annual_ownership, values, Unit::Dollar)? / flights,
        ),
    ];
    let costs = amounts
        .into_iter()
        .map(|(id, value)| {
            Ok(CostItem {
                id: format!("rate.{id}"),
                category: id.into(),
                basis: None,
                amount: Binding::Constant(Quantity::new(value, Unit::Dollar)?),
            })
        })
        .collect::<PhysicsResult<Vec<_>>>()?;
    Ok((costs, flights))
}

fn annual_flights(
    model: &EconomicModel,
    values: &BTreeMap<String, Quantity>,
    duration: f64,
    turnaround: f64,
) -> PhysicsResult<f64> {
    let block_time = positive(
        "annual block time",
        &model.utilization.flight_time,
        values,
        Unit::Hour,
    )?;
    let available_time = positive(
        "annual operating time",
        &model.utilization.operating_time,
        values,
        Unit::Hour,
    )?;
    let flights = (block_time / duration).min(available_time / (duration + turnaround));
    if !flights.is_finite() || flights <= 0.0 {
        return Err(PhysicsError::invalid(
            "annual flights",
            "utilization must yield a finite positive cycle count",
        ));
    }
    Ok(flights)
}

fn read(binding: &Binding, values: &BTreeMap<String, Quantity>, unit: Unit) -> PhysicsResult<f64> {
    nonnegative(
        "economic rate or mission input",
        scalar(binding, values, unit)?,
    )
}

fn positive(
    id: &str,
    binding: &Binding,
    values: &BTreeMap<String, Quantity>,
    unit: Unit,
) -> PhysicsResult<f64> {
    let value = read(binding, values, unit)?;
    if value <= 0.0 {
        return Err(PhysicsError::invalid(id, "must be positive"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests;

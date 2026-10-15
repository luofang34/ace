use crate::bindings::{nonnegative, scalar};
use crate::{PhysicsError, PhysicsResult};
use ace_model::{Operations, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod economics;

/// One network endpoint's physical compatibility and charge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirportResult {
    /// Airport/gate identifier.
    pub id: String,
    /// Span slack, metres.
    pub span_margin_m: f64,
    /// Runway slack, metres.
    pub runway_margin_m: f64,
    /// Whether both physical conditions are satisfied.
    pub compatible: bool,
    /// Per-movement charge, US dollars.
    pub movement_fee_usd: f64,
}

/// Trip-cost and seat-distance economics with explicit operating assumptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsReport {
    /// Monetary basis; absence means price-year/scope evidence is unavailable.
    pub basis: Option<ace_model::EconomicBasis>,
    /// Selected rate-based scenario, when supplied.
    pub scenario: Option<String>,
    /// Achievable cycles per year under flight and turnaround availability.
    pub annual_flights: Option<f64>,
    /// Required route scenarios evaluated independently with the same cost basis.
    pub scenarios: BTreeMap<String, OperationsReport>,
    /// Seats installed.
    pub seats: u32,
    /// Occupied fraction.
    pub load_factor: f64,
    /// Revenue stage length, metres.
    pub distance_m: f64,
    /// All cost terms, including each selected airport movement.
    pub costs_usd: BTreeMap<String, f64>,
    /// Total trip cost.
    pub trip_cost_usd: f64,
    /// Cost per available seat nautical mile.
    pub casm_usd_per_nmi: f64,
    /// Cost per occupied seat nautical mile; undefined at zero load factor.
    pub passenger_cost_usd_per_nmi: Option<f64>,
    /// Revenue with an explicit per-passenger yield.
    pub revenue_usd: Option<f64>,
    /// Revenue less total trip cost.
    pub profit_usd: Option<f64>,
    /// Airport and gate compatibility evidence.
    pub airports: Vec<AirportResult>,
    /// Whether every endpoint is physically compatible.
    pub network_compatible: bool,
}

/// Calculate cost and compatibility together so airport violations cannot disappear into CASM.
pub fn evaluate_operations(
    operations: &Operations,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<OperationsReport> {
    operations.validate_economics()?;
    match &operations.economics {
        Some(model) => economics::evaluate(operations, model, values),
        None => evaluate_trip(operations, values),
    }
}

fn evaluate_trip(
    operations: &Operations,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<OperationsReport> {
    let seats = seat_count(operations, values)?;
    let load_factor = scalar(&operations.load_factor, values, Unit::Ratio)?;
    if !(0.0..=1.0).contains(&load_factor) {
        return Err(PhysicsError::invalid(
            "operations.load_factor",
            "must be between zero and one",
        ));
    }
    let distance_m = scalar(&operations.distance, values, Unit::Meter)?;
    if distance_m <= 0.0 {
        return Err(PhysicsError::invalid(
            "operations.distance",
            "distance must be positive",
        ));
    }
    let (costs, airports) = trip_costs(operations, values)?;
    let trip_cost_usd = costs.values().sum::<f64>();
    nonnegative("operations.trip_cost", trip_cost_usd)?;
    let casm = trip_cost_usd / (seats * distance_m / 1852.0);
    nonnegative("operations.casm", casm)?;
    let revenue = operations
        .revenue_per_passenger
        .as_ref()
        .map(|value| {
            scalar(value, values, Unit::Dollar)
                .and_then(|yield_| nonnegative("operations.yield", yield_))
        })
        .transpose()?
        .map(|yield_| yield_ * seats * load_factor);
    if let Some(value) = revenue {
        nonnegative("operations.revenue", value)?;
    }
    let passenger_cost = if load_factor > 0.0 {
        Some(nonnegative(
            "operations.passenger_cost",
            casm / load_factor,
        )?)
    } else {
        None
    };
    Ok(OperationsReport {
        basis: operations.basis.clone(),
        scenario: None,
        annual_flights: None,
        scenarios: BTreeMap::new(),
        seats: seats as u32,
        load_factor,
        distance_m,
        costs_usd: costs,
        trip_cost_usd,
        casm_usd_per_nmi: casm,
        passenger_cost_usd_per_nmi: passenger_cost,
        revenue_usd: revenue,
        profit_usd: revenue.map(|value| value - trip_cost_usd),
        network_compatible: airports.iter().all(|airport| airport.compatible),
        airports,
    })
}

fn airport_results(
    operations: &Operations,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<Vec<AirportResult>> {
    let mut result = Vec::new();
    for airport in &operations.airports {
        let span = operations.span.as_ref().ok_or_else(|| {
            PhysicsError::invalid(&airport.id, "airport compatibility requires vehicle span")
        })?;
        let required = operations.required_runway.as_ref().ok_or_else(|| {
            PhysicsError::invalid(
                &airport.id,
                "airport compatibility requires runway performance",
            )
        })?;
        let span = nonnegative(&airport.id, scalar(span, values, Unit::Meter)?)?;
        let runway = nonnegative(&airport.id, scalar(required, values, Unit::Meter)?)?;
        let maximum_span = nonnegative(
            &airport.id,
            scalar(&airport.maximum_span, values, Unit::Meter)?,
        )?;
        let available = nonnegative(&airport.id, scalar(&airport.runway, values, Unit::Meter)?)?;
        let span_margin_m = maximum_span - span;
        let runway_margin_m = available - runway;
        result.push(AirportResult {
            id: airport.id.clone(),
            span_margin_m,
            runway_margin_m,
            compatible: span_margin_m >= 0.0 && runway_margin_m >= 0.0,
            movement_fee_usd: nonnegative(
                &airport.id,
                scalar(&airport.movement_fee, values, Unit::Dollar)?,
            )?,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests;

fn trip_costs(
    operations: &Operations,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<(BTreeMap<String, f64>, Vec<AirportResult>)> {
    let mut costs = BTreeMap::new();
    for cost in &operations.costs {
        let amount = nonnegative(&cost.id, scalar(&cost.amount, values, Unit::Dollar)?)?;
        if costs.insert(cost.id.clone(), amount).is_some() {
            return Err(PhysicsError::invalid(&cost.id, "duplicate cost term"));
        }
    }
    let airports = airport_results(operations, values)?;
    for airport in &airports {
        let id = format!("airport.{}", airport.id);
        if costs.insert(id.clone(), airport.movement_fee_usd).is_some() {
            return Err(PhysicsError::invalid(id, "airport fee counted twice"));
        }
    }
    Ok((costs, airports))
}

fn seat_count(operations: &Operations, values: &BTreeMap<String, Quantity>) -> PhysicsResult<f64> {
    let seats = scalar(&operations.seats, values, Unit::Ratio)?;
    if seats < 1.0 || seats > f64::from(u32::MAX) || seats.fract() != 0.0 {
        return Err(PhysicsError::invalid(
            "operations.seats",
            "seat count must be a positive integer",
        ));
    }
    Ok(seats)
}

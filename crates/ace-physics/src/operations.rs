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

/// Trip costs and applicable normalizations with explicit operating assumptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsReport {
    /// Monetary basis; absence means price-year/scope evidence is unavailable.
    pub basis: Option<ace_model::EconomicBasis>,
    /// Selected rate-based scenario, when supplied.
    pub scenario: Option<String>,
    /// Achievable cycles per year under block-time and turnaround availability.
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
    /// Cost per available seat nautical mile; absent for zero seats or zero distance.
    pub casm_usd_per_nmi: Option<f64>,
    /// Normal-trip elapsed time, including ground phases, when supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_duration_h: Option<f64>,
    /// Total trip cost divided by normal-trip elapsed hours; not airborne-hour cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_per_block_hour_usd: Option<f64>,
    /// Block-hour cost per kilogram of declared payload; absent without payload or duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_per_payload_kg_block_hour_usd: Option<f64>,
    /// Cost per occupied seat nautical mile; absent at zero seats, distance or load factor.
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
    let load_factor = match &operations.load_factor {
        Some(binding) => scalar(binding, values, Unit::Ratio)?,
        None if seats == 0.0 => 0.0,
        None => {
            return Err(PhysicsError::invalid(
                "operations.load_factor",
                "required when seats are declared",
            ));
        }
    };
    if !(0.0..=1.0).contains(&load_factor) {
        return Err(PhysicsError::invalid(
            "operations.load_factor",
            "must be between zero and one",
        ));
    }
    let distance_m = nonnegative(
        "operations.distance",
        scalar(&operations.distance, values, Unit::Meter)?,
    )?;
    let (costs, airports) = trip_costs(operations, values)?;
    let trip_cost_usd = costs.values().sum::<f64>();
    nonnegative("operations.trip_cost", trip_cost_usd)?;
    let casm = (seats > 0.0 && distance_m > 0.0)
        .then(|| {
            nonnegative(
                "operations.casm",
                trip_cost_usd / (seats * distance_m / 1852.0),
            )
        })
        .transpose()?;
    let (block_duration_h, cost_per_block_hour_usd) =
        hourly_cost(operations, values, trip_cost_usd)?;
    let cost_per_payload_kg_block_hour_usd =
        payload_hourly_cost(operations, values, cost_per_block_hour_usd)?;
    let revenue = passenger_revenue(operations, values, seats, load_factor)?;
    let passenger_cost = if load_factor > 0.0 {
        casm.map(|cost| nonnegative("operations.passenger_cost", cost / load_factor))
            .transpose()?
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
        block_duration_h,
        cost_per_block_hour_usd,
        cost_per_payload_kg_block_hour_usd,
        passenger_cost_usd_per_nmi: passenger_cost,
        revenue_usd: revenue,
        profit_usd: revenue.map(|value| value - trip_cost_usd),
        network_compatible: airports.iter().all(|airport| airport.compatible),
        airports,
    })
}

fn passenger_revenue(
    operations: &Operations,
    values: &BTreeMap<String, Quantity>,
    seats: f64,
    load_factor: f64,
) -> PhysicsResult<Option<f64>> {
    let Some(binding) = &operations.revenue_per_passenger else {
        return Ok(None);
    };
    if seats == 0.0 {
        return Err(PhysicsError::invalid(
            "operations.revenue_per_passenger",
            "passenger yield is not applicable to a zero-seat operation",
        ));
    }
    let yield_ = nonnegative("operations.yield", scalar(binding, values, Unit::Dollar)?)?;
    nonnegative("operations.revenue", yield_ * seats * load_factor).map(Some)
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
    let Some(binding) = &operations.seats else {
        return Ok(0.0);
    };
    let seats = scalar(binding, values, Unit::Ratio)?;
    if seats < 0.0 || seats > f64::from(u32::MAX) || seats.fract() != 0.0 {
        return Err(PhysicsError::invalid(
            "operations.seats",
            "seat count must be a nonnegative integer",
        ));
    }
    Ok(seats)
}

fn payload_hourly_cost(
    operations: &Operations,
    values: &BTreeMap<String, Quantity>,
    cost_per_block_hour_usd: Option<f64>,
) -> PhysicsResult<Option<f64>> {
    let Some(binding) = &operations.payload else {
        return Ok(None);
    };
    let payload_kg = scalar(binding, values, Unit::Kilogram)?;
    if payload_kg <= 0.0 {
        return Err(PhysicsError::invalid(
            "operations.payload",
            "must be positive",
        ));
    }
    Ok(cost_per_block_hour_usd.map(|hourly| hourly / payload_kg))
}

fn hourly_cost(
    operations: &Operations,
    values: &BTreeMap<String, Quantity>,
    cost: f64,
) -> PhysicsResult<(Option<f64>, Option<f64>)> {
    let Some(binding) = &operations.block_duration else {
        return Ok((None, None));
    };
    let hours = scalar(binding, values, Unit::Hour)?;
    if hours <= 0.0 {
        return Err(PhysicsError::invalid(
            "operations.block_duration",
            "must be positive",
        ));
    }
    Ok((
        Some(hours),
        Some(nonnegative("operations.hourly_cost", cost / hours)?),
    ))
}

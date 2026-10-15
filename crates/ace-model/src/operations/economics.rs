use crate::{Airport, Binding};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Common monetary basis for every cost and revenue in an operating model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EconomicBasis {
    /// ISO currency code; the native monetary unit currently supports USD.
    pub currency: String,
    /// Calendar year of constant-price estimates.
    pub price_year: u16,
    /// Explicit scope such as direct operating cost or total ownership cost.
    pub cost_scope: String,
}

/// Canonical unit rates, all in the operating model's declared monetary basis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CostRates {
    /// Fuel price per kilogram.
    pub fuel: Binding,
    /// Crew cost per flight hour.
    pub crew_flight_hour: Binding,
    /// Maintenance cost per flight hour.
    pub maintenance_flight_hour: Binding,
    /// Maintenance cost per flight cycle.
    pub maintenance_cycle: Binding,
    /// Ground-handling cost per turnaround hour.
    pub turnaround_hour: Binding,
    /// Ownership cost per year, allocated over achievable annual flights.
    pub annual_ownership: Binding,
}

/// Fleet utilization bounds for one vehicle over a year.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnnualUtilization {
    /// Planned annual flight time.
    pub flight_time: Binding,
    /// Available operating time, including flight and turnaround.
    pub operating_time: Binding,
}

/// Route case whose achieved duration and burn bind to canonical mission calculations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OperatingScenario {
    /// Route stage length.
    pub distance: Binding,
    /// Normal-trip mission duration; contingency time is excluded.
    pub block_duration: Binding,
    /// Normal-trip fuel consumption; contingency fuel is excluded.
    pub block_fuel: Binding,
    /// Turnaround duration.
    pub turnaround: Binding,
    /// Occupied fraction, or the operating model's fraction.
    pub load_factor: Option<Binding>,
    /// Route endpoints, or the operating model's endpoints.
    pub airports: Option<Vec<Airport>>,
}

/// Explicit rate-based economics across required route scenarios.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EconomicModel {
    /// Scenario supplying the canonical trip-cost and CASM outputs.
    pub selected: String,
    /// Cost rates shared by the selected fleet.
    pub rates: CostRates,
    /// Annual flight and operating availability.
    pub utilization: AnnualUtilization,
    /// Required network scenarios; incompatible endpoints block network feasibility.
    pub scenarios: BTreeMap<String, OperatingScenario>,
}

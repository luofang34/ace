use crate::Binding;
use serde::{Deserialize, Serialize};
mod bindings;
mod economics;
pub use economics::{
    AnnualUtilization, CostRates, EconomicBasis, EconomicModel, OperatingScenario,
};

/// Airport constraints and costs supplied by the operating case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Airport {
    /// Airport or gate identifier.
    pub id: String,
    /// Maximum compatible span.
    pub maximum_span: Binding,
    /// Available runway.
    pub runway: Binding,
    /// Cost per movement.
    pub movement_fee: Binding,
}

/// One fuel, crew, maintenance, ownership, airport, or navigation cost term.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CostItem {
    /// Unique component identifier.
    pub id: String,
    /// Cost category.
    pub category: String,
    /// Cost per trip.
    pub amount: Binding,
    /// Explicit imported basis; absence inherits the operating model's basis.
    pub basis: Option<EconomicBasis>,
}

/// Operating economics with optional seat-distance normalization and explicit airport assumptions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Operations {
    /// Monetary currency, price year and scope; absence leaves economic quality unspecified.
    pub basis: Option<EconomicBasis>,
    /// Optional rate-based calculation and required route cases.
    pub economics: Option<EconomicModel>,
    /// Seats installed; zero declares a non-passenger operation.
    pub seats: Binding,
    /// Revenue distance.
    pub distance: Binding,
    /// Normal-trip elapsed time for cost per block hour; rate-based scenarios supply their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_duration: Option<Binding>,
    /// Occupied fraction.
    pub load_factor: Binding,
    /// Trip cost breakdown.
    pub costs: Vec<CostItem>,
    /// Network endpoints or compatible gates.
    #[serde(default)]
    pub airports: Vec<Airport>,
    /// Canonical span used for airport checks.
    pub span: Option<Binding>,
    /// Required runway length.
    pub required_runway: Option<Binding>,
    /// Achieved yield per occupied seat.
    pub revenue_per_passenger: Option<Binding>,
}

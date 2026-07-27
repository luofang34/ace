use serde::{Deserialize, Serialize};

use crate::domain::quantity::QuantityOutput;
use crate::domain::schema::ComponentMassStatement;

use super::ResultProvenance;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MassPropertiesAnalysis {
    pub(crate) statement: ComponentMassStatement,
    pub(crate) closure_error: QuantityOutput,
    pub(crate) states: Vec<MassPropertiesState>,
    pub(crate) minimum_center_of_gravity: QuantityOutput,
    pub(crate) maximum_center_of_gravity: QuantityOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) neutral_point: Option<QuantityOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) minimum_static_margin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) maximum_static_margin: Option<f64>,
    pub(crate) stability_supported: bool,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) provenance: ResultProvenance,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MassPropertiesState {
    pub(crate) id: String,
    pub(crate) total_mass: QuantityOutput,
    pub(crate) fuel_mass: QuantityOutput,
    pub(crate) payload_mass: QuantityOutput,
    pub(crate) center_of_gravity: QuantityOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) static_margin: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct MassPropertiesMetricSnapshot {
    pub(crate) minimum_center_of_gravity_m: f64,
    pub(crate) maximum_center_of_gravity_m: f64,
    pub(crate) minimum_static_margin: Option<f64>,
}

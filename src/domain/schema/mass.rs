use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawMass {
    pub(crate) maximum_takeoff_mass: String,
    pub(crate) operating_empty_mass: String,
    pub(crate) maximum_payload_mass: String,
    pub(crate) maximum_fuel_mass: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) components: Vec<RawComponentMass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fuel_station: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) payload_station: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawComponentMass {
    pub(crate) id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mass: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) station: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MassProperties {
    pub(crate) maximum_takeoff_mass_kg: f64,
    pub(crate) operating_empty_mass_kg: f64,
    pub(crate) maximum_payload_mass_kg: f64,
    pub(crate) maximum_fuel_mass_kg: f64,
    pub(crate) statement: ComponentMassStatement,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ComponentMassStatement {
    pub(crate) model_id: &'static str,
    pub(crate) model_version: u32,
    pub(crate) components: Vec<ComponentMass>,
    pub(crate) fuel_station: MassPropertyValue,
    pub(crate) payload_station: MassPropertyValue,
    pub(crate) closure_error_kg: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ComponentMass {
    pub(crate) component_id: String,
    pub(crate) component_kind: String,
    pub(crate) count: u32,
    pub(crate) mass: MassPropertyValue,
    pub(crate) station: MassPropertyValue,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MassPropertyValue {
    pub(crate) value: f64,
    pub(crate) unit: &'static str,
    pub(crate) provenance: MassPropertyProvenance,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MassPropertyProvenance {
    pub(crate) kind: &'static str,
    pub(crate) source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) correlation_id: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) correlation_version: Option<u32>,
    pub(crate) explicitly_provided: bool,
}

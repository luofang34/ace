use crate::{
    Analysis, Clearance, Closure, DesignVariable, GeometryObject, LoadingCase, MassItem, Mission,
    ModelError, ModelResult, Objective, Operations, Parameter, Requirement, Section,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod validation;

/// Authoritative vehicle data independent of aircraft solvers, UI, storage, or transport.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VehicleModel {
    /// Persisted schema revision.
    pub schema_version: u32,
    /// Stable project vehicle identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Domain schema label, e.g. fixed_wing, rotorcraft, or ground_vehicle.
    pub domain: String,
    /// Revision advanced by validated design edits.
    #[serde(default)]
    pub revision: u64,
    /// Canonical parameter graph.
    pub parameters: BTreeMap<String, Parameter>,
    /// Analysis plugin declarations.
    #[serde(default)]
    pub analyses: BTreeMap<String, Analysis>,
    /// Explicitly permitted dependency cycles.
    #[serde(default)]
    pub closures: BTreeMap<String, Closure>,
    /// Computable requirements.
    #[serde(default)]
    pub requirements: BTreeMap<String, Requirement>,
    /// Semantic geometry and reserved volumes.
    #[serde(default)]
    pub geometry: BTreeMap<String, GeometryObject>,
    /// Required pairwise clearances.
    #[serde(default)]
    pub clearances: BTreeMap<String, Clearance>,
    /// Requested critical stations.
    #[serde(default)]
    pub sections: BTreeMap<String, Section>,
    /// Non-overlapping mass contributions.
    #[serde(default)]
    pub mass_items: BTreeMap<String, MassItem>,
    /// Payload/fuel configurations.
    #[serde(default)]
    pub loading_cases: BTreeMap<String, LoadingCase>,
    /// Ordered missions.
    #[serde(default)]
    pub missions: BTreeMap<String, Mission>,
    /// Operations and economics for the operating case.
    pub operations: Option<Operations>,
    /// Bill of materials and processes for production cost estimation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production: Option<crate::Production>,
    /// Independent optimization objectives.
    #[serde(default)]
    pub objectives: BTreeMap<String, Objective>,
    /// Inputs varied by design studies.
    #[serde(default)]
    pub design_variables: Vec<DesignVariable>,
}

impl VehicleModel {
    /// Validate dimensional fields, reference integrity, and schema invariants.
    pub fn validate(&self) -> ModelResult<()> {
        if self.schema_version != 1 || self.id.is_empty() || self.domain.is_empty() {
            return Err(ModelError::invalid(
                "vehicle",
                "schema_version must be 1 and id/domain nonempty",
            ));
        }
        for (name, parameter) in &self.parameters {
            validate_identifier(name)?;
            parameter.validate(name)?;
            if let Some(expression) = &parameter.expression {
                for dependency in expression.dependencies()? {
                    self.parameter(&dependency)?;
                }
            }
        }
        validation::validate(self)
    }

    /// Retrieve a declared parameter, preserving the missing identifier in errors.
    pub fn parameter(&self, name: &str) -> ModelResult<&Parameter> {
        self.parameters
            .get(name)
            .ok_or_else(|| ModelError::UnknownReference(name.to_owned()))
    }
}

fn validate_identifier(name: &str) -> ModelResult<()> {
    let valid = name.split('.').all(|part| {
        !part.is_empty()
            && part
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_alphabetic())
            && part
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
    });
    if !valid {
        return Err(ModelError::invalid(
            name,
            "expected a dotted canonical parameter identifier",
        ));
    }
    Ok(())
}

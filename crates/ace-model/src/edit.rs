use crate::{
    Analysis, Clearance, Closure, DesignVariable, GeometryObject, LoadingCase, MassItem, Mission,
    ModelError, ModelResult, Objective, Operations, Parameter, Requirement, Section, VehicleModel,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Typed structural changes applied together before reference validation and graph compilation.
/// `value: null` removes a named object; otherwise the declaration is inserted or replaced.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelEdit {
    /// Canonical parameter declaration.
    Parameter {
        /// Canonical identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<Parameter>,
    },
    /// Semantic geometry declaration.
    Geometry {
        /// Object identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<GeometryObject>,
    },
    /// Non-overlapping mass contribution.
    MassItem {
        /// Item identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<MassItem>,
    },
    /// Ordered mission and its contingency branches.
    Mission {
        /// Mission identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<Mission>,
    },
    /// Computable requirement.
    Requirement {
        /// Requirement identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<Requirement>,
    },
    /// Operating case and economic assumptions.
    Operations {
        /// Replacement, or removal of the operating case.
        value: Option<Operations>,
    },
    /// Solver declaration, including state-driven phases.
    Analysis {
        /// Analysis identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<Analysis>,
    },
    /// Explicit bounded dependency closure.
    Closure {
        /// Closure identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<Closure>,
    },
    /// Requested geometry clearance.
    Clearance {
        /// Clearance identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<Clearance>,
    },
    /// Requested section.
    Section {
        /// Section identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<Section>,
    },
    /// Loading distribution.
    LoadingCase {
        /// Loading identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<LoadingCase>,
    },
    /// Independent design objective.
    Objective {
        /// Objective identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<Objective>,
    },
    /// Complete design-variable search bounds.
    DesignVariables {
        /// Replacement bounds.
        values: Vec<DesignVariable>,
    },
    /// Complete production bill.
    Production {
        /// Replacement, or removal of the bill.
        value: Option<crate::Production>,
    },
    /// One fabricated part, such as a material or process choice from the catalog.
    ProductionPart {
        /// Part identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<crate::Part>,
    },
    /// One reusable catalog entry, such as a propulsion profile.
    CatalogEntry {
        /// Entry identifier.
        id: String,
        /// Replacement, or removal.
        value: Option<crate::CatalogEntry>,
    },
}

impl ModelEdit {
    /// Apply to a private working copy; the caller must validate the complete transaction.
    pub fn apply(&self, model: &mut VehicleModel) -> ModelResult<()> {
        match self {
            Self::Parameter { id, value } => replace(&mut model.parameters, id, value),
            Self::Geometry { id, value } => replace(&mut model.geometry, id, value),
            Self::MassItem { id, value } => replace(&mut model.mass_items, id, value),
            Self::Mission { id, value } => replace(&mut model.missions, id, value),
            Self::Requirement { id, value } => replace(&mut model.requirements, id, value),
            Self::Analysis { id, value } => replace(&mut model.analyses, id, value),
            Self::Closure { id, value } => replace(&mut model.closures, id, value),
            Self::Clearance { id, value } => replace(&mut model.clearances, id, value),
            Self::Section { id, value } => replace(&mut model.sections, id, value),
            Self::LoadingCase { id, value } => replace(&mut model.loading_cases, id, value),
            Self::Objective { id, value } => replace(&mut model.objectives, id, value),
            Self::Operations { value } => {
                model.operations = value.clone();
                Ok(())
            }
            Self::DesignVariables { values } => {
                model.design_variables = values.clone();
                Ok(())
            }
            Self::Production { value } => {
                model.production = value.clone();
                Ok(())
            }
            Self::ProductionPart { id, value } => {
                let bill = model.production.as_mut().ok_or_else(|| {
                    crate::ModelError::invalid(id, "no production bill is defined")
                })?;
                replace(&mut bill.parts, id, value)
            }
            Self::CatalogEntry { id, value } => replace(&mut model.catalog, id, value),
        }
    }
}

fn replace<T: Clone>(
    map: &mut BTreeMap<String, T>,
    id: &str,
    value: &Option<T>,
) -> ModelResult<()> {
    if id.is_empty() {
        return Err(ModelError::invalid("edit", "identifier cannot be empty"));
    }
    match value {
        Some(value) => {
            map.insert(id.into(), value.clone());
        }
        None => {
            map.remove(id)
                .ok_or_else(|| ModelError::UnknownReference(id.into()))?;
        }
    }
    Ok(())
}

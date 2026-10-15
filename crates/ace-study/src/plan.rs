use crate::{StudyError, StudyResult};
use ace_model::{Quantity, VehicleModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod validation;

/// Search strategy; every evaluation uses the authoritative synthesis graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SearchMethod {
    /// Cartesian product of explicit, unit-bearing samples.
    Grid {
        /// Parameter identifiers mapped to sampled values.
        axes: BTreeMap<String, Vec<Quantity>>,
    },
    /// Pareto pattern search, halving each variable's step at each level.
    Refine {
        /// Bounded number of step reductions, from 1 through 52.
        levels: u32,
        /// Selected declared variables; empty selects all.
        #[serde(default)]
        variables: Vec<String>,
    },
}

/// Portable search instructions; objectives and bounds remain in the vehicle model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StudyPlan {
    /// Plan schema identifier; must be 1.
    pub schema_version: u32,
    /// Maximum number of distinct candidate calculations.
    pub max_evaluations: u32,
    /// Sampling or optimization strategy.
    pub method: SearchMethod,
}

#[derive(Clone)]
pub(crate) struct Variable {
    pub name: String,
    pub lower: Quantity,
    pub upper: Quantity,
}

impl StudyPlan {
    /// Check units, budgets, writable inputs, and declared design bounds.
    pub fn validate(&self, model: &VehicleModel) -> StudyResult<()> {
        validation::validate(self, model)
    }
}

pub(crate) fn variables(model: &VehicleModel, names: &[String]) -> StudyResult<Vec<Variable>> {
    let selected: Vec<_> = if names.is_empty() {
        model
            .design_variables
            .iter()
            .map(|v| v.parameter.clone())
            .collect()
    } else {
        names.to_vec()
    };
    selected
        .iter()
        .map(|name| {
            let variable = model
                .design_variables
                .iter()
                .find(|v| &v.parameter == name)
                .ok_or_else(|| StudyError::invalid(name, "not a declared design variable"))?;
            let unit = model.parameter(name)?.unit;
            Ok(Variable {
                name: name.clone(),
                lower: variable.lower.to(unit)?,
                upper: variable.upper.to(unit)?,
            })
        })
        .collect()
}

pub(crate) fn sample(
    model: &VehicleModel,
    variable: &Variable,
    value: Quantity,
) -> StudyResult<Quantity> {
    let value = model.parameter(&variable.name)?.check_value(value)?;
    if value.value < variable.lower.value || value.value > variable.upper.value {
        return Err(StudyError::invalid(
            &variable.name,
            "sample lies outside declared design bounds",
        ));
    }
    Ok(value)
}

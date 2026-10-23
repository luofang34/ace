use crate::{Expression, ModelError, ModelResult, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Source evidence for inputs, geometry, requirements, and calculations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Origin such as user, reference, estimated, or derived.
    pub kind: String,
    /// Document, dataset, calculation, or asset reference.
    pub reference: String,
    /// Revision of the referenced source.
    pub revision: Option<String>,
}

/// Declared uncertainty; absence means unknown rather than exact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Uncertainty {
    /// Input standard deviation in compatible units.
    pub standard_deviation: Quantity,
    /// Distribution assumed for a probabilistic study: `normal` about the declared
    /// value, or `uniform` with the same standard deviation.
    pub distribution: String,
}

/// One authoritative design parameter or computed output declaration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    /// Unit used by the model and result projection.
    pub unit: Unit,
    /// Input value, or explicit initial seed for a closure output.
    pub value: Option<Quantity>,
    /// Dimensional expression; analysis outputs have no expression.
    pub expression: Option<Expression>,
    /// Origin of this parameter's declaration.
    pub source: Source,
    /// Responsible discipline or editor.
    pub owner: String,
    /// Revision advanced by validated edit transactions.
    #[serde(default)]
    pub revision: u64,
    /// Optional declared input uncertainty.
    pub uncertainty: Option<Uncertainty>,
    /// Inclusive design bound.
    pub lower: Option<Quantity>,
    /// Inclusive design bound.
    pub upper: Option<Quantity>,
}

impl Parameter {
    /// Normalize a value and enforce design bounds.
    pub fn check_value(&self, value: Quantity) -> ModelResult<Quantity> {
        let value = value.to(self.unit)?;
        if let Some(lower) = self.lower
            && value.value < lower.to(self.unit)?.value
        {
            return Err(ModelError::invalid(
                "parameter",
                "value is below its lower bound",
            ));
        }
        if let Some(upper) = self.upper
            && value.value > upper.to(self.unit)?.value
        {
            return Err(ModelError::invalid(
                "parameter",
                "value is above its upper bound",
            ));
        }
        Ok(value)
    }

    /// Validate metadata, expressions, units, bounds, and uncertainty.
    pub fn validate(&self, path: &str) -> ModelResult<()> {
        if self.owner.trim().is_empty()
            || self.source.reference.trim().is_empty()
            || self.source.kind.trim().is_empty()
        {
            return Err(ModelError::invalid(path, "owner and source are required"));
        }
        if let (Some(low), Some(high)) = (self.lower, self.upper)
            && low.to(self.unit)?.value > high.to(self.unit)?.value
        {
            return Err(ModelError::invalid(path, "bounds are reversed"));
        }
        for value in [self.lower, self.upper].into_iter().flatten() {
            value.to(self.unit)?;
        }
        if let Some(value) = self.value {
            self.check_value(value)?;
        }
        if let Some(expression) = &self.expression {
            expression.dependencies()?;
        }
        if let Some(uncertainty) = &self.uncertainty {
            if uncertainty.standard_deviation.to(self.unit)?.value < 0.0 {
                return Err(ModelError::invalid(
                    path,
                    "standard deviation cannot be negative",
                ));
            }
            if !matches!(uncertainty.distribution.as_str(), "normal" | "uniform") {
                return Err(ModelError::invalid(
                    path,
                    "uncertainty distribution must be normal or uniform",
                ));
            }
        }
        Ok(())
    }
}

/// A semantic field bound to a canonical parameter or explicit constant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Binding {
    /// Canonical parameter path.
    Parameter(String),
    /// Explicit quantity.
    Constant(Quantity),
}

impl Binding {
    /// Resolve a bound quantity with unit checking.
    pub fn resolve(
        &self,
        values: &BTreeMap<String, Quantity>,
        unit: Unit,
    ) -> ModelResult<Quantity> {
        match self {
            Self::Parameter(name) => values
                .get(name)
                .ok_or_else(|| ModelError::UnknownReference(name.clone()))?
                .to(unit),
            Self::Constant(value) => value.to(unit),
        }
    }

    /// Return a graph dependency, if this is not a constant.
    pub fn parameter(&self) -> Option<&str> {
        match self {
            Self::Parameter(name) => Some(name),
            Self::Constant(_) => None,
        }
    }
}

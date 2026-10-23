use super::ContractError;
use crate::{Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Where an applicability coordinate is read at execution time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(
    tag = "kind",
    content = "name",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum DomainVariable {
    /// A declared scalar input port.
    Input(String),
    /// A condition in the actual mission state, including applied stage controls.
    Condition(String),
}

/// One finite endpoint in the enclosing domain bound's unit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DomainLimit {
    /// Endpoint magnitude.
    pub value: f64,
    /// Whether equality is accepted.
    pub inclusive: bool,
}

/// Executable interval restriction, independent of a particular solver implementation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DomainBound {
    /// Input or state coordinate to inspect.
    pub variable: DomainVariable,
    /// Physical unit of both endpoints.
    pub unit: Unit,
    /// Lower endpoint; absent means no lower restriction.
    pub lower: Option<DomainLimit>,
    /// Upper endpoint; absent means no upper restriction.
    pub upper: Option<DomainLimit>,
    /// Whether an absent coordinate is a contract failure.
    pub required: bool,
}

impl DomainBound {
    /// Construct a required closed interval.
    pub fn closed(variable: DomainVariable, unit: Unit, lower: f64, upper: f64) -> Self {
        Self {
            variable,
            unit,
            lower: Some(DomainLimit {
                value: lower,
                inclusive: true,
            }),
            upper: Some(DomainLimit {
                value: upper,
                inclusive: true,
            }),
            required: true,
        }
    }

    /// Construct a required strictly positive coordinate.
    pub fn positive(variable: DomainVariable, unit: Unit) -> Self {
        Self {
            variable,
            unit,
            lower: Some(DomainLimit {
                value: 0.0,
                inclusive: false,
            }),
            upper: None,
            required: true,
        }
    }

    fn validate(&self) -> Result<(), ContractError> {
        let name = match &self.variable {
            DomainVariable::Input(name) | DomainVariable::Condition(name) => name,
        };
        let invalid = |reason| ContractError::invalid(name, reason);
        if name.trim().is_empty() || self.lower.is_none() && self.upper.is_none() {
            return Err(invalid(
                "domain coordinate needs a name and at least one bound",
            ));
        }
        for limit in [self.lower, self.upper].into_iter().flatten() {
            Quantity::new(limit.value, self.unit)?;
        }
        if let (Some(lower), Some(upper)) = (self.lower, self.upper)
            && (lower.value > upper.value
                || lower.value == upper.value && !(lower.inclusive && upper.inclusive))
        {
            return Err(invalid("domain interval is empty or reversed"));
        }
        Ok(())
    }

    fn check(&self, value: Option<Quantity>) -> Result<(), ContractError> {
        let Some(value) = value else {
            return if self.required {
                Err(ContractError::Missing {
                    variable: self.variable.clone(),
                })
            } else {
                Ok(())
            };
        };
        let actual = value.to(self.unit)?;
        let below = self.lower.is_some_and(|limit| {
            actual.value < limit.value || actual.value == limit.value && !limit.inclusive
        });
        let above = self.upper.is_some_and(|limit| {
            actual.value > limit.value || actual.value == limit.value && !limit.inclusive
        });
        if below || above {
            return Err(ContractError::Outside {
                variable: self.variable.clone(),
                actual,
                lower: self.lower,
                upper: self.upper,
            });
        }
        Ok(())
    }
}

/// Declared executable limits and physical assumptions; neither is a validation claim.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ValidityDomain {
    /// Intersected bounds, with at most one interval per coordinate.
    pub bounds: Vec<DomainBound>,
    /// Assumptions that require domain-specific checks or independent validation.
    pub assumptions: Vec<String>,
}

impl ValidityDomain {
    /// Validate finite bounds and unambiguous coordinate definitions.
    pub fn validate(&self) -> Result<(), ContractError> {
        let mut seen = BTreeSet::new();
        for bound in &self.bounds {
            bound.validate()?;
            if !seen.insert(&bound.variable) {
                return Err(ContractError::invalid("domain", "duplicate coordinate"));
            }
        }
        if self.assumptions.iter().any(|text| text.trim().is_empty()) {
            return Err(ContractError::invalid(
                "domain",
                "empty physical assumption",
            ));
        }
        Ok(())
    }

    /// Check actual values without clamping, extrapolation or substituting defaults.
    /// Phase callers check the entry state and each reported trajectory condition.
    pub fn check(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        conditions: Option<&BTreeMap<String, Quantity>>,
    ) -> Result<(), ContractError> {
        self.validate()?;
        for bound in &self.bounds {
            let value = match &bound.variable {
                DomainVariable::Input(name) => inputs.get(name),
                DomainVariable::Condition(name) => conditions.and_then(|values| values.get(name)),
            };
            bound.check(value.copied())?;
        }
        Ok(())
    }
}

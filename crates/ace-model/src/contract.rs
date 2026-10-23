use crate::{ModelError, Quantity, Unit};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod domain;
pub use domain::{DomainBound, DomainLimit, DomainVariable, ValidityDomain};

/// Versioned meaning of a scalar beyond its physical dimensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PortMeaning {
    /// Application-owned semantic schema, including its version.
    pub schema: String,
    /// Shared reference convention, such as projected wing area or a moment origin.
    pub reference: String,
}

/// Scalar solver port with explicit dimensions and optional semantic refinement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScalarPort {
    /// Unit accepted or produced by the implementation.
    pub unit: Unit,
    /// Absent means dimensional compatibility only, not semantic qualification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaning: Option<PortMeaning>,
}

impl From<Unit> for ScalarPort {
    fn from(unit: Unit) -> Self {
        Self {
            unit,
            meaning: None,
        }
    }
}

impl ScalarPort {
    /// Declare an application-owned semantic contract with an explicit reference convention.
    pub fn semantic(unit: Unit, schema: impl Into<String>, reference: impl Into<String>) -> Self {
        Self {
            unit,
            meaning: Some(PortMeaning {
                schema: schema.into(),
                reference: reference.into(),
            }),
        }
    }

    /// Reject empty semantic identities instead of treating them as unspecified.
    pub fn validate(&self, path: &str) -> Result<(), ContractError> {
        if self.meaning.as_ref().is_some_and(|meaning| {
            meaning.schema.trim().is_empty() || meaning.reference.trim().is_empty()
        }) {
            return Err(ContractError::invalid(
                path,
                "semantic schema and reference must be nonempty",
            ));
        }
        Ok(())
    }

    /// Check a directly connected producer; a specified meaning requires an exact match.
    pub fn accepts(&self, producer: &Self) -> bool {
        self.unit.compatible(producer.unit) && self.meaning == producer.meaning
    }
}

/// Invalid contract, incompatible composition, or an out-of-domain evaluation.
#[derive(Debug, Error)]
pub enum ContractError {
    /// Invalid declaration or incompatible graph connection.
    #[error("invalid solver contract at {path}: {reason}")]
    Invalid {
        /// Solver, port, or domain declaration.
        path: String,
        /// Failed invariant.
        reason: String,
    },
    /// Required domain coordinate is missing.
    #[error("missing domain coordinate {variable:?}")]
    Missing {
        /// Input or state condition that was required.
        variable: DomainVariable,
    },
    /// A finite physical value falls outside the declared interval.
    #[error("outside solver domain at {variable:?}: {actual:?}, lower {lower:?}, upper {upper:?}")]
    Outside {
        /// Checked input or state condition.
        variable: DomainVariable,
        /// Actual value converted to the domain unit.
        actual: Quantity,
        /// Lower bound in the same unit.
        lower: Option<DomainLimit>,
        /// Upper bound in the same unit.
        upper: Option<DomainLimit>,
    },
    /// Non-finite value or incompatible dimensions.
    #[error(transparent)]
    Quantity(#[from] ModelError),
}

impl ContractError {
    /// Construct a contextual contract error.
    pub fn invalid(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Invalid {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests;

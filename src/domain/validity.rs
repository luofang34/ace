use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::{AexError, AexResult};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ValidityVariable {
    Altitude,
    Mach,
    TrueAirspeed,
    Mass,
    Throttle,
    AspectRatio,
    LoadFactor,
}

impl ValidityVariable {
    pub(crate) fn unit(self) -> &'static str {
        match self {
            Self::Altitude => "m",
            Self::Mach | Self::Throttle | Self::AspectRatio | Self::LoadFactor => "1",
            Self::TrueAirspeed => "m/s",
            Self::Mass => "kg",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ValidityBasis {
    PublishedSpecification,
    ModelForm,
    ResolvedProfile,
    ScreeningAssumption,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct ValidityBound {
    pub(crate) variable: ValidityVariable,
    pub(crate) minimum: Option<f64>,
    pub(crate) maximum: Option<f64>,
    pub(crate) minimum_inclusive: bool,
    pub(crate) maximum_inclusive: bool,
    pub(crate) unit: String,
    pub(crate) basis: ValidityBasis,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct ModelValidityDomain {
    pub(crate) model_id: String,
    pub(crate) bounds: Vec<ValidityBound>,
}

impl ModelValidityDomain {
    pub(crate) fn validate(&self) -> AexResult<()> {
        if self.model_id.trim().is_empty() || self.bounds.is_empty() {
            return Err(AexError::validation(
                "INVALID_MODEL_VALIDITY_DOMAIN",
                "model_validity_domain",
                "model ID and at least one variable bound are required",
            ));
        }
        let mut variables = BTreeSet::new();
        for bound in &self.bounds {
            validate_bound(bound)?;
            if !variables.insert(bound.variable) {
                return Err(AexError::validation(
                    "DUPLICATE_MODEL_VALIDITY_VARIABLE",
                    &self.model_id,
                    format!("duplicate {:?} bound", bound.variable),
                ));
            }
        }
        Ok(())
    }
}

pub(crate) trait ValidityDomainProvider {
    fn validity_domain(&self) -> ModelValidityDomain;
}

fn validate_bound(bound: &ValidityBound) -> AexResult<()> {
    if bound.unit != bound.variable.unit()
        || bound.minimum.is_none() && bound.maximum.is_none()
        || bound.minimum.is_some_and(|value| !value.is_finite())
        || bound.maximum.is_some_and(|value| !value.is_finite())
        || matches!((bound.minimum, bound.maximum), (Some(min), Some(max)) if min > max)
    {
        return Err(AexError::validation(
            "INVALID_MODEL_VALIDITY_BOUND",
            format!("model_validity_domain.{:?}", bound.variable),
            "bounds must use the canonical unit and finite ordered limits",
        ));
    }
    Ok(())
}

use crate::{StudyError, StudyResult};
use ace_model::Quantity;
use serde::{Deserialize, Serialize};

/// Samples along one grid axis, listed explicitly or generated between two endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Axis {
    /// Explicit, unit-bearing samples in evaluation order.
    Samples(Vec<Quantity>),
    /// Evenly spaced samples that include both endpoints.
    Range(AxisRange),
}

/// Endpoint-inclusive sample generator; the samples take the start unit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AxisRange {
    /// First sample.
    pub start: Quantity,
    /// Last sample, in any unit of the same dimension.
    pub stop: Quantity,
    /// Number of samples, at least two.
    pub count: u32,
    /// Linear or geometric progression between the endpoints.
    #[serde(default)]
    pub spacing: Spacing,
}

/// Progression between range endpoints.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Spacing {
    /// Equal differences between successive samples.
    #[default]
    Linear,
    /// Equal ratios between successive samples; both endpoints must be positive.
    Logarithmic,
}

impl From<Vec<Quantity>> for Axis {
    fn from(samples: Vec<Quantity>) -> Self {
        Self::Samples(samples)
    }
}

impl Axis {
    /// Resolve the axis into its ordered samples.
    pub fn samples(&self, name: &str) -> StudyResult<Vec<Quantity>> {
        match self {
            Self::Samples(samples) => Ok(samples.clone()),
            Self::Range(range) => range.samples(name),
        }
    }
}

impl AxisRange {
    fn samples(&self, name: &str) -> StudyResult<Vec<Quantity>> {
        if self.count < 2 {
            return Err(StudyError::invalid(
                name,
                "range count must be at least two",
            ));
        }
        let start = self.start.value;
        let stop = self.stop.to(self.start.unit)?.value;
        if self.spacing == Spacing::Logarithmic && (start <= 0.0 || stop <= 0.0) {
            return Err(StudyError::invalid(
                name,
                "logarithmic range endpoints must be positive",
            ));
        }
        let last = f64::from(self.count - 1);
        (0..self.count)
            .map(|index| {
                let fraction = f64::from(index) / last;
                let value = match self.spacing {
                    Spacing::Linear => start + fraction * (stop - start),
                    Spacing::Logarithmic => start * (stop / start).powf(fraction),
                };
                Quantity::new(value, self.start.unit).map_err(StudyError::from)
            })
            .collect()
    }
}

use ace_model::{Priority, Unit};
use serde::{Deserialize, Serialize};

/// Two-sided 95% normal quantile used for every reported interval.
const Z95: f64 = 1.959_963_984_540_054;

/// Sample statistics of one canonical metric over the samples that produced it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricStatistics {
    /// Canonical parameter unit.
    pub unit: Unit,
    /// Samples with a usable value.
    pub available: u32,
    /// Samples without a usable value; never imputed.
    pub unavailable: u32,
    /// Arithmetic mean.
    pub mean: f64,
    /// Unbiased sample standard deviation; zero for a single sample.
    pub standard_deviation: f64,
    /// Smallest observation.
    pub minimum: f64,
    /// Nearest-rank 5th percentile.
    pub p05: f64,
    /// Nearest-rank median.
    pub median: f64,
    /// Nearest-rank 95th percentile.
    pub p95: f64,
    /// Largest observation.
    pub maximum: f64,
}

impl MetricStatistics {
    pub(crate) fn from_values(unit: Unit, mut values: Vec<f64>, unavailable: u32) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        values.sort_by(f64::total_cmp);
        let count = values.len() as f64;
        let mean = values.iter().sum::<f64>() / count;
        let variance = if values.len() > 1 {
            values
                .iter()
                .map(|value| (value - mean).powi(2))
                .sum::<f64>()
                / (count - 1.0)
        } else {
            0.0
        };
        let rank = |fraction: f64| {
            let index = ((fraction * count).ceil() as usize).clamp(1, values.len()) - 1;
            values.get(index).copied().unwrap_or(mean)
        };
        Some(Self {
            unit,
            available: values.len() as u32,
            unavailable,
            mean,
            standard_deviation: variance.sqrt(),
            minimum: values.first().copied().unwrap_or(mean),
            p05: rank(0.05),
            median: rank(0.5),
            p95: rank(0.95),
            maximum: values.last().copied().unwrap_or(mean),
        })
    }
}

/// Finite-sample verdict against a requirement's declared confidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbabilityStatus {
    /// The 95% interval lies at or above the declared confidence.
    Pass,
    /// The 95% interval lies below the declared confidence.
    Fail,
    /// The interval straddles the confidence; more samples are needed.
    Indeterminate,
    /// The requirement declares no confidence; only the estimate is reported.
    NotRequested,
}

/// Estimated probability that a requirement holds under the declared uncertainty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequirementProbability {
    /// Enforcement policy copied from the requirement.
    pub priority: Priority,
    /// Declared minimum probability, when any.
    pub confidence: Option<f64>,
    /// Samples whose margin was nonnegative.
    pub satisfied: u32,
    /// All samples, including those whose metric was unavailable.
    pub samples: u32,
    /// Samples counted as unsatisfied because evidence was missing or inapplicable.
    pub unavailable: u32,
    /// Point estimate `satisfied / samples`.
    pub probability: f64,
    /// Wilson 95% lower bound.
    pub lower: f64,
    /// Wilson 95% upper bound.
    pub upper: f64,
    /// Comparison with the declared confidence.
    pub status: ProbabilityStatus,
}

impl RequirementProbability {
    pub(crate) fn new(
        priority: Priority,
        confidence: Option<f64>,
        satisfied: u32,
        samples: u32,
        unavailable: u32,
    ) -> Self {
        let (lower, upper) = wilson(satisfied, samples);
        let status = match confidence {
            None => ProbabilityStatus::NotRequested,
            Some(target) if lower >= target => ProbabilityStatus::Pass,
            Some(target) if upper < target => ProbabilityStatus::Fail,
            Some(_) => ProbabilityStatus::Indeterminate,
        };
        Self {
            priority,
            confidence,
            satisfied,
            samples,
            unavailable,
            probability: f64::from(satisfied) / f64::from(samples.max(1)),
            lower,
            upper,
            status,
        }
    }
}

/// Wilson score interval; well behaved at zero and full success counts.
pub(crate) fn wilson(successes: u32, trials: u32) -> (f64, f64) {
    if trials == 0 {
        return (0.0, 1.0);
    }
    let n = f64::from(trials);
    let p = f64::from(successes) / n;
    let z2 = Z95 * Z95;
    let centre = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let half = Z95 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / (1.0 + z2 / n);
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

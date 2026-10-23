use crate::{StudyError, StudyResult};
use ace_model::{Quantity, VehicleModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Identity of the generator and transforms; changing either changes every record.
pub(crate) const ALGORITHM: &str = "splitmix64/1; normal: Box-Muller cosine branch; uniform: mean +/- sqrt(3) sigma; truncation: redraw within declared parameter bounds, at most 64 draws";

const MAX_DRAWS: u32 = 64;

/// Version-stable 64-bit generator, independent of any external crate's defaults.
pub(crate) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1) with 53 random mantissa bits.
    pub(crate) fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    fn standard_normal(&mut self) -> f64 {
        let radius = (-2.0 * (1.0 - self.unit()).ln()).sqrt();
        radius * (std::f64::consts::TAU * self.unit()).cos()
    }
}

/// Declared input distribution resolved in the parameter's canonical unit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputDistribution {
    /// `normal` or `uniform`.
    pub distribution: String,
    /// Declared value, used as the distribution mean.
    pub mean: Quantity,
    /// Declared standard deviation.
    pub standard_deviation: Quantity,
    /// Inclusive truncation bound from the parameter declaration.
    pub lower: Option<Quantity>,
    /// Inclusive truncation bound from the parameter declaration.
    pub upper: Option<Quantity>,
}

impl InputDistribution {
    pub(crate) fn draw(&self, name: &str, random: &mut SplitMix64) -> StudyResult<Quantity> {
        let lower = self.lower.map_or(f64::NEG_INFINITY, |bound| bound.value);
        let upper = self.upper.map_or(f64::INFINITY, |bound| bound.value);
        for _ in 0..MAX_DRAWS {
            let deviation = match self.distribution.as_str() {
                "uniform" => (2.0 * random.unit() - 1.0) * 3.0_f64.sqrt(),
                _ => random.standard_normal(),
            };
            let value = self.mean.value + deviation * self.standard_deviation.value;
            if (lower..=upper).contains(&value) {
                return Ok(Quantity::new(value, self.mean.unit)?);
            }
        }
        Err(StudyError::invalid(
            name,
            "declared bounds reject nearly every draw; narrow the uncertainty or widen the bounds",
        ))
    }
}

/// Uncertain inputs with nonzero spread; computed outputs cannot be perturbed.
pub(crate) fn inputs(model: &VehicleModel) -> StudyResult<BTreeMap<String, InputDistribution>> {
    let outputs: std::collections::BTreeSet<_> = model
        .analyses
        .values()
        .flat_map(|analysis| analysis.outputs.values())
        .collect();
    let mut selected = BTreeMap::new();
    for (name, parameter) in &model.parameters {
        let Some(uncertainty) = &parameter.uncertainty else {
            continue;
        };
        let (Some(mean), None, false) = (
            parameter.value,
            &parameter.expression,
            outputs.contains(name),
        ) else {
            return Err(StudyError::invalid(
                name,
                "uncertainty applies only to independent input values",
            ));
        };
        let standard_deviation = uncertainty.standard_deviation.to(parameter.unit)?;
        if standard_deviation.value == 0.0 {
            continue;
        }
        selected.insert(
            name.clone(),
            InputDistribution {
                distribution: uncertainty.distribution.clone(),
                mean: mean.to(parameter.unit)?,
                standard_deviation,
                lower: parameter.lower.map(|q| q.to(parameter.unit)).transpose()?,
                upper: parameter.upper.map(|q| q.to(parameter.unit)).transpose()?,
            },
        );
    }
    if selected.is_empty() {
        return Err(StudyError::invalid(
            "uncertainty",
            "no input declares a nonzero uncertainty",
        ));
    }
    Ok(selected)
}

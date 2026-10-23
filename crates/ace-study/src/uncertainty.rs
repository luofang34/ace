use crate::{Feasibility, StudyError, StudyResult};
use ace_engine::{MarginStatus, Session, evaluate_requirements};
use ace_model::{Priority, Quantity, Source, VehicleModel};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod sampling;
mod statistics;

pub use sampling::InputDistribution;
pub use statistics::{MetricStatistics, ProbabilityStatus, RequirementProbability};

/// Largest sample count accepted by one study; each sample is a full graph evaluation.
const MAX_SAMPLES: u32 = 100_000;

/// Seeded propagation of declared input uncertainty through the synthesis graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UncertaintyPlan {
    /// Plan schema identifier; must be 1.
    pub schema_version: u32,
    /// Number of joint input samples, from 1 through 100000.
    pub samples: u32,
    /// Generator seed recorded with the evidence.
    pub seed: u64,
    /// Additional canonical metrics to summarize beside requirement and objective metrics.
    #[serde(default)]
    pub metrics: Vec<String>,
}

/// One joint draw and the canonical values it produced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UncertaintySample {
    /// Sampled input values in canonical units.
    pub inputs: BTreeMap<String, Quantity>,
    /// Summarized metrics; absent when the sample did not produce a usable value.
    pub values: BTreeMap<String, Option<Quantity>>,
    /// Requirement satisfaction; absent when evidence was missing or inapplicable.
    pub satisfied: BTreeMap<String, Option<bool>>,
    /// Engine failure for this draw, when the graph could not run at all.
    pub failure: Option<String>,
}

/// Content-addressed probabilistic evidence; replaying the plan reproduces its identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyRecord {
    /// SHA-256 of this record with this field empty.
    pub id: String,
    /// Generator, transform and truncation identity.
    pub algorithm: String,
    /// Exact build that executed every sample.
    pub build: ace_engine::BuildIdentity,
    /// Authoritative baseline; sampling never edits the caller's model.
    pub baseline: VehicleModel,
    /// Plan actually executed.
    pub plan: UncertaintyPlan,
    /// Resolved input distributions.
    pub inputs: BTreeMap<String, InputDistribution>,
    /// Draws in generation order.
    pub samples: Vec<UncertaintySample>,
    /// Statistics per summarized metric.
    pub metrics: BTreeMap<String, MetricStatistics>,
    /// Satisfaction probability per requirement.
    pub requirements: BTreeMap<String, RequirementProbability>,
    /// Mandatory feasibility under uncertainty. Sampled evidence is never better than
    /// provisional, and a hard requirement without a declared confidence leaves it unknown.
    pub feasibility: Feasibility,
}

impl UncertaintyPlan {
    /// Check the schema, sample budget and requested metrics against the model.
    pub fn validate(&self, model: &VehicleModel) -> StudyResult<()> {
        if self.schema_version != 1 || self.samples == 0 || self.samples > MAX_SAMPLES {
            return Err(StudyError::invalid(
                "uncertainty",
                "requires schema 1 and between 1 and 100000 samples",
            ));
        }
        for metric in &self.metrics {
            model.parameter(metric)?;
        }
        Ok(())
    }

    /// Sample every uncertain input jointly and evaluate the full graph per draw.
    pub fn run(&self, session: &Session) -> StudyResult<UncertaintyRecord> {
        let model = session.model();
        self.validate(model)?;
        let inputs = sampling::inputs(model)?;
        let metrics = summarized(model, &self.metrics);
        let mut baseline = session.fork();
        baseline.run()?;
        let mut random = sampling::SplitMix64::new(self.seed);
        let mut samples = Vec::with_capacity(self.samples as usize);
        for _ in 0..self.samples {
            let draw = inputs
                .iter()
                .map(|(name, input)| Ok((name.clone(), input.draw(name, &mut random)?)))
                .collect::<StudyResult<BTreeMap<_, _>>>()?;
            samples.push(evaluate(&baseline, draw, &metrics)?);
        }
        let mut record = UncertaintyRecord {
            id: String::new(),
            algorithm: sampling::ALGORITHM.into(),
            build: ace_engine::implementation_identity(),
            baseline: model.clone(),
            plan: self.clone(),
            inputs,
            metrics: statistics(model, &metrics, &samples)?,
            requirements: probabilities(model, &samples),
            feasibility: Feasibility::Unknown,
            samples,
        };
        record.feasibility = feasibility(&record.requirements);
        record.id = crate::record::digest(&record)?;
        Ok(record)
    }
}

impl UncertaintyRecord {
    /// Detect changed inputs, samples or conclusions after serialization.
    pub fn verify_integrity(&self) -> StudyResult<()> {
        let mut copy = self.clone();
        copy.id.clear();
        if crate::record::digest(&copy)? != self.id {
            return Err(StudyError::invalid(
                "uncertainty",
                "record digest does not match its data",
            ));
        }
        Ok(())
    }
}

fn summarized(model: &VehicleModel, extra: &[String]) -> BTreeSet<String> {
    let mut metrics: BTreeSet<_> = extra.iter().cloned().collect();
    metrics.extend(model.requirements.values().map(|item| item.metric.clone()));
    metrics.extend(model.objectives.values().map(|item| item.metric.clone()));
    metrics
}

fn evaluate(
    baseline: &Session,
    inputs: BTreeMap<String, Quantity>,
    metrics: &BTreeSet<String>,
) -> StudyResult<UncertaintySample> {
    let mut session = baseline.fork();
    let source = Source {
        kind: "uncertainty_sample".into(),
        reference: crate::record::digest(&inputs)?,
        revision: None,
    };
    let mut sample = UncertaintySample {
        inputs,
        values: metrics.iter().map(|name| (name.clone(), None)).collect(),
        satisfied: BTreeMap::new(),
        failure: None,
    };
    let outcome = session
        .edit(&sample.inputs, source)
        .and_then(|_| session.run());
    let snapshot = match outcome {
        Ok(snapshot) => snapshot,
        Err(error) => {
            sample.failure = Some(error.to_string());
            return Ok(sample);
        }
    };
    for (name, value) in &mut sample.values {
        *value = snapshot
            .values
            .get(name)
            .filter(|record| record.usable())
            .and_then(|record| record.quantity);
    }
    for (id, margin) in evaluate_requirements(session.model(), &snapshot)? {
        let satisfied = match margin.status {
            MarginStatus::Pass | MarginStatus::Warn => Some(true),
            MarginStatus::Fail => Some(false),
            MarginStatus::Unknown | MarginStatus::NotApplicable => None,
        };
        sample.satisfied.insert(id, satisfied);
    }
    Ok(sample)
}

fn statistics(
    model: &VehicleModel,
    metrics: &BTreeSet<String>,
    samples: &[UncertaintySample],
) -> StudyResult<BTreeMap<String, MetricStatistics>> {
    let mut result = BTreeMap::new();
    for name in metrics {
        let unit = model.parameter(name)?.unit;
        let mut values = Vec::new();
        for sample in samples {
            if let Some(value) = sample.values.get(name).copied().flatten() {
                values.push(value.to(unit)?.value);
            }
        }
        let missing = samples.len().saturating_sub(values.len()) as u32;
        if let Some(stats) = MetricStatistics::from_values(unit, values, missing) {
            result.insert(name.clone(), stats);
        }
    }
    Ok(result)
}

fn probabilities(
    model: &VehicleModel,
    samples: &[UncertaintySample],
) -> BTreeMap<String, RequirementProbability> {
    let total = samples.len() as u32;
    model
        .requirements
        .iter()
        .map(|(id, requirement)| {
            let outcomes = samples
                .iter()
                .map(|sample| sample.satisfied.get(id).copied().flatten());
            let satisfied = outcomes
                .clone()
                .filter(|value| *value == Some(true))
                .count() as u32;
            let unavailable = outcomes.filter(Option::is_none).count() as u32;
            let probability = RequirementProbability::new(
                requirement.priority,
                requirement.confidence,
                satisfied,
                total,
                unavailable,
            );
            (id.clone(), probability)
        })
        .collect()
}

fn feasibility(requirements: &BTreeMap<String, RequirementProbability>) -> Feasibility {
    let hard: Vec<_> = requirements
        .values()
        .filter(|item| item.priority == Priority::Hard)
        .collect();
    if hard
        .iter()
        .any(|item| item.status == ProbabilityStatus::Fail)
    {
        Feasibility::Violated
    } else if hard
        .iter()
        .all(|item| item.status == ProbabilityStatus::Pass)
    {
        Feasibility::Provisional
    } else {
        Feasibility::Unknown
    }
}

#[cfg(test)]
mod tests;

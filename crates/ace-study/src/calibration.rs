use crate::{StudyError, StudyResult};
use ace_engine::Session;
use ace_model::{Quantity, Source, VehicleModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod fit;

/// Identity of the deterministic search; changing it changes every record.
const ALGORITHM: &str = "cyclic coordinate golden-section search/1; 40 interior evaluations per coordinate per sweep; never accepts a worse value; objective: sum of squared measurement-normalized residuals";

/// Independently measured quantity with its one-sigma measurement uncertainty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    /// Measured magnitude and units.
    pub value: Quantity,
    /// Positive one-sigma measurement uncertainty in compatible units.
    pub standard_deviation: Quantity,
}

/// Provenance of measured data; a calibration never globally qualifies a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeasurementSource {
    /// Test report, flight log or immutable dataset identity.
    pub citation: String,
    /// Exact configuration or serial number of the measured article.
    pub article: String,
    /// Conditions under which the measurement is meaningful.
    pub assumptions: Vec<String>,
}

/// Bounded independent input adjusted to explain the measurements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CalibratedParameter {
    /// Canonical independent input.
    pub parameter: String,
    /// Inclusive search bound inside the parameter's declared bounds.
    pub lower: Quantity,
    /// Inclusive search bound inside the parameter's declared bounds.
    pub upper: Quantity,
}

/// Measured observations of one configuration and the inputs allowed to explain them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CalibrationCase {
    /// Case schema; must be one.
    pub schema_version: u32,
    /// Stable case identifier.
    pub name: String,
    /// Where the observations come from.
    pub measurement: MeasurementSource,
    /// Input values reproducing the test conditions; not part of the calibrated result.
    #[serde(default)]
    pub conditions: BTreeMap<String, Quantity>,
    /// Canonical outputs and their measured values.
    pub observations: BTreeMap<String, Observation>,
    /// Inputs adjusted by the fit.
    pub parameters: Vec<CalibratedParameter>,
    /// Complete coordinate sweeps, from 1 through 64.
    pub sweeps: u32,
}

/// Predicted value and its disagreement with one observation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Residual {
    /// Model prediction in the observation unit; absent when the model produced no value.
    pub predicted: Option<Quantity>,
    /// Prediction minus measurement, divided by the measurement standard deviation.
    pub normalized: Option<f64>,
}

/// Model agreement with every observation at one set of calibrated input values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Agreement {
    /// Calibrated input values in canonical units.
    pub parameters: BTreeMap<String, Quantity>,
    /// Residual per observation.
    pub residuals: BTreeMap<String, Residual>,
    /// Sum of squared normalized residuals; absent when any prediction is missing.
    pub chi_square: Option<f64>,
}

/// Content-addressed comparison of prediction and measurement, before and after the fit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationRecord {
    /// SHA-256 of this record with this field empty.
    pub id: String,
    /// Search algorithm identity.
    pub algorithm: String,
    /// Exact build that executed every evaluation.
    pub build: ace_engine::BuildIdentity,
    /// Model whose predictions were tested; the caller's session is never edited.
    pub baseline: VehicleModel,
    /// Exact case and measurements.
    pub case: CalibrationCase,
    /// Agreement of the uncalibrated model.
    pub prior: Agreement,
    /// Agreement after the fit.
    pub posterior: Agreement,
    /// Observations minus calibrated parameters; nonpositive means not identifiable.
    pub degrees_of_freedom: i64,
    /// Every posterior residual lies within three measurement standard deviations.
    pub consistent: bool,
}

impl CalibrationCase {
    /// Fit the calibrated inputs and record agreement before and after.
    pub fn run(&self, baseline: &Session) -> StudyResult<CalibrationRecord> {
        self.validate(baseline.model())?;
        let mut context = baseline.fork();
        context.edit(&self.conditions, self.source("measurement_condition")?)?;
        let start = self
            .parameters
            .iter()
            .map(|item| {
                let parameter = baseline.model().parameter(&item.parameter)?;
                let value = parameter.value.ok_or_else(|| {
                    StudyError::invalid(&item.parameter, "calibrated input has no prior value")
                })?;
                Ok((item.parameter.clone(), value.to(parameter.unit)?))
            })
            .collect::<StudyResult<BTreeMap<_, _>>>()?;
        let prior = fit::agreement(&context, self, &start)?;
        let posterior = fit::search(&context, self, prior.clone())?;
        let consistent = posterior
            .residuals
            .values()
            .all(|residual| residual.normalized.is_some_and(|value| value.abs() <= 3.0));
        let mut record = CalibrationRecord {
            id: String::new(),
            algorithm: ALGORITHM.into(),
            build: ace_engine::implementation_identity(),
            baseline: baseline.model().clone(),
            case: self.clone(),
            prior,
            posterior,
            degrees_of_freedom: self.observations.len() as i64 - self.parameters.len() as i64,
            consistent,
        };
        record.id = crate::record::digest(&record)?;
        Ok(record)
    }

    fn source(&self, kind: &str) -> StudyResult<Source> {
        Ok(Source {
            kind: kind.into(),
            reference: format!("{}:{}", self.name, crate::record::digest(self)?),
            revision: None,
        })
    }

    fn validate(&self, model: &VehicleModel) -> StudyResult<()> {
        let text_missing = self.name.trim().is_empty()
            || self.measurement.citation.trim().is_empty()
            || self.measurement.article.trim().is_empty()
            || self.measurement.assumptions.is_empty()
            || self
                .measurement
                .assumptions
                .iter()
                .any(|text| text.trim().is_empty());
        if self.schema_version != 1
            || text_missing
            || self.observations.is_empty()
            || self.parameters.is_empty()
            || !(1..=64).contains(&self.sweeps)
        {
            return Err(StudyError::invalid(
                "calibration",
                "requires schema 1, measurement provenance, observations, parameters and 1-64 sweeps",
            ));
        }
        for (name, observation) in &self.observations {
            let unit = model.parameter(name)?.unit;
            observation.value.to(unit)?;
            if observation.standard_deviation.to(unit)?.value <= 0.0 {
                return Err(StudyError::invalid(
                    name,
                    "measurement standard deviation must be positive",
                ));
            }
        }
        let outputs: std::collections::BTreeSet<_> = model
            .analyses
            .values()
            .flat_map(|analysis| analysis.outputs.values())
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        for item in &self.parameters {
            let parameter = model.parameter(&item.parameter)?;
            let lower = parameter.check_value(item.lower)?.value;
            let upper = parameter.check_value(item.upper)?.value;
            if parameter.expression.is_some()
                || outputs.contains(&item.parameter)
                || self.conditions.contains_key(&item.parameter)
                || !seen.insert(&item.parameter)
                || lower >= upper
            {
                return Err(StudyError::invalid(
                    &item.parameter,
                    "calibrate each independent input once, outside the test conditions, over a nonempty interval",
                ));
            }
        }
        Ok(())
    }
}

impl CalibrationRecord {
    /// Calibrated values and their provenance, ready for `Session::edit`.
    pub fn updates(&self) -> (BTreeMap<String, Quantity>, Source) {
        (
            self.posterior.parameters.clone(),
            Source {
                kind: "calibrated".into(),
                reference: format!("calibration:{}", self.id),
                revision: None,
            },
        )
    }

    /// Detect changed measurements, predictions or conclusions after serialization.
    pub fn verify_integrity(&self) -> StudyResult<()> {
        let mut copy = self.clone();
        copy.id.clear();
        if crate::record::digest(&copy)? != self.id {
            return Err(StudyError::invalid(
                &self.case.name,
                "calibration record digest does not match its data",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

use super::{Agreement, CalibrationCase, Residual};
use crate::StudyResult;
use ace_engine::Session;
use ace_model::Quantity;
use std::collections::BTreeMap;

const GOLDEN: f64 = 0.618_033_988_749_894_9;
const EVALUATIONS: u32 = 40;

/// Evaluate the observations at one set of calibrated values.
pub(super) fn agreement(
    context: &Session,
    case: &CalibrationCase,
    parameters: &BTreeMap<String, Quantity>,
) -> StudyResult<Agreement> {
    let mut session = context.fork();
    let snapshot = session
        .edit(parameters, case.source("calibration_trial")?)
        .and_then(|_| session.run());
    let mut residuals = BTreeMap::new();
    let mut chi_square = Some(0.0);
    for (name, observation) in &case.observations {
        let unit = observation.value.unit;
        let predicted = match &snapshot {
            Ok(snapshot) => snapshot
                .values
                .get(name)
                .filter(|record| record.usable())
                .and_then(|record| record.quantity)
                .map(|value| value.to(unit))
                .transpose()?,
            Err(_) => None,
        };
        let sigma = observation.standard_deviation.to(unit)?.value;
        let normalized = predicted.map(|value| (value.value - observation.value.value) / sigma);
        chi_square = chi_square
            .zip(normalized)
            .map(|(total, value)| total + value * value);
        residuals.insert(
            name.clone(),
            Residual {
                predicted,
                normalized,
            },
        );
    }
    Ok(Agreement {
        parameters: parameters.clone(),
        residuals,
        chi_square,
    })
}

fn score(agreement: &Agreement) -> f64 {
    agreement.chi_square.unwrap_or(f64::INFINITY)
}

/// Cyclic coordinate golden-section search; the incumbent is replaced only by a better fit.
pub(super) fn search(
    context: &Session,
    case: &CalibrationCase,
    prior: Agreement,
) -> StudyResult<Agreement> {
    let mut best = prior;
    for _ in 0..case.sweeps {
        for item in &case.parameters {
            let unit = context.model().parameter(&item.parameter)?.unit;
            let (mut low, mut high) = (item.lower.to(unit)?.value, item.upper.to(unit)?.value);
            let trial = |value: f64| -> StudyResult<Agreement> {
                let mut parameters = best.parameters.clone();
                parameters.insert(item.parameter.clone(), Quantity::new(value, unit)?);
                agreement(context, case, &parameters)
            };
            let mut left = trial(high - GOLDEN * (high - low))?;
            let mut right = trial(low + GOLDEN * (high - low))?;
            for _ in 2..EVALUATIONS {
                if score(&left) <= score(&right) {
                    high = value(&right, &item.parameter);
                    right = left;
                    left = trial(high - GOLDEN * (high - low))?;
                } else {
                    low = value(&left, &item.parameter);
                    left = right;
                    right = trial(low + GOLDEN * (high - low))?;
                }
            }
            for candidate in [left, right] {
                if score(&candidate) < score(&best) {
                    best = candidate;
                }
            }
        }
    }
    Ok(best)
}

fn value(agreement: &Agreement, parameter: &str) -> f64 {
    agreement
        .parameters
        .get(parameter)
        .map_or(f64::NAN, |value| value.value)
}

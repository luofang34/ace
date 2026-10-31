//! Route-distance search for the point where the mission's reserve-protected fuel runs out.

use std::collections::BTreeMap;

use ace_engine::{Session, Snapshot};
use ace_model::{Quantity, Source, Unit};

use super::{PayloadRangeRoles, current, model_error};
use crate::domain::diagnostic::{AexError, AexResult};

/// Relative bracket width at which the range is reported.
const TOLERANCE: f64 = 1e-3;
/// Doublings or halvings allowed while bracketing, and refinement missions after that.
const BRACKET_STEPS: u32 = 16;
const REFINEMENTS: u32 = 40;

pub(super) struct Solved {
    pub range_m: f64,
    pub remaining_kg: f64,
    pub width_m: f64,
    pub flown: u32,
}

enum Outcome {
    /// Every path completed; fuel left on the most demanding one.
    Completed(f64),
    /// A path ran out of loaded fuel.
    Shortfall,
}

pub(super) fn source(corner: &str) -> Source {
    Source {
        kind: "derived".into(),
        reference: format!("payload-range {corner} loading"),
        revision: None,
    }
}

/// Longest completed route distance, bracketed from `start` and refined by secant steps.
pub(super) fn range(
    session: &mut Session,
    roles: &PayloadRangeRoles<'_>,
    start: f64,
) -> AexResult<Solved> {
    let mut flown = 0;
    let mut fly = |session: &mut Session, distance: f64| {
        flown += 1;
        trial(session, roles, distance)
    };
    let mut completed = Vec::<(f64, f64)>::new();
    let mut shortfall = None;
    let mut distance = start;
    for _ in 0..BRACKET_STEPS {
        match fly(session, distance)? {
            Outcome::Completed(remaining) => {
                completed.push((distance, remaining));
                if shortfall.is_some() {
                    break;
                }
                distance *= 2.0;
            }
            Outcome::Shortfall => {
                shortfall = Some(distance);
                if !completed.is_empty() {
                    break;
                }
                distance *= 0.5;
            }
        }
    }
    let (Some(mut high), Some(&(mut low, mut low_fuel))) = (
        shortfall,
        completed.iter().max_by(|a, b| a.0.total_cmp(&b.0)),
    ) else {
        return Err(AexError::analysis(
            "PAYLOAD_RANGE_UNBRACKETED",
            format!(
                "no completed and fuel-limited distances within {BRACKET_STEPS} steps of {start} m"
            ),
        ));
    };
    let mut previous = completed
        .iter()
        .filter(|point| point.0 < low)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .copied();
    for _ in 0..REFINEMENTS {
        if (high - low) <= TOLERANCE * high {
            break;
        }
        let guess = previous
            .filter(|(distance, fuel)| *fuel > low_fuel && *distance < low)
            .map(|(distance, fuel)| low + low_fuel * (low - distance) / (fuel - low_fuel))
            .unwrap_or(0.5 * (low + high));
        // Keep each step strictly inside the bracket so it always shrinks.
        let margin = 0.05 * (high - low);
        let next = guess.clamp(low + margin, high - margin);
        match fly(session, next)? {
            Outcome::Completed(remaining) => {
                previous = Some((low, low_fuel));
                (low, low_fuel) = (next, remaining);
            }
            Outcome::Shortfall => high = next,
        }
    }
    Ok(Solved {
        range_m: low,
        remaining_kg: low_fuel,
        width_m: high - low,
        flown,
    })
}

fn trial(
    session: &mut Session,
    roles: &PayloadRangeRoles<'_>,
    distance: f64,
) -> AexResult<Outcome> {
    session.edit(
        &BTreeMap::from([(
            roles.distance.to_owned(),
            Quantity::new(distance, Unit::Meter).map_err(model_error)?,
        )]),
        source("distance trial"),
    )?;
    let snapshot = session.run()?;
    if let Ok(remaining) = current(&snapshot, roles.remaining) {
        let remaining = remaining.to(Unit::Kilogram).map_err(model_error)?.value;
        return Ok(if remaining >= 0.0 {
            Outcome::Completed(remaining)
        } else {
            Outcome::Shortfall
        });
    }
    let record = snapshot.values.get(roles.remaining);
    let run = record
        .and_then(|record| record.run_id.as_ref())
        .and_then(|id| snapshot.runs.iter().find(|run| &run.id == id));
    if run.is_some_and(|run| exhausted(&run.details)) {
        return Ok(Outcome::Shortfall);
    }
    Err(AexError::analysis(
        "PAYLOAD_RANGE_MISSION_FAILED",
        format!(
            "the mission failed at {distance} m for a reason other than fuel: {}",
            failure(&snapshot, roles.remaining)
        ),
    ))
}

/// Whether the main path or any contingency branch stopped for lack of loaded fuel.
fn exhausted(report: &serde_json::Value) -> bool {
    report["fuel_exhausted"].as_bool() == Some(true)
        || report["branches"]
            .as_object()
            .is_some_and(|branches| branches.values().any(exhausted))
}

fn failure(snapshot: &Snapshot, name: &str) -> String {
    snapshot
        .values
        .get(name)
        .and_then(|record| record.message.clone())
        .unwrap_or_else(|| format!("{name} is unavailable"))
}

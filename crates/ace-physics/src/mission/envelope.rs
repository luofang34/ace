use super::{MissionPoint, MissionReport};
use crate::{PhysicsError, PhysicsResult};
use ace_model::{CgLimit, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Evaluated CG envelope station in vehicle coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgStation {
    /// Total vehicle mass, kilograms.
    pub mass_kg: f64,
    /// Forward CG boundary, meters.
    pub minimum_m: f64,
    /// Aft CG boundary, meters.
    pub maximum_m: f64,
}

/// Actual integration boundary checked against the declared CG envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgWitness {
    /// Nominal mission or independent branch identifier.
    pub scenario: String,
    /// Phase identifier, or initial loading.
    pub stage: String,
    /// Integration boundary within the stage.
    pub point: usize,
    /// Elapsed scenario time, seconds.
    pub time_s: f64,
    /// Actual total mass, kilograms.
    pub mass_kg: f64,
    /// Actual longitudinal CG, meters.
    pub cg_x_m: f64,
    /// Smaller forward/aft clearance; absent outside the declared mass domain.
    pub margin_m: Option<f64>,
}

/// CG checks of all nominal and contingency integration boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgReport {
    /// Mission whose state transitions were executed.
    pub mission: String,
    /// Declared bounds resolved from canonical values.
    pub envelope: Vec<CgStation>,
    /// Every sampled integration boundary, including initial and release states.
    pub points: Vec<CgWitness>,
    /// Limiting sampled state when all required states have valid bounds.
    pub limiting: Option<CgWitness>,
    /// Missing mission or envelope evidence; never classified as feasible.
    pub failure: Option<String>,
}

/// Check real mission states against a piecewise-linear envelope without extrapolation.
pub fn evaluate_cg_envelope(
    mission: &MissionReport,
    limits: &[CgLimit],
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<CgReport> {
    let envelope = resolve(limits, values)?;
    let mut points = Vec::new();
    collect(mission, &envelope, &mut points);
    let failure = mission.scenario_failure().or_else(|| {
        points
            .iter()
            .find(|point| point.margin_m.is_none())
            .map(|point| {
                format!(
                    "CG envelope undefined at {} / {} / point {} ({} kg)",
                    point.scenario, point.stage, point.point, point.mass_kg
                )
            })
    });
    let limiting = if failure.is_none() {
        points
            .iter()
            .min_by(|a, b| {
                a.margin_m
                    .unwrap_or(f64::INFINITY)
                    .total_cmp(&b.margin_m.unwrap_or(f64::INFINITY))
            })
            .cloned()
    } else {
        None
    };
    Ok(CgReport {
        mission: mission.mission.clone(),
        envelope,
        points,
        limiting,
        failure,
    })
}

fn resolve(
    limits: &[CgLimit],
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<Vec<CgStation>> {
    let stations = limits
        .iter()
        .map(|limit| {
            Ok(CgStation {
                mass_kg: limit.mass.resolve(values, Unit::Kilogram)?.value,
                minimum_m: limit.minimum.resolve(values, Unit::Meter)?.value,
                maximum_m: limit.maximum.resolve(values, Unit::Meter)?.value,
            })
        })
        .collect::<PhysicsResult<Vec<_>>>()?;
    if stations.len() < 2
        || stations
            .iter()
            .any(|s| s.mass_kg <= 0.0 || s.minimum_m > s.maximum_m)
        || stations
            .windows(2)
            .any(|pair| matches!(pair,[a,b] if a.mass_kg>=b.mass_kg))
    {
        return Err(PhysicsError::invalid(
            "CG envelope",
            "requires increasing positive masses and ordered CG limits",
        ));
    }
    Ok(stations)
}

fn collect(report: &MissionReport, envelope: &[CgStation], points: &mut Vec<CgWitness>) {
    points.push(witness(report, "initial", 0, &report.initial, envelope));
    for stage in &report.stages {
        for (index, point) in stage
            .trajectory
            .iter()
            .chain(std::iter::once(&stage.end))
            .enumerate()
        {
            points.push(witness(report, &stage.id, index, point, envelope));
        }
    }
    for branch in report.branches.values() {
        collect(branch, envelope, points);
    }
}

fn witness(
    report: &MissionReport,
    stage: &str,
    index: usize,
    point: &MissionPoint,
    envelope: &[CgStation],
) -> CgWitness {
    let mass = point.mass.total_mass_kg;
    let cg = point.mass.cg_m[0];
    let margin = envelope.windows(2).find_map(|pair| {
        let [a, b] = pair else {
            return None;
        };
        if mass < a.mass_kg || mass > b.mass_kg {
            return None;
        }
        let fraction = (mass - a.mass_kg) / (b.mass_kg - a.mass_kg);
        let minimum = a.minimum_m + fraction * (b.minimum_m - a.minimum_m);
        let maximum = a.maximum_m + fraction * (b.maximum_m - a.maximum_m);
        Some((cg - minimum).min(maximum - cg))
    });
    CgWitness {
        scenario: report.mission.clone(),
        stage: stage.into(),
        point: index,
        time_s: point.state.time.value,
        mass_kg: mass,
        cg_x_m: cg,
        margin_m: margin,
    }
}

#[cfg(test)]
mod tests;

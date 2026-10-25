use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ace_engine::{Feasibility, Margin, ValueRecord};
use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult};

/// Named outputs of several synthesized vehicles, each with its own provenance and verdict.
#[derive(Debug, Serialize)]
pub(crate) struct VehicleComparison {
    pub metrics: Vec<String>,
    pub vehicles: Vec<ComparedVehicle>,
}

/// One vehicle's selected values in its declared units, requirement margins and feasibility.
#[derive(Debug, Serialize)]
pub(crate) struct ComparedVehicle {
    pub path: PathBuf,
    pub vehicle_id: String,
    pub model_digest: String,
    pub completed: bool,
    pub feasibility: Feasibility,
    pub values: BTreeMap<String, ValueRecord>,
    pub margins: BTreeMap<String, Margin>,
}

/// Synthesize every vehicle and tabulate the requested parameters without unit conversion.
pub(crate) fn compare_blocking(
    paths: &[PathBuf],
    metrics: &[String],
) -> AexResult<VehicleComparison> {
    if paths.is_empty() {
        return Err(AexError::validation(
            "MISSING_COMPARISON_VEHICLE",
            "vehicles",
            "at least one vehicle is required",
        ));
    }
    if metrics.is_empty() || metrics.iter().collect::<BTreeSet<_>>().len() != metrics.len() {
        return Err(AexError::validation(
            "INVALID_COMPARISON_METRICS",
            "metrics",
            "provide one or more distinct parameter identifiers",
        ));
    }
    let vehicles = paths
        .iter()
        .map(|path| compare_one(path, metrics))
        .collect::<AexResult<_>>()?;
    Ok(VehicleComparison {
        metrics: metrics.to_vec(),
        vehicles,
    })
}

fn compare_one(path: &Path, metrics: &[String]) -> AexResult<ComparedVehicle> {
    let mut session = super::synthesis::open_blocking(path, &BTreeMap::new())?;
    for metric in metrics {
        if session.model().parameter(metric).is_err() {
            return Err(AexError::validation(
                "UNKNOWN_COMPARISON_METRIC",
                metric.clone(),
                format!("{} declares no such parameter", path.display()),
            ));
        }
    }
    let snapshot = session.run()?;
    let margins = ace_engine::evaluate_requirements(session.model(), &snapshot)?;
    let feasibility = ace_engine::assess(session.model(), &snapshot)?.feasibility;
    let values = metrics
        .iter()
        .filter_map(|metric| {
            snapshot
                .values
                .get(metric)
                .map(|record| (metric.clone(), record.clone()))
        })
        .collect();
    Ok(ComparedVehicle {
        path: path.to_path_buf(),
        vehicle_id: snapshot.vehicle_id.clone(),
        model_digest: snapshot.model_digest.clone(),
        completed: snapshot.complete(),
        feasibility,
        values,
        margins,
    })
}

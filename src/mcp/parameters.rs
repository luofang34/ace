use serde_json::Value;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::services::sweep::SweepVariable;

use super::schema::SweepVariableRequest;

pub(super) fn sweep_variable(request: SweepVariableRequest) -> AexResult<SweepVariable> {
    if let Some(values) = request.values {
        if values.is_empty() {
            return Err(AexError::validation(
                "EMPTY_SWEEP_VALUES",
                request.path,
                "explicit value list cannot be empty",
            ));
        }
        return Ok(SweepVariable {
            path: request.path,
            values,
        });
    }
    let start = request.start.ok_or_else(|| {
        AexError::validation("MISSING_SWEEP_START", &request.path, "start is required")
    })?;
    let stop = request.stop.ok_or_else(|| {
        AexError::validation("MISSING_SWEEP_STOP", &request.path, "stop is required")
    })?;
    SweepVariable::linear(
        request.path,
        &start,
        &stop,
        request.count.unwrap_or(25),
        request.logarithmic.unwrap_or(false),
    )
}

pub(super) fn parse_wing_loading(raw: &str) -> AexResult<f64> {
    crate::cli::commands::parse_wing_loading(raw)
}

pub(super) fn dotted_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(value, |current, part| current.get(part))
}

pub(super) fn infer_result_unit(path: &str) -> &'static str {
    if path.contains("distance") || path.contains("range") {
        "nmi display, m internal"
    } else if path.contains("ceiling") || path.ends_with("_m") {
        "m"
    } else if path.contains("speed") {
        "m/s"
    } else if path.contains("fuel") || path.contains("mass") {
        "kg"
    } else {
        "dimensionless or result-specific"
    }
}

pub(super) fn governing_equations(path: &str) -> Vec<&'static str> {
    if path.contains("ceiling") {
        vec![
            "ROC = excess power / weight",
            "bounded root: ROC - threshold = 0",
        ]
    } else if path.contains("range") || path.contains("distance") {
        vec![
            "distance = true airspeed × segment duration",
            "fuel flow from BSFC or TSFC",
        ]
    } else {
        vec!["CD = CD0 + k CL² + CDadditional + CDwave", "drag = q S CD"]
    }
}

pub(super) fn input_dependencies(path: &str) -> Vec<&'static str> {
    if path.contains("ceiling") {
        vec![
            "aircraft mass",
            "wing geometry",
            "drag polar",
            "propulsion lapse",
            "ISA atmosphere",
        ]
    } else {
        vec!["resolved aircraft", "mission", "selected profiles"]
    }
}

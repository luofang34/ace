use serde_json::Value;

use crate::domain::diagnostic::AexResult;

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

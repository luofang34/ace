use std::collections::BTreeMap;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::schema::{RawWing, Wing};

use super::{optional_quantity, positive, positive_quantity};

const AREA_PATH: &str = "aircraft.geometry.wing.area";
const SPAN_PATH: &str = "aircraft.geometry.wing.span";
const ASPECT_RATIO_PATH: &str = "aircraft.geometry.wing.aspect_ratio";
const RELATIVE_TOLERANCE: f64 = 0.005;

struct Planform {
    area_m2: f64,
    span_m: f64,
    aspect_ratio: f64,
}

pub(super) fn resolve_wing(raw: &RawWing) -> AexResult<Wing> {
    let planform = resolve_planform(
        raw.area
            .as_deref()
            .map(|value| positive_quantity(value, Dimension::Area, AREA_PATH))
            .transpose()?,
        raw.span
            .as_deref()
            .map(|value| positive_quantity(value, Dimension::Length, SPAN_PATH))
            .transpose()?,
        raw.aspect_ratio
            .map(|value| positive(value, ASPECT_RATIO_PATH))
            .transpose()?,
    )?;
    Ok(Wing {
        area_m2: planform.area_m2,
        span_m: planform.span_m,
        aspect_ratio: planform.aspect_ratio,
        sweep_quarter_chord_rad: parse_quantity(&raw.sweep_quarter_chord, Dimension::Angle)?,
        center_body_edge_sweep_rad: optional_quantity(
            raw.center_body_edge_sweep.as_deref(),
            Dimension::Angle,
        )?,
    })
}

pub(crate) fn complete_planform_overrides(
    overrides: &BTreeMap<String, String>,
    baseline: &Wing,
) -> AexResult<BTreeMap<String, String>> {
    let mut completed = overrides.clone();
    let area = override_quantity(overrides, AREA_PATH, Dimension::Area)?;
    let span = override_quantity(overrides, SPAN_PATH, Dimension::Length)?;
    let aspect_ratio = overrides
        .get(ASPECT_RATIO_PATH)
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|source| {
                    AexError::validation("INVALID_OVERRIDE", ASPECT_RATIO_PATH, source.to_string())
                })
                .and_then(|value| positive(value, ASPECT_RATIO_PATH))
        })
        .transpose()?;
    match (area, span, aspect_ratio) {
        (None, None, None) | (Some(_), Some(_), Some(_)) => {}
        (Some(area), Some(span), None) => insert_ratio(&mut completed, span.powi(2) / area),
        (Some(area), None, Some(ratio)) => insert_span(&mut completed, (area * ratio).sqrt()),
        (None, Some(span), Some(ratio)) => insert_area(&mut completed, span.powi(2) / ratio),
        (Some(area), None, None) => {
            insert_ratio(&mut completed, baseline.aspect_ratio);
            insert_span(&mut completed, (area * baseline.aspect_ratio).sqrt());
        }
        (None, Some(span), None) => {
            insert_area(&mut completed, baseline.area_m2);
            insert_ratio(&mut completed, span.powi(2) / baseline.area_m2);
        }
        (None, None, Some(ratio)) => {
            insert_area(&mut completed, baseline.area_m2);
            insert_span(&mut completed, (baseline.area_m2 * ratio).sqrt());
        }
    }
    Ok(completed)
}

fn resolve_planform(
    area_m2: Option<f64>,
    span_m: Option<f64>,
    aspect_ratio: Option<f64>,
) -> AexResult<Planform> {
    match (area_m2, span_m, aspect_ratio) {
        (Some(area_m2), Some(span_m), declared_ratio) => {
            let resolved_ratio = span_m.powi(2) / area_m2;
            if let Some(declared_ratio) = declared_ratio {
                validate_declared_ratio(declared_ratio, resolved_ratio)?;
            }
            Ok(Planform {
                area_m2,
                span_m,
                aspect_ratio: resolved_ratio,
            })
        }
        (Some(area_m2), None, Some(aspect_ratio)) => Ok(Planform {
            area_m2,
            span_m: (area_m2 * aspect_ratio).sqrt(),
            aspect_ratio,
        }),
        (None, Some(span_m), Some(aspect_ratio)) => Ok(Planform {
            area_m2: span_m.powi(2) / aspect_ratio,
            span_m,
            aspect_ratio,
        }),
        _ => Err(AexError::validation(
            "INCOMPLETE_WING_PLANFORM",
            "aircraft.geometry.wing",
            "at least two of area, span, and aspect_ratio are required",
        )),
    }
}

fn validate_declared_ratio(declared: f64, resolved: f64) -> AexResult<()> {
    let absolute_error = (declared - resolved).abs();
    let allowed_error = resolved * RELATIVE_TOLERANCE;
    let rounding_allowance = f64::EPSILON * declared.abs().max(resolved.abs()).max(1.0) * 4.0;
    if absolute_error <= allowed_error + rounding_allowance {
        Ok(())
    } else {
        let relative_error = absolute_error / resolved;
        Err(AexError::validation(
            "INCONSISTENT_WING_PLANFORM",
            "aircraft.geometry.wing.aspect_ratio",
            format!(
                "declared aspect ratio {declared} differs from span²/area {resolved} by {:.3}%",
                relative_error * 100.0
            ),
        ))
    }
}

fn override_quantity(
    overrides: &BTreeMap<String, String>,
    path: &str,
    dimension: Dimension,
) -> AexResult<Option<f64>> {
    overrides
        .get(path)
        .map(|value| parse_quantity(value, dimension).and_then(|value| positive(value, path)))
        .transpose()
}

fn insert_area(overrides: &mut BTreeMap<String, String>, value: f64) {
    overrides.insert(AREA_PATH.to_owned(), format!("{value:.12} m^2"));
}

fn insert_span(overrides: &mut BTreeMap<String, String>, value: f64) {
    overrides.insert(SPAN_PATH.to_owned(), format!("{value:.12} m"));
}

fn insert_ratio(overrides: &mut BTreeMap<String, String>, value: f64) {
    overrides.insert(ASPECT_RATIO_PATH.to_owned(), format!("{value:.12}"));
}

#[cfg(test)]
mod tests;

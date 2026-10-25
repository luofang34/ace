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
        aerodynamic_center_m: optional_quantity(
            raw.aerodynamic_center.as_deref(),
            Dimension::Length,
        )?,
    })
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

#[cfg(test)]
mod tests;

use crate::backends::contracts::{PolarPoint, StabilitySummary};
use crate::domain::diagnostic::{AexError, AexResult};

pub(super) fn positive_marker_number(output: &str, marker: &str) -> AexResult<f64> {
    let raw = output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(marker))
        .ok_or_else(|| parse_failure(format!("missing marker {marker}")))?;
    let value = raw
        .trim()
        .parse::<f64>()
        .map_err(|source| parse_failure(format!("invalid {marker} value: {source}")))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(parse_failure(format!(
            "{marker} must be finite and positive, got {value}"
        )));
    }
    Ok(value)
}

pub(super) fn polar_points(output: &str) -> AexResult<Vec<PolarPoint>> {
    let points = output
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("ACE_POLAR="))
        .map(parse_polar_point)
        .collect::<AexResult<Vec<_>>>()?;
    if points.is_empty() {
        return Err(parse_failure("VSPAERO returned no polar points"));
    }
    Ok(points)
}

fn parse_polar_point(raw: &str) -> AexResult<PolarPoint> {
    let values = raw
        .split(',')
        .map(|value| {
            value
                .trim()
                .parse::<f64>()
                .map_err(|source| parse_failure(format!("invalid polar value: {source}")))
        })
        .collect::<AexResult<Vec<_>>>()?;
    if values.len() != 4 {
        return Err(parse_failure(format!(
            "expected four polar values, got {}",
            values.len()
        )));
    }
    Ok(PolarPoint {
        angle_of_attack_deg: Some(values[0]),
        lift_coefficient: values[1],
        drag_coefficient: values[2],
        pitching_moment_coefficient: Some(values[3]),
    })
}

pub(super) fn stability_summary(points: &[PolarPoint]) -> StabilitySummary {
    let slope = points.first().zip(points.last()).and_then(|(first, last)| {
        let alpha_delta = last.angle_of_attack_deg? - first.angle_of_attack_deg?;
        let moment_delta = last.pitching_moment_coefficient? - first.pitching_moment_coefficient?;
        (alpha_delta.abs() > f64::EPSILON).then_some(moment_delta / alpha_delta)
    });
    StabilitySummary {
        pitching_moment_slope_per_deg: slope,
        statically_stable: slope.map(|value| value < 0.0),
        note: "Static pitch stability inferred from the VSPAERO CMy-alpha slope.".to_owned(),
    }
}

pub(super) fn maximum_lift_to_drag_ratio(points: &[PolarPoint]) -> Option<f64> {
    points
        .iter()
        .filter(|point| point.drag_coefficient > 0.0)
        .map(|point| point.lift_coefficient / point.drag_coefficient)
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .max_by(f64::total_cmp)
}

fn parse_failure(message: impl Into<String>) -> AexError {
    AexError::BackendExecution {
        backend: "openvsp".to_owned(),
        operation: "result parsing".to_owned(),
        message: message.into(),
    }
}

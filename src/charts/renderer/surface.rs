use crate::charts::spec::SurfaceSpec;
use crate::domain::diagnostic::{AexError, AexResult};

#[derive(Debug, Clone, Copy)]
pub(super) struct SurfaceCell {
    pub(super) x_min: f64,
    pub(super) x_max: f64,
    pub(super) y_min: f64,
    pub(super) y_max: f64,
    pub(super) intensity: f64,
    pub(super) feasible: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ContourSegment {
    pub(super) start: (f64, f64),
    pub(super) end: (f64, f64),
}

pub(super) fn validate(x: &[f64], y: &[f64], surface: &SurfaceSpec) -> AexResult<()> {
    let cell_count = x.len().saturating_mul(y.len());
    let valid = x.len() >= 2
        && y.len() >= 2
        && strictly_increasing(x)
        && strictly_increasing(y)
        && surface.values.len() == cell_count
        && surface.feasible_mask.len() == cell_count
        && surface.values.iter().all(|value| value.is_finite())
        && surface.contour_levels.iter().all(|value| value.is_finite());
    if valid {
        Ok(())
    } else {
        Err(AexError::analysis(
            "INVALID_CHART_SURFACE",
            "surface axes must increase and row-major values/mask must match their finite grid",
        ))
    }
}

pub(super) fn cells(x: &[f64], y: &[f64], surface: &SurfaceSpec) -> Vec<SurfaceCell> {
    let (minimum, maximum) = value_range(&surface.values);
    let span = (maximum - minimum).max(f64::EPSILON);
    let mut cells = Vec::with_capacity(surface.values.len());
    for (row, y_value) in y.iter().enumerate() {
        let (y_min, y_max) = coordinate_bounds(y, row, *y_value);
        for (column, x_value) in x.iter().enumerate() {
            let index = row * x.len() + column;
            let (x_min, x_max) = coordinate_bounds(x, column, *x_value);
            cells.push(SurfaceCell {
                x_min,
                x_max,
                y_min,
                y_max,
                intensity: (surface.values[index] - minimum) / span,
                feasible: surface.feasible_mask[index],
            });
        }
    }
    cells
}

pub(super) fn contours(
    x: &[f64],
    y: &[f64],
    surface: &SurfaceSpec,
    level: f64,
) -> Vec<ContourSegment> {
    let mut segments = Vec::new();
    for row in 0..y.len() - 1 {
        for column in 0..x.len() - 1 {
            let corners = [
                ((x[column], y[row]), surface.values[row * x.len() + column]),
                (
                    (x[column + 1], y[row]),
                    surface.values[row * x.len() + column + 1],
                ),
                (
                    (x[column + 1], y[row + 1]),
                    surface.values[(row + 1) * x.len() + column + 1],
                ),
                (
                    (x[column], y[row + 1]),
                    surface.values[(row + 1) * x.len() + column],
                ),
            ];
            let mut intersections = Vec::new();
            for edge in [(0, 1), (1, 2), (2, 3), (3, 0)] {
                if let Some(point) = intersection(corners[edge.0], corners[edge.1], level)
                    && !intersections.contains(&point)
                {
                    intersections.push(point);
                }
            }
            for pair in intersections.chunks_exact(2) {
                segments.push(ContourSegment {
                    start: pair[0],
                    end: pair[1],
                });
            }
        }
    }
    segments
}

fn intersection(
    first: ((f64, f64), f64),
    second: ((f64, f64), f64),
    level: f64,
) -> Option<(f64, f64)> {
    let first_delta = first.1 - level;
    let second_delta = second.1 - level;
    if first_delta == 0.0 {
        return Some(first.0);
    }
    if second_delta == 0.0 {
        return Some(second.0);
    }
    if first_delta.is_sign_positive() == second_delta.is_sign_positive() {
        return None;
    }
    let fraction = first_delta / (first_delta - second_delta);
    Some((
        first.0.0 + fraction * (second.0.0 - first.0.0),
        first.0.1 + fraction * (second.0.1 - first.0.1),
    ))
}

fn coordinate_bounds(values: &[f64], index: usize, value: f64) -> (f64, f64) {
    let lower = if index == 0 {
        value - (values[1] - value) * 0.5
    } else {
        (values[index - 1] + value) * 0.5
    };
    let upper = if index + 1 == values.len() {
        value + (value - values[index - 1]) * 0.5
    } else {
        (value + values[index + 1]) * 0.5
    };
    (lower, upper)
}

fn value_range(values: &[f64]) -> (f64, f64) {
    values.iter().copied().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
    )
}

fn strictly_increasing(values: &[f64]) -> bool {
    values
        .windows(2)
        .all(|pair| pair[0].is_finite() && pair[0] < pair[1])
        && values.last().is_some_and(|value| value.is_finite())
}

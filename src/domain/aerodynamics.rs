use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::domain::schema::AeroConfiguration;

pub(crate) const TABLE_POLAR_MODEL_ID: &str = "aero.polar_table";

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PolarTable {
    pub(crate) mach: Vec<f64>,
    pub(crate) cd0: Vec<f64>,
    pub(crate) cl_max: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) oswald_efficiency: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) induced_drag_factor: Option<Vec<f64>>,
}

impl PolarTable {
    pub(crate) fn interpolate(&self, mach: f64, aspect_ratio: f64) -> Option<PolarCoefficients> {
        let bracket = bracket(&self.mach, mach)?;
        let cd0 = interpolate_values(&self.cd0, bracket)?;
        let cl_max = interpolate_values(&self.cl_max, bracket)?;
        let induced_drag_factor = match (&self.oswald_efficiency, &self.induced_drag_factor) {
            (Some(values), None) => {
                let efficiency = interpolate_values(values, bracket)?;
                1.0 / (PI * efficiency * aspect_ratio)
            }
            (None, Some(values)) => interpolate_values(values, bracket)?,
            _ => return None,
        };
        Some(PolarCoefficients {
            cd0,
            cl_max,
            induced_drag_factor,
            extrapolated: bracket.extrapolated,
        })
    }
}

impl AeroConfiguration {
    pub(crate) fn polar_coefficients(&self, mach: f64, aspect_ratio: f64) -> PolarCoefficients {
        self.polar_table
            .as_ref()
            .and_then(|table| table.interpolate(mach, aspect_ratio))
            .unwrap_or_else(|| PolarCoefficients {
                cd0: self.cd0,
                cl_max: self.cl_max,
                induced_drag_factor: 1.0 / (PI * self.oswald_efficiency * aspect_ratio),
                extrapolated: false,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PolarCoefficients {
    pub(crate) cd0: f64,
    pub(crate) cl_max: f64,
    pub(crate) induced_drag_factor: f64,
    pub(crate) extrapolated: bool,
}

#[derive(Debug, Clone, Copy)]
struct AxisBracket {
    lower: usize,
    upper: usize,
    fraction: f64,
    extrapolated: bool,
}

fn bracket(axis: &[f64], value: f64) -> Option<AxisBracket> {
    let first = *axis.first()?;
    let final_index = axis.len().checked_sub(1)?;
    let last = *axis.get(final_index)?;
    if final_index == 0 {
        return None;
    }
    let (lower, upper, extrapolated) = if value < first {
        (0, 1, true)
    } else if value > last {
        (final_index - 1, final_index, true)
    } else {
        interior_bracket(axis, value, final_index)
    };
    Some(AxisBracket {
        lower,
        upper,
        fraction: (value - axis.get(lower)?) / (axis.get(upper)? - axis.get(lower)?),
        extrapolated,
    })
}

fn interior_bracket(axis: &[f64], value: f64, final_index: usize) -> (usize, usize, bool) {
    let upper = axis.partition_point(|axis_value| *axis_value < value);
    if upper == 0 {
        (0, 1, false)
    } else if upper >= axis.len() {
        (final_index - 1, final_index, false)
    } else {
        (upper - 1, upper, false)
    }
}

fn interpolate_values(values: &[f64], bracket: AxisBracket) -> Option<f64> {
    let lower = *values.get(bracket.lower)?;
    let upper = *values.get(bracket.upper)?;
    Some(lower + bracket.fraction * (upper - lower))
}

#[cfg(test)]
mod tests;

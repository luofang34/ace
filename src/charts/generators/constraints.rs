use crate::charts::spec::{Annotation, AxisSpec, ChartSpec, SeriesSpec, SurfaceSpec};
use crate::domain::result::ConstraintResult;
use crate::domain::schema::ResolvedScenario;

const LOADING_AXIS_POINTS: u32 = 48;

pub(crate) fn constraints(scenario: &ResolvedScenario, result: &ConstraintResult) -> ChartSpec {
    let (loading_label, loading_unit) = axis_parts(&result.y_axis);
    let required = required_loading(result);
    let loading_values = loading_axis(result, &required);
    let (values, feasible_mask) = surface_values(result, &required, &loading_values);
    ChartSpec {
        chart_type: "carpet".to_owned(),
        title: format!("{}: constraint diagram", scenario.name),
        x: AxisSpec {
            label: "Wing loading".to_owned(),
            unit: "N/m^2".to_owned(),
            values: result.wing_loading_n_m2.clone(),
        },
        y: AxisSpec {
            label: loading_label,
            unit: loading_unit.clone(),
            values: loading_values,
        },
        surface: Some(SurfaceSpec {
            label: "Loading margin".to_owned(),
            unit: loading_unit.clone(),
            values,
            feasible_mask,
            contour_levels: vec![0.0],
        }),
        series: result
            .constraints
            .iter()
            .map(|(id, values)| SeriesSpec {
                id: id.clone(),
                label: id.replace('_', " "),
                unit: loading_unit.clone(),
                values: values.clone(),
            })
            .collect(),
        annotations: vec![Annotation {
            x: result.selected_wing_loading_n_m2,
            y: result.selected_loading,
            label: "selected design".to_owned(),
        }],
        warnings: result.warnings.clone(),
    }
}

fn required_loading(result: &ConstraintResult) -> Vec<f64> {
    (0..result.wing_loading_n_m2.len())
        .map(|index| {
            result
                .constraints
                .values()
                .filter_map(|curve| curve.get(index))
                .copied()
                .fold(0.0, f64::max)
        })
        .collect()
}

fn loading_axis(result: &ConstraintResult, required: &[f64]) -> Vec<f64> {
    let maximum = required
        .iter()
        .copied()
        .chain([result.selected_loading])
        .fold(0.0, f64::max)
        .max(1.0)
        * 1.15;
    let mut values = (0..LOADING_AXIS_POINTS)
        .map(|index| maximum * f64::from(index) / f64::from(LOADING_AXIS_POINTS - 1))
        .collect::<Vec<_>>();
    values.push(result.selected_loading);
    values.sort_by(f64::total_cmp);
    values.dedup_by(|left, right| left.to_bits() == right.to_bits());
    values
}

fn surface_values(
    result: &ConstraintResult,
    required: &[f64],
    loading_values: &[f64],
) -> (Vec<f64>, Vec<bool>) {
    let mut values = Vec::with_capacity(required.len().saturating_mul(loading_values.len()));
    let mut feasible = Vec::with_capacity(values.capacity());
    for loading in loading_values {
        for (index, required_loading) in required.iter().enumerate() {
            let margin = loading - required_loading;
            values.push(margin);
            feasible.push(
                result.wing_loading_n_m2[index] <= result.stall_wing_loading_limit_n_m2
                    && margin >= 0.0,
            );
        }
    }
    (values, feasible)
}

fn axis_parts(raw: &str) -> (String, String) {
    raw.rsplit_once(" [").map_or_else(
        || (raw.to_owned(), "1".to_owned()),
        |(label, unit)| {
            (
                label.to_owned(),
                unit.strip_suffix(']').unwrap_or(unit).to_owned(),
            )
        },
    )
}

#[cfg(test)]
mod tests;

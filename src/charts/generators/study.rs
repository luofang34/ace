use ace_model::Quantity;
use ace_study::{Candidate, Feasibility, StudyArchive};

use crate::charts::spec::{AxisSpec, ChartSpec, SeriesSpec, SurfaceSpec};
use crate::domain::diagnostic::{AexError, AexResult};

/// One evaluated candidate projected onto its studied inputs and one output.
struct Row {
    inputs: Vec<(f64, String)>,
    value: f64,
    feasible: bool,
}

/// Chart one output of a vehicle study: a line for one studied input, or a feasibility-masked
/// carpet over a complete two-input grid.
pub(crate) fn study(archive: &StudyArchive, metric: &str) -> AexResult<ChartSpec> {
    let names = archive
        .candidates
        .first()
        .map(|candidate| candidate.updates.keys().cloned().collect::<Vec<_>>())
        .filter(|names| matches!(names.len(), 1 | 2))
        .ok_or_else(|| {
            AexError::analysis("EMPTY_STUDY", "study has no one- or two-input candidates")
        })?;
    let rows = archive
        .candidates
        .iter()
        .map(|candidate| row(candidate, &names, metric))
        .collect::<AexResult<Vec<_>>>()?;
    let unit = metric_unit(archive, metric)?;
    if names.len() == 1 {
        line_chart(&rows, &names[0], metric, unit)
    } else {
        surface_chart(&rows, &names, metric, unit)
    }
}

fn row(candidate: &Candidate, names: &[String], metric: &str) -> AexResult<Row> {
    if candidate.updates.keys().ne(names.iter()) {
        return Err(irregular_grid());
    }
    let inputs = candidate
        .updates
        .values()
        .map(|quantity| Ok((quantity.value, symbol(*quantity)?)))
        .collect::<AexResult<_>>()?;
    let value = candidate
        .state
        .values
        .get(metric)
        .and_then(|record| record.quantity)
        .map(|quantity| quantity.value)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            AexError::validation(
                "MISSING_STUDY_METRIC",
                metric,
                format!("candidate {} has no usable value", candidate.id),
            )
        })?;
    Ok(Row {
        inputs,
        value,
        feasible: matches!(
            candidate.feasibility,
            Feasibility::Satisfied | Feasibility::Provisional
        ),
    })
}

fn metric_unit(archive: &StudyArchive, metric: &str) -> AexResult<String> {
    let quantity = archive
        .candidates
        .first()
        .and_then(|candidate| candidate.state.values.get(metric))
        .and_then(|record| record.quantity)
        .ok_or_else(|| {
            AexError::validation("MISSING_STUDY_METRIC", metric, "metric not present")
        })?;
    symbol(quantity)
}

fn symbol(quantity: Quantity) -> AexResult<String> {
    serde_json::to_value(quantity.unit)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| AexError::analysis("INVALID_STUDY_UNIT", "unit has no symbol"))
}

fn line_chart(rows: &[Row], name: &str, metric: &str, unit: String) -> AexResult<ChartSpec> {
    let parsed = axis(rows, 0);
    Ok(ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("Vehicle study: {metric}"),
        x: AxisSpec {
            label: name.to_owned(),
            unit: common_unit(&parsed)?,
            values: parsed.iter().map(|(value, _)| *value).collect(),
        },
        y: AxisSpec {
            label: metric.to_owned(),
            unit: unit.clone(),
            values: Vec::new(),
        },
        surface: None,
        series: vec![SeriesSpec {
            id: metric.to_owned(),
            label: metric.to_owned(),
            unit,
            values: rows.iter().map(|row| row.value).collect(),
        }],
        annotations: Vec::new(),
        warnings: Vec::new(),
    })
}

fn surface_chart(
    rows: &[Row],
    names: &[String],
    metric: &str,
    unit: String,
) -> AexResult<ChartSpec> {
    let x_parsed = axis(rows, 0);
    let y_parsed = axis(rows, 1);
    let x_values = unique_values(&x_parsed);
    let y_values = unique_values(&y_parsed);
    let expected = x_values.len().saturating_mul(y_values.len());
    if expected != rows.len() {
        return Err(irregular_grid());
    }
    let mut values = vec![f64::NAN; expected];
    let mut feasible = vec![false; expected];
    let mut populated = vec![false; expected];
    for (row, ((x, _), (y, _))) in rows.iter().zip(x_parsed.iter().zip(&y_parsed)) {
        let column = value_index(&x_values, *x).ok_or_else(irregular_grid)?;
        let row_index = value_index(&y_values, *y).ok_or_else(irregular_grid)?;
        let index = row_index * x_values.len() + column;
        if populated[index] {
            return Err(irregular_grid());
        }
        values[index] = row.value;
        feasible[index] = row.feasible;
        populated[index] = true;
    }
    let contours = contour_levels(&values);
    Ok(ChartSpec {
        chart_type: "carpet".to_owned(),
        title: format!("Vehicle study: {metric}"),
        x: AxisSpec {
            label: names[0].clone(),
            unit: common_unit(&x_parsed)?,
            values: x_values,
        },
        y: AxisSpec {
            label: names[1].clone(),
            unit: common_unit(&y_parsed)?,
            values: y_values,
        },
        surface: Some(SurfaceSpec {
            label: metric.to_owned(),
            unit,
            values,
            feasible_mask: feasible,
            contour_levels: contours,
        }),
        series: Vec::new(),
        annotations: Vec::new(),
        warnings: Vec::new(),
    })
}

fn axis(rows: &[Row], index: usize) -> Vec<(f64, String)> {
    rows.iter()
        .filter_map(|row| row.inputs.get(index).cloned())
        .collect()
}

fn common_unit(values: &[(f64, String)]) -> AexResult<String> {
    let unit = values
        .first()
        .map(|(_, unit)| unit.clone())
        .unwrap_or_default();
    if values.iter().all(|(_, candidate)| candidate == &unit) {
        Ok(unit)
    } else {
        Err(AexError::analysis(
            "INCONSISTENT_STUDY_UNITS",
            "study axis uses inconsistent units",
        ))
    }
}

fn unique_values(values: &[(f64, String)]) -> Vec<f64> {
    let mut unique = values.iter().map(|(value, _)| *value).collect::<Vec<_>>();
    unique.sort_by(f64::total_cmp);
    unique.dedup_by(|left, right| left.to_bits() == right.to_bits());
    unique
}

fn value_index(values: &[f64], target: f64) -> Option<usize> {
    values
        .iter()
        .position(|value| value.to_bits() == target.to_bits())
}

fn contour_levels(values: &[f64]) -> Vec<f64> {
    let minimum = values.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (maximum - minimum).abs() <= f64::EPSILON {
        Vec::new()
    } else {
        [0.25, 0.5, 0.75]
            .map(|fraction| minimum + fraction * (maximum - minimum))
            .to_vec()
    }
}

fn irregular_grid() -> AexError {
    AexError::analysis(
        "IRREGULAR_STUDY_GRID",
        "two-input study candidates must form one complete Cartesian grid",
    )
}

#[cfg(test)]
mod tests;

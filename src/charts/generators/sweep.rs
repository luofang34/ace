use crate::charts::spec::{AxisSpec, ChartSpec, SeriesSpec, SurfaceSpec};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::result::{SweepResult, SweepRow};

pub(crate) fn sweep(result: &SweepResult, metric: &str) -> AexResult<ChartSpec> {
    let paths = result
        .rows
        .first()
        .map(|row| row.variables.keys().cloned().collect::<Vec<_>>())
        .filter(|paths| matches!(paths.len(), 1 | 2))
        .ok_or_else(|| AexError::analysis("EMPTY_SWEEP", "sweep has no plottable rows"))?;
    if result
        .rows
        .iter()
        .any(|row| row.variables.keys().ne(paths.iter()))
    {
        return Err(irregular_grid());
    }
    if paths.len() == 1 {
        line_chart(result, metric, &paths[0])
    } else {
        surface_chart(result, metric, &paths)
    }
}

fn line_chart(result: &SweepResult, metric: &str, path: &str) -> AexResult<ChartSpec> {
    let parsed = result
        .rows
        .iter()
        .map(|row| variable(row, path))
        .collect::<AexResult<Vec<_>>>()?;
    Ok(ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("Parameter sweep: {metric}"),
        x: AxisSpec {
            label: path.to_owned(),
            unit: common_unit(&parsed)?,
            values: parsed.iter().map(|(value, _)| *value).collect(),
        },
        y: AxisSpec {
            label: metric.to_owned(),
            unit: "SI".to_owned(),
            values: Vec::new(),
        },
        surface: None,
        series: vec![SeriesSpec {
            id: metric.to_owned(),
            label: metric.to_owned(),
            unit: "SI".to_owned(),
            values: metric_values(result, metric)?,
        }],
        annotations: Vec::new(),
        warnings: result.warnings.clone(),
    })
}

fn surface_chart(result: &SweepResult, metric: &str, paths: &[String]) -> AexResult<ChartSpec> {
    let x_parsed = parsed_axis(result, &paths[0])?;
    let y_parsed = parsed_axis(result, &paths[1])?;
    let x_values = unique_values(&x_parsed);
    let y_values = unique_values(&y_parsed);
    let expected = x_values.len().saturating_mul(y_values.len());
    if expected != result.rows.len() {
        return Err(irregular_grid());
    }
    let mut values = vec![f64::NAN; expected];
    let mut feasible = vec![false; expected];
    let mut populated = vec![false; expected];
    for row in &result.rows {
        let x = variable(row, &paths[0])?.0;
        let y = variable(row, &paths[1])?.0;
        let column = value_index(&x_values, x).ok_or_else(irregular_grid)?;
        let row_index = value_index(&y_values, y).ok_or_else(irregular_grid)?;
        let index = row_index * x_values.len() + column;
        if populated[index] {
            return Err(irregular_grid());
        }
        values[index] = metric_value(row, metric)?;
        feasible[index] = row.feasible;
        populated[index] = true;
    }
    if populated.iter().any(|populated| !populated) {
        return Err(irregular_grid());
    }
    let contours = contour_levels(&values);
    Ok(ChartSpec {
        chart_type: "carpet".to_owned(),
        title: format!("Parameter sweep: {metric}"),
        x: AxisSpec {
            label: paths[0].clone(),
            unit: common_unit(&x_parsed)?,
            values: x_values,
        },
        y: AxisSpec {
            label: paths[1].clone(),
            unit: common_unit(&y_parsed)?,
            values: y_values,
        },
        surface: Some(SurfaceSpec {
            label: metric.to_owned(),
            unit: "SI".to_owned(),
            values,
            feasible_mask: feasible,
            contour_levels: contours,
        }),
        series: Vec::new(),
        annotations: Vec::new(),
        warnings: result.warnings.clone(),
    })
}

fn parsed_axis(result: &SweepResult, path: &str) -> AexResult<Vec<(f64, String)>> {
    result.rows.iter().map(|row| variable(row, path)).collect()
}

fn variable(row: &SweepRow, path: &str) -> AexResult<(f64, String)> {
    let raw = row
        .variables
        .get(path)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AexError::analysis("INVALID_SWEEP_ROW", "variable is not a string"))?;
    let mut parts = raw.splitn(2, char::is_whitespace);
    let value = parts
        .next()
        .and_then(|number| number.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .ok_or_else(|| AexError::analysis("INVALID_SWEEP_ROW", "variable is not numeric"))?;
    Ok((value, parts.next().map_or("1", str::trim).to_owned()))
}

fn metric_values(result: &SweepResult, metric: &str) -> AexResult<Vec<f64>> {
    result
        .rows
        .iter()
        .map(|row| metric_value(row, metric))
        .collect()
}

fn metric_value(row: &SweepRow, metric: &str) -> AexResult<f64> {
    row.metrics
        .get(metric)
        .copied()
        .filter(|value| value.is_finite())
        .ok_or_else(|| AexError::validation("MISSING_SWEEP_METRIC", metric, "metric not present"))
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
            "INCONSISTENT_SWEEP_UNITS",
            "sweep axis uses inconsistent units",
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
        "IRREGULAR_SWEEP_GRID",
        "two-variable sweep rows must form one complete Cartesian grid",
    )
}

#[cfg(test)]
mod tests;

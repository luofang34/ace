use std::fs;
use std::path::Path;

use plotters::prelude::*;

use crate::charts::spec::ChartSpec;
use crate::domain::diagnostic::{AexError, AexResult};

pub(crate) fn render_svg_blocking(spec: &ChartSpec, output: &Path) -> AexResult<()> {
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|source| AexError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    validate_spec(spec)?;
    let x_range = numeric_range(&spec.x.values)?;
    let all_y: Vec<f64> = spec
        .series
        .iter()
        .flat_map(|series| series.values.iter().copied())
        .chain(spec.annotations.iter().map(|annotation| annotation.y))
        .collect();
    let y_range = numeric_range(&all_y)?;
    let backend = SVGBackend::new(output, (1100, 700));
    let root = backend.into_drawing_area();
    root.fill(&WHITE).map_err(plot_error)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(&spec.title, ("sans-serif", 30))
        .margin(20)
        .x_label_area_size(50)
        .y_label_area_size(70)
        .build_cartesian_2d(x_range.0..x_range.1, y_range.0..y_range.1)
        .map_err(plot_error)?;
    chart
        .configure_mesh()
        .x_desc(format!("{} [{}]", spec.x.label, spec.x.unit))
        .y_desc(format!("{} [{}]", spec.y.label, spec.y.unit))
        .draw()
        .map_err(plot_error)?;
    for (index, series) in spec.series.iter().enumerate() {
        let color = Palette99::pick(index);
        let points = spec
            .x
            .values
            .iter()
            .copied()
            .zip(series.values.iter().copied());
        chart
            .draw_series(LineSeries::new(points, &color))
            .map_err(plot_error)?
            .label(series.label.clone())
            .legend(move |(x, y)| PathElement::new([(x, y), (x + 20, y)], &color));
    }
    for annotation in &spec.annotations {
        chart
            .draw_series(PointSeries::of_element(
                [(annotation.x, annotation.y)],
                5,
                &BLACK,
                &|coordinate, size, style| {
                    EmptyElement::at(coordinate)
                        + Circle::new((0, 0), size, style.filled())
                        + Text::new(annotation.label.clone(), (8, -8), ("sans-serif", 14))
                },
            ))
            .map_err(plot_error)?;
    }
    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .map_err(plot_error)?;
    root.present().map_err(plot_error)
}

fn validate_spec(spec: &ChartSpec) -> AexResult<()> {
    if spec.x.values.is_empty() || spec.series.is_empty() {
        return Err(AexError::analysis(
            "EMPTY_CHART_SPEC",
            "chart requires x values and at least one series",
        ));
    }
    if spec
        .series
        .iter()
        .any(|series| series.values.len() != spec.x.values.len())
    {
        return Err(AexError::analysis(
            "INVALID_CHART_SPEC",
            "each series must match the x-axis length",
        ));
    }
    Ok(())
}

fn numeric_range(values: &[f64]) -> AexResult<(f64, f64)> {
    let mut minimum = f64::INFINITY;
    let mut maximum = f64::NEG_INFINITY;
    for value in values.iter().copied().filter(|value| value.is_finite()) {
        minimum = minimum.min(value);
        maximum = maximum.max(value);
    }
    if !minimum.is_finite() || !maximum.is_finite() {
        return Err(AexError::analysis(
            "INVALID_CHART_DATA",
            "chart data has no finite values",
        ));
    }
    if (maximum - minimum).abs() < 1.0e-12 {
        let padding = maximum.abs().max(1.0) * 0.05;
        return Ok((minimum - padding, maximum + padding));
    }
    let padding = (maximum - minimum) * 0.05;
    Ok((minimum - padding, maximum + padding))
}

fn plot_error<E: std::fmt::Display>(source: E) -> AexError {
    AexError::analysis("CHART_RENDER_ERROR", source.to_string())
}

use super::{ChartSpec, plot_error};
use crate::domain::diagnostic::AexResult;
use plotters::{
    coord::{Shift, types::RangedCoordf64},
    prelude::*,
};

pub(super) fn rows(spec: &ChartSpec) -> Vec<String> {
    let mut rows = Vec::new();
    for (i, annotation) in spec.annotations.iter().enumerate() {
        let text = format!("[{}] {}", i + 1, annotation.label);
        let mut row = String::new();
        for ch in text.chars() {
            if row.chars().count() >= 75 {
                rows.push(std::mem::take(&mut row));
            }
            row.push(ch);
        }
        rows.push(row);
    }
    rows
}

pub(super) fn legend<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    rows: &[String],
) -> AexResult<()> {
    for (i, row) in rows.iter().enumerate() {
        area.draw(&Text::new(
            row.as_str(),
            (30, 18 + i as i32 * 21),
            ("sans-serif", 14),
        ))
        .map_err(plot_error)?;
    }
    Ok(())
}

pub(super) fn draw<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    spec: &ChartSpec,
) -> AexResult<()> {
    let (xs, ys) = chart.plotting_area().get_pixel_range();
    let mut used = Vec::new();
    for (i, annotation) in spec.annotations.iter().enumerate() {
        let anchor = chart
            .as_coord_spec()
            .translate(&(annotation.x, annotation.y));
        let number = (i + 1).to_string();
        let size = [number.len() as i32 * 9 + 8, 20];
        let label = place(anchor, size, [xs.start, ys.start, xs.end, ys.end], &used);
        let Some(label) = label else {
            return Err(crate::domain::diagnostic::AexError::analysis(
                "CHART_LABEL_SPACE",
                "too many point labels for readable placement",
            ));
        };
        used.push(label);
        let offset = (label[0] - anchor.0, label[1] - anchor.1);
        chart
            .draw_series(std::iter::once(
                EmptyElement::at((annotation.x, annotation.y))
                    + Circle::new((0, 0), 4, BLACK.filled())
                    + PathElement::new([(0, 0), (offset.0, offset.1 + 10)], BLACK.mix(0.4))
                    + Rectangle::new(
                        [
                            (offset.0, offset.1),
                            (offset.0 + size[0], offset.1 + size[1]),
                        ],
                        WHITE.filled(),
                    )
                    + Text::new(number, (offset.0 + 4, offset.1 + 14), ("sans-serif", 14)),
            ))
            .map_err(plot_error)?;
    }
    Ok(())
}

pub(super) fn place(
    anchor: (i32, i32),
    size: [i32; 2],
    bounds: [i32; 4],
    used: &[[i32; 4]],
) -> Option<[i32; 4]> {
    for radius in 0..40 {
        for (dx, dy) in [
            (10 + radius * 12, -26),
            (-size[0] - 10 - radius * 12, -26),
            (10, 10 + radius * 24),
            (-size[0] - 10, -26 - radius * 24),
        ] {
            let x = (anchor.0 + dx).clamp(bounds[0] + 2, bounds[2] - size[0] - 2);
            let y = (anchor.1 + dy).clamp(bounds[1] + 2, bounds[3] - size[1] - 2);
            let candidate = [x, y, x + size[0], y + size[1]];
            if used.iter().all(|r| {
                candidate[2] + 3 < r[0]
                    || r[2] + 3 < candidate[0]
                    || candidate[3] + 3 < r[1]
                    || r[3] + 3 < candidate[1]
            }) {
                return Some(candidate);
            }
        }
    }
    None
}

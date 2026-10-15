use super::validation::{evidence, identifier, invalid, text};
use crate::{DrawingData, DrawingResult, FigureContent, FigureSpec};
use std::collections::BTreeSet;

pub(super) fn validate(data: &DrawingData, ids: &mut BTreeSet<String>) -> DrawingResult<()> {
    for fact in &data.facts {
        identifier(&fact.id, ids)?;
        text(&fact.label, false)?;
        if let Some(value) = &fact.value {
            text(value, false)?;
        }
        evidence(data, &fact.evidence)?;
    }
    let mut figures = BTreeSet::new();
    for figure in &data.figures {
        // Case-insensitive filesystems must not merge distinct requested exports.
        let filename = figure.id.to_ascii_lowercase();
        identifier(&filename, &mut figures)?;
        if filename == "drawing"
            || filename == "manifest"
            || !figure
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || figure.id.len() > 80
        {
            return Err(invalid(
                "figure IDs require 1..80 filename-safe letters, digits, hyphens or underscores",
            ));
        }
        text(&figure.title, false)?;
        text(&figure.reference, false)?;
        if !(100..=4096).contains(&figure.width_px)
            || !(100..=4096).contains(&figure.height_px)
            || u64::from(figure.width_px) * u64::from(figure.height_px) > 4_000_000
        {
            return Err(invalid(
                "figure dimensions require 100..4096 pixels and at most 4 million target pixels",
            ));
        }
        content(figure, data)?;
    }
    Ok(())
}

fn content(figure: &FigureSpec, data: &DrawingData) -> DrawingResult<()> {
    match &figure.content {
        FigureContent::GeneralArrangement => (),
        FigureContent::Curves { ids } => {
            let mut units: Option<[ace_model::Unit; 2]> = None;
            for id in ids {
                let curve = data
                    .results
                    .curves
                    .iter()
                    .find(|c| &c.id == id)
                    .ok_or_else(|| invalid(format!("unknown figure curve {id}")))?;
                let pair = [curve.x_axis.unit, curve.y_axis.unit];
                if let Some([x, y]) = units
                    && (!pair[0].compatible(x) || !pair[1].compatible(y))
                {
                    return Err(invalid("figure curves must have compatible axes"));
                }
                units = Some(pair);
            }
        }
        FigureContent::Facts { ids } => {
            for id in ids {
                if !data.facts.iter().any(|f| &f.id == id) {
                    return Err(invalid(format!("unknown figure fact {id}")));
                }
            }
        }
        FigureContent::Diagram { paths, labels } => {
            for path in paths {
                evidence(data, &path.evidence)?;
                if path.points.len() < 2 || path.points.len() > 100_000 {
                    return Err(invalid("diagram paths require 2..100000 points"));
                }
                for point in &path.points {
                    normalized(*point)?;
                }
            }
            for label in labels {
                evidence(data, &label.evidence)?;
                normalized(label.at)?;
                text(&label.text, false)?;
            }
        }
    }
    Ok(())
}

fn normalized(point: [f64; 2]) -> DrawingResult<()> {
    if point
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(invalid("diagram coordinates must lie in 0..1"));
    }
    Ok(())
}

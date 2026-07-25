use crate::charts::spec::{Annotation, AxisSpec, ChartSpec, SeriesSpec};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::evidence::StudyRunResult;
use crate::domain::warning::WarningCode;

#[derive(Debug, Clone)]
struct TradePoint {
    x: f64,
    y: f64,
    label: String,
    count: usize,
}

pub(crate) fn trade_space(result: &StudyRunResult) -> Option<ChartSpec> {
    let using_pareto = !result.pareto_candidates.is_empty();
    let candidates = if using_pareto {
        &result.pareto_candidates
    } else {
        &result.selected_candidates
    };
    let first = candidates.first()?;
    let objectives = first.objective_values.keys().take(2).collect::<Vec<_>>();
    if objectives.len() != 2 {
        return None;
    }
    let x_id = objectives[0];
    let y_id = objectives[1];
    let points = candidates
        .iter()
        .filter_map(|candidate| {
            Some(TradePoint {
                x: *candidate.objective_values.get(x_id)?,
                y: *candidate.objective_values.get(y_id)?,
                label: candidate.candidate.candidate_id.chars().take(18).collect(),
                count: 1,
            })
        })
        .collect::<Vec<_>>();
    let points = coalesce_points(points);
    let selection_label = if using_pareto {
        "feasible Pareto"
    } else {
        "selected"
    };
    let warnings = projection_warnings(first, x_id, y_id);
    Some(ChartSpec {
        chart_type: "scatter".to_owned(),
        title: format!("{}: {selection_label} trade space", result.study_id),
        x: AxisSpec {
            label: x_id.clone(),
            unit: "reported SI".to_owned(),
            values: points.iter().map(|point| point.x).collect(),
        },
        y: AxisSpec {
            label: y_id.clone(),
            unit: "reported SI".to_owned(),
            values: Vec::new(),
        },
        series: vec![SeriesSpec {
            id: if using_pareto {
                "pareto_candidates".to_owned()
            } else {
                "selected_candidates".to_owned()
            },
            label: format!("{selection_label} candidates"),
            unit: "reported SI".to_owned(),
            values: points.iter().map(|point| point.y).collect(),
        }],
        annotations: points
            .iter()
            .map(|point| Annotation {
                x: point.x,
                y: point.y,
                label: if point.count == 1 {
                    point.label.clone()
                } else {
                    format!("{} ×{}", point.label, point.count)
                },
            })
            .collect(),
        warnings,
    })
}

fn projection_warnings(
    first: &crate::domain::evidence::CandidateSummary,
    x_id: &str,
    y_id: &str,
) -> Vec<Diagnostic> {
    if first.objective_values.len() <= 2 {
        return Vec::new();
    }
    let omitted = first
        .objective_values
        .keys()
        .filter(|id| id.as_str() != x_id && id.as_str() != y_id)
        .cloned()
        .collect::<Vec<_>>();
    vec![Diagnostic::warning_with_context(
        WarningCode::StudyChartProjected,
        "trade-space chart projects a higher-dimensional objective set",
        "study.objectives",
        serde_json::json!({
            "shown": [x_id, y_id],
            "omitted": omitted,
        }),
    )]
}

fn coalesce_points(points: Vec<TradePoint>) -> Vec<TradePoint> {
    let mut unique = Vec::<TradePoint>::new();
    for point in points {
        if let Some(existing) = unique.iter_mut().find(|existing| {
            existing.x.to_bits() == point.x.to_bits() && existing.y.to_bits() == point.y.to_bits()
        }) {
            existing.count = existing.count.saturating_add(1);
        } else {
            unique.push(point);
        }
    }
    unique
}

#[cfg(test)]
mod tests;

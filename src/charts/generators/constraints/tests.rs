#![allow(clippy::expect_used)]

use tempfile::TempDir;

use crate::charts::renderer::render_svg_blocking;
use crate::charts::spec::ChartSpec;
use crate::domain::content_identity::digest_serializable;
use crate::domain::result::ConstraintResult;
use crate::models::constraints::ConstraintAnalyzer;
use crate::test_support::example_scenario;

use super::constraints;

fn b777_chart() -> Result<(ConstraintResult, ChartSpec), Box<dyn std::error::Error>> {
    let scenario = example_scenario("b777")?;
    let result = ConstraintAnalyzer::new(scenario.clone()).analyze(300.0, 9000.0, 80)?;
    let chart = constraints(&scenario, &result);
    Ok((result, chart))
}

fn golden_spec(chart: &ChartSpec) -> serde_json::Value {
    let surface = chart.surface.as_ref().map(|surface| {
        serde_json::json!({
            "label": surface.label,
            "unit": surface.unit,
            "values": rounded_values(&surface.values),
            "feasible_mask": surface.feasible_mask,
            "contour_levels": rounded_values(&surface.contour_levels),
        })
    });
    serde_json::json!({
        "chart_type": chart.chart_type,
        "title": chart.title,
        "x": {
            "label": chart.x.label,
            "unit": chart.x.unit,
            "values": rounded_values(&chart.x.values),
        },
        "y": {
            "label": chart.y.label,
            "unit": chart.y.unit,
            "values": rounded_values(&chart.y.values),
        },
        "surface": surface,
        "series": chart.series.iter().map(|series| serde_json::json!({
            "id": series.id,
            "label": series.label,
            "unit": series.unit,
            "values": rounded_values(&series.values),
        })).collect::<Vec<_>>(),
        "annotations": chart.annotations.iter().map(|annotation| serde_json::json!({
            "x": rounded(annotation.x),
            "y": rounded(annotation.y),
            "label": annotation.label,
        })).collect::<Vec<_>>(),
        "warning_codes": chart.warnings.iter().map(|warning| &warning.code).collect::<Vec<_>>(),
    })
}

fn rounded_values(values: &[f64]) -> Vec<f64> {
    values.iter().map(|value| rounded(*value)).collect()
}

fn rounded(value: f64) -> f64 {
    (value * 1.0e8).round() / 1.0e8
}

#[test]
fn b777_carpet_boundaries_and_selected_mask_match_constraint_analysis()
-> Result<(), Box<dyn std::error::Error>> {
    let (result, chart) = b777_chart()?;
    for series in &chart.series {
        assert_eq!(series.values, result.constraints[&series.id]);
    }
    let surface = chart.surface.as_ref().ok_or("missing carpet surface")?;
    let selected_row = chart
        .y
        .values
        .iter()
        .position(|value| value.to_bits() == result.selected_loading.to_bits())
        .ok_or("selected loading is not represented")?;
    let start = selected_row * chart.x.values.len();
    let end = start + chart.x.values.len();

    assert_eq!(
        &surface.feasible_mask[start..end],
        result.feasible_region_mask
    );
    Ok(())
}

#[test]
fn b777_carpet_chart_spec_matches_golden_digest() -> Result<(), Box<dyn std::error::Error>> {
    let (_, chart) = b777_chart()?;
    let digest = digest_serializable(&golden_spec(&chart))?;
    let expected =
        include_str!("../../../../tests/golden/charts/b777-constraint-chart.sha256").trim();

    assert_eq!(digest, expected);
    Ok(())
}

#[test]
fn b777_carpet_svg_contains_shading_boundaries_and_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let (_, chart) = b777_chart()?;
    let temporary = TempDir::new()?;
    let output = temporary.path().join("b777-carpet.svg");
    render_svg_blocking(&chart, &output)?;
    let svg = std::fs::read_to_string(output)?;

    assert!(svg.matches("<rect").count() > 100);
    assert!(svg.matches("<polyline").count() >= chart.series.len());
    assert!(svg.contains("selected design"));
    Ok(())
}

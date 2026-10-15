use tempfile::TempDir;

use crate::charts::spec::{AxisSpec, ChartSpec, SurfaceSpec};

use super::render_svg_blocking;

#[test]
fn crowded_point_labels_remain_separated_and_inside_chart_bounds() {
    let bounds = [0, 0, 900, 600];
    let mut boxes = Vec::new();
    for anchor in [(899, 1), (899, 1), (895, 2), (5, 598), (5, 598), (6, 592)] {
        let label = super::annotations::place(anchor, [26, 20], bounds, &boxes).unwrap();
        assert!(label[0] >= 0 && label[1] >= 0 && label[2] <= 900 && label[3] <= 600);
        assert!(boxes.iter().all(|other: &[i32; 4]| label[2] < other[0]
            || other[2] < label[0]
            || label[3] < other[1]
            || other[3] < label[1]));
        boxes.push(label);
    }
}

#[test]
fn actual_chart_export_keeps_complete_labels_outside_the_plot() {
    let mut spec = surface_chart(vec![-1., 1., 1., -1.], vec![true; 4]);
    spec.annotations = vec![
        crate::charts::spec::Annotation {
            x: 2.,
            y: 4.,
            label: "Full payload mission boundary".into(),
        },
        crate::charts::spec::Annotation {
            x: 2.,
            y: 4.,
            label: "Ferry mission boundary".into(),
        },
    ];
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("annotations.svg");
    render_svg_blocking(&spec, &path).unwrap();
    let svg = std::fs::read_to_string(path).unwrap();
    assert!(svg.contains("[1] Full payload mission boundary"));
    assert!(svg.contains("[2] Ferry mission boundary"));
    assert!(svg.contains("height=\"752\""));
}

fn surface_chart(values: Vec<f64>, feasible_mask: Vec<bool>) -> ChartSpec {
    ChartSpec {
        chart_type: "carpet".to_owned(),
        title: "surface".to_owned(),
        x: AxisSpec {
            label: "x".to_owned(),
            unit: "1".to_owned(),
            values: vec![1.0, 2.0],
        },
        y: AxisSpec {
            label: "y".to_owned(),
            unit: "1".to_owned(),
            values: vec![3.0, 4.0],
        },
        surface: Some(SurfaceSpec {
            label: "z".to_owned(),
            unit: "1".to_owned(),
            values,
            feasible_mask,
            contour_levels: vec![0.0],
        }),
        series: Vec::new(),
        annotations: Vec::new(),
        warnings: Vec::new(),
    }
}

#[test]
fn invalid_surface_fails_before_artifact_parent_is_created()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let parent = temporary.path().join("not-created");
    let output = parent.join("surface.svg");
    let error = render_svg_blocking(&surface_chart(vec![1.0], vec![true]), &output)
        .expect_err("malformed row-major data must fail");

    assert_eq!(error.detail().code, "INVALID_CHART_SURFACE");
    assert!(!parent.exists());
    Ok(())
}

#[test]
fn surface_renderer_emits_shading_and_contours() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let output = temporary.path().join("surface.svg");
    render_svg_blocking(
        &surface_chart(vec![-1.0, 1.0, -1.0, 1.0], vec![false, true, false, true]),
        &output,
    )?;
    let svg = std::fs::read_to_string(output)?;

    assert!(svg.matches("<rect").count() >= 5);
    assert!(svg.contains("opacity="));
    assert!(svg.contains("<polyline"));
    Ok(())
}

#[test]
fn legacy_chart_json_defaults_the_additive_surface() -> Result<(), Box<dyn std::error::Error>> {
    let chart: ChartSpec = serde_json::from_value(serde_json::json!({
        "chart_type": "line",
        "title": "legacy",
        "x": {"label": "x", "unit": "1", "values": [1.0, 2.0]},
        "y": {"label": "y", "unit": "1", "values": []},
        "series": [{"id": "y", "label": "y", "unit": "1", "values": [2.0, 3.0]}],
        "annotations": [],
        "warnings": []
    }))?;

    assert!(chart.surface.is_none());
    Ok(())
}

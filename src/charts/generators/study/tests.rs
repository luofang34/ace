use ace_engine::{Registry, Session};
use ace_model::VehicleModel;
use ace_study::StudyPlan;
use serde_json::json;

use super::study;

fn model() -> VehicleModel {
    serde_json::from_value(json!({"schema_version":1,"id":"chart_fixture","name":"Chart fixture","domain":"vehicle",
        "parameters": {
            "x":{"unit":"m","value":{"value":0,"unit":"m"},"source":{"kind":"test","reference":"input"},"owner":"test"},
            "y":{"unit":"1","value":{"value":1,"unit":"1"},"source":{"kind":"test","reference":"input"},"owner":"test"},
            "area":{"unit":"m^2","source":{"kind":"test","reference":"product"},"owner":"test","expression":{"op":"multiply",
                "left":{"op":"multiply","left":{"op":"reference","parameter":"x"},"right":{"op":"reference","parameter":"x"}},
                "right":{"op":"reference","parameter":"y"}}}},
        "requirements":{"limit":{"name":"Area limit","metric":"area","relation":"le","target":{"value":5,"unit":"m^2"},
            "priority":"hard","source":{"kind":"test","reference":"SRD"}}},
        "design_variables":[
            {"parameter":"x","lower":{"value":0,"unit":"m"},"upper":{"value":4,"unit":"m"}},
            {"parameter":"y","lower":{"value":0,"unit":"1"},"upper":{"value":4,"unit":"1"}}]}))
    .unwrap()
}

fn archive(axes: serde_json::Value) -> ace_study::StudyArchive {
    let plan: StudyPlan = serde_json::from_value(
        json!({"schema_version":1,"max_evaluations":16,"method":{"kind":"grid","axes":axes}}),
    )
    .unwrap();
    let session = Session::new(model(), Registry::default()).unwrap();
    ace_study::run(&session, &plan).unwrap()
}

#[test]
fn one_input_studies_chart_a_line_in_the_output_unit() {
    let archive = archive(
        json!({"x":{"start":{"value":1,"unit":"m"},"stop":{"value":3,"unit":"m"},"count":3}}),
    );
    let chart = study(&archive, "area").unwrap();
    assert_eq!(chart.chart_type, "line");
    assert_eq!(chart.x.values, [1.0, 2.0, 3.0]);
    assert_eq!(chart.x.unit, "m");
    assert_eq!(chart.series[0].values, [1.0, 4.0, 9.0]);
    assert_eq!(chart.series[0].unit, "m^2");
}

#[test]
fn two_input_grids_chart_a_row_major_surface_masked_by_feasibility() {
    let archive = archive(json!({
        "x":[{"value":1,"unit":"m"},{"value":2,"unit":"m"}],
        "y":[{"value":1,"unit":"1"},{"value":2,"unit":"1"}]}));
    let chart = study(&archive, "area").unwrap();
    let surface = chart.surface.unwrap();
    assert_eq!(chart.x.values, [1.0, 2.0]);
    assert_eq!(chart.y.values, [1.0, 2.0]);
    assert_eq!(surface.values, [1.0, 4.0, 2.0, 8.0]);
    assert_eq!(surface.feasible_mask, [true, true, true, false]);
}

#[test]
fn unknown_metrics_have_a_stable_error() {
    let archive = archive(json!({"x":[{"value":1,"unit":"m"}]}));
    let error = study(&archive, "missing").expect_err("unknown metric must fail");
    assert_eq!(error.detail().code, "MISSING_STUDY_METRIC");
}

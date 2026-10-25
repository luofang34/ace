use super::*;
use ace_model::Unit;
use serde_json::json;

fn q(value: f64, unit: Unit) -> Quantity {
    Quantity::new(value, unit).unwrap()
}

fn range(start: Quantity, stop: Quantity, count: u32, spacing: Spacing) -> Axis {
    Axis::Range(AxisRange {
        start,
        stop,
        count,
        spacing,
    })
}

#[test]
fn linear_ranges_include_both_endpoints_in_the_start_unit() {
    let axis = range(
        q(0.0, Unit::Foot),
        q(0.6096, Unit::Meter),
        3,
        Spacing::Linear,
    );
    let samples = axis.samples("x").unwrap();
    assert!(samples.iter().all(|sample| sample.unit == Unit::Foot));
    let values: Vec<_> = samples.iter().map(|sample| sample.value).collect();
    assert_eq!(values[0], 0.0);
    assert!((values[1] - 1.0).abs() < 1e-12);
    assert!((values[2] - 2.0).abs() < 1e-12);
}

#[test]
fn logarithmic_ranges_keep_equal_ratios_and_reject_nonpositive_endpoints() {
    let axis = range(
        q(1.0, Unit::Meter),
        q(100.0, Unit::Meter),
        3,
        Spacing::Logarithmic,
    );
    let samples = axis.samples("x").unwrap();
    assert!((samples[1].value - 10.0).abs() < 1e-12);
    assert!((samples[2].value - 100.0).abs() < 1e-12);
    let invalid = range(
        q(0.0, Unit::Meter),
        q(100.0, Unit::Meter),
        3,
        Spacing::Logarithmic,
    );
    assert!(invalid.samples("x").is_err());
}

#[test]
fn ranges_reject_single_samples_and_mismatched_dimensions() {
    assert!(
        range(q(1.0, Unit::Meter), q(2.0, Unit::Meter), 1, Spacing::Linear)
            .samples("x")
            .is_err()
    );
    assert!(
        range(
            q(1.0, Unit::Meter),
            q(2.0, Unit::Kilogram),
            2,
            Spacing::Linear
        )
        .samples("x")
        .is_err()
    );
}

#[test]
fn plans_accept_explicit_and_generated_axes_with_unchanged_explicit_encoding() {
    let explicit = json!({"schema_version":1,"max_evaluations":4,"method":{"kind":"grid","axes":{
        "x":[{"value":1.5,"unit":"m"},{"value":2.5,"unit":"m"}]}}});
    let plan: StudyPlan = serde_json::from_value(explicit.clone()).unwrap();
    assert_eq!(serde_json::to_value(&plan).unwrap(), explicit);
    let generated: StudyPlan = serde_json::from_value(json!({"schema_version":1,"max_evaluations":4,
        "method":{"kind":"grid","axes":{"x":{"start":{"value":1,"unit":"m"},"stop":{"value":2,"unit":"m"},
        "count":2,"spacing":"logarithmic"}}}}))
    .unwrap();
    let SearchMethod::Grid { axes } = generated.method else {
        panic!("grid plan expected");
    };
    assert_eq!(axes["x"].samples("x").unwrap().len(), 2);
    assert!(
        serde_json::from_value::<StudyPlan>(json!({"schema_version":1,"max_evaluations":4,
            "method":{"kind":"grid","axes":{"x":{"start":{"value":1,"unit":"m"},
            "stop":{"value":2,"unit":"m"},"count":2,"step":1}}}}))
        .is_err()
    );
}

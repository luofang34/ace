use super::*;
use ace_engine::Registry;
use ace_model::Unit;
use serde_json::json;

fn q(value: f64, unit: Unit) -> Quantity {
    Quantity::new(value, unit).unwrap()
}

/// force = k * v^2, with k a predicted coefficient and v a test condition.
fn model() -> VehicleModel {
    serde_json::from_value(json!({"schema_version":1,"id":"drag","name":"Quadratic drag","domain":"vehicle",
        "parameters":{
            "k":{"unit":"1","value":{"value":2,"unit":"1"},"lower":{"value":0,"unit":"1"},"upper":{"value":10,"unit":"1"},
                "source":{"kind":"estimated","reference":"handbook"},"owner":"aero"},
            "v":{"unit":"m","value":{"value":5,"unit":"m"},"source":{"kind":"test","reference":"condition"},"owner":"test"},
            "force":{"unit":"m^2","source":{"kind":"derived","reference":"k v^2"},"owner":"aero",
                "expression":{"op":"multiply","left":{"op":"reference","parameter":"k"},
                    "right":{"op":"power","base":{"op":"reference","parameter":"v"},"exponent":2}}}}})).unwrap()
}

fn case(measured: f64, upper: f64) -> CalibrationCase {
    CalibrationCase {
        schema_version: 1,
        name: "run-12".into(),
        measurement: MeasurementSource {
            citation: "Illustrative test log, not a real measurement".into(),
            article: "serial 0001".into(),
            assumptions: vec!["steady condition".into()],
        },
        conditions: BTreeMap::from([("v".into(), q(10.0, Unit::Meter))]),
        observations: BTreeMap::from([(
            "force".into(),
            Observation {
                value: q(measured, Unit::SquareMeter),
                standard_deviation: q(1.0, Unit::SquareMeter),
            },
        )]),
        parameters: vec![CalibratedParameter {
            parameter: "k".into(),
            lower: q(0.5, Unit::Ratio),
            upper: q(upper, Unit::Ratio),
        }],
        sweeps: 2,
    }
}

fn session() -> Session {
    Session::new(model(), Registry::default()).unwrap()
}

#[test]
fn measurements_overturn_the_prediction_and_calibrated_values_carry_their_evidence() {
    let baseline = session();
    let record = case(250.0, 6.0).run(&baseline).unwrap();
    assert!((record.prior.residuals["force"].normalized.unwrap() + 50.0).abs() < 1e-9);
    assert!(record.posterior.chi_square.unwrap() < 1e-6);
    assert!((record.posterior.parameters["k"].value - 2.5).abs() < 1e-5);
    assert!(record.consistent);
    assert_eq!(record.degrees_of_freedom, 0);
    assert_eq!(baseline.model().parameters["k"].value.unwrap().value, 2.0);
    let (updates, source) = record.updates();
    assert_eq!(updates.keys().collect::<Vec<_>>(), ["k"]);
    assert_eq!(source.kind, "calibrated");
    assert!(source.reference.ends_with(&record.id));
    let mut calibrated = baseline.fork();
    calibrated.edit(&updates, source).unwrap();
    let snapshot = calibrated.run().unwrap();
    assert_eq!(snapshot.values["k"].source.kind, "calibrated");
    assert!((snapshot.values["force"].quantity.unwrap().value - 62.5).abs() < 1e-3);
}

#[test]
fn unreachable_measurements_are_reported_inconsistent_rather_than_forced() {
    let record = case(900.0, 6.0).run(&session()).unwrap();
    assert!(!record.consistent);
    assert!((record.posterior.parameters["k"].value - 6.0).abs() < 1e-5);
    assert!(record.posterior.chi_square < record.prior.chi_square);
}

#[test]
fn identical_cases_reproduce_identical_records_and_tampering_is_detected() {
    let baseline = session();
    let first = case(250.0, 6.0).run(&baseline).unwrap();
    assert_eq!(first.id, case(250.0, 6.0).run(&baseline).unwrap().id);
    let mut tampered: CalibrationRecord =
        serde_json::from_slice(&serde_json::to_vec(&first).unwrap()).unwrap();
    tampered.verify_integrity().unwrap();
    tampered.consistent = false;
    assert!(tampered.verify_integrity().is_err());
}

#[test]
fn invalid_cases_are_rejected_before_any_evaluation() {
    let baseline = session();
    let mut computed = case(250.0, 6.0);
    computed.parameters[0].parameter = "force".into();
    computed.parameters[0].lower = q(1.0, Unit::SquareMeter);
    computed.parameters[0].upper = q(2.0, Unit::SquareMeter);
    assert!(computed.run(&baseline).is_err());
    let mut outside = case(250.0, 11.0);
    assert!(outside.run(&baseline).is_err());
    outside = case(250.0, 6.0);
    outside
        .observations
        .get_mut("force")
        .unwrap()
        .standard_deviation = q(0.0, Unit::SquareMeter);
    assert!(outside.run(&baseline).is_err());
    let mut condition = case(250.0, 6.0);
    condition.conditions.insert("k".into(), q(1.0, Unit::Ratio));
    assert!(condition.run(&baseline).is_err());
    let mut anonymous = case(250.0, 6.0);
    anonymous.measurement.article = " ".into();
    assert!(anonymous.run(&baseline).is_err());
}

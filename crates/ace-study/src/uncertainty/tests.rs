use super::sampling::{InputDistribution, SplitMix64};
use super::*;
use ace_engine::Registry;
use ace_model::Unit;
use serde_json::json;

fn q(value: f64, unit: Unit) -> Quantity {
    Quantity::new(value, unit).unwrap()
}

fn requirement(target: f64, confidence: f64) -> serde_json::Value {
    json!({"name":"Reach limit","metric":"reach","relation":"le","target":{"value":target,"unit":"m"},
        "priority":"hard","confidence":confidence,"source":{"kind":"test","reference":"SRD"}})
}

fn model(requirements: serde_json::Value) -> VehicleModel {
    serde_json::from_value(json!({"schema_version":1,"id":"uncertain","name":"Linear response","domain":"vehicle",
        "parameters":{
            "x":{"unit":"m","value":{"value":2,"unit":"m"},"source":{"kind":"test","reference":"input"},"owner":"test",
                "uncertainty":{"standard_deviation":{"value":0.5,"unit":"m"},"distribution":"normal"}},
            "reach":{"unit":"m","source":{"kind":"test","reference":"3x"},"owner":"test",
                "expression":{"op":"multiply","left":{"op":"constant","value":{"value":3,"unit":"1"}},"right":{"op":"reference","parameter":"x"}}}},
        "requirements":requirements})).unwrap()
}

fn plan(samples: u32) -> UncertaintyPlan {
    UncertaintyPlan {
        schema_version: 1,
        samples,
        seed: 7,
        metrics: Vec::new(),
    }
}

fn session(model: VehicleModel) -> Session {
    Session::new(model, Registry::default()).unwrap()
}

#[test]
fn generator_matches_the_published_splitmix64_sequence() {
    let mut random = SplitMix64::new(0);
    assert_eq!(random.next_u64(), 0xe220_a839_7b1d_cdaf);
    assert_eq!(random.next_u64(), 0x6e78_9e6a_a1b9_65f4);
    assert_eq!(random.next_u64(), 0x06c4_5d18_8009_454f);
}

#[test]
fn normal_and_uniform_draws_reproduce_declared_moments_and_bounds() {
    let mut random = SplitMix64::new(11);
    for (distribution, lower, upper) in [
        ("normal", None, None),
        ("uniform", None, None),
        (
            "normal",
            Some(q(9.0, Unit::Meter)),
            Some(q(11.5, Unit::Meter)),
        ),
    ] {
        let input = InputDistribution {
            distribution: distribution.into(),
            mean: q(10.0, Unit::Meter),
            standard_deviation: q(2.0, Unit::Meter),
            lower,
            upper,
        };
        let draws: Vec<f64> = (0..20_000)
            .map(|_| input.draw("x", &mut random).unwrap().value)
            .collect();
        let stats = MetricStatistics::from_values(Unit::Meter, draws.clone(), 0).unwrap();
        if lower.is_some() {
            assert!(stats.minimum >= 9.0 && stats.maximum <= 11.5);
            continue;
        }
        assert!((stats.mean - 10.0).abs() < 0.05, "{distribution}");
        assert!(
            (stats.standard_deviation - 2.0).abs() < 0.05,
            "{distribution}"
        );
        if distribution == "uniform" {
            let half = 2.0 * 3.0_f64.sqrt();
            assert!(stats.minimum >= 10.0 - half && stats.maximum < 10.0 + half);
        }
    }
    let impossible = InputDistribution {
        distribution: "normal".into(),
        mean: q(0.0, Unit::Meter),
        standard_deviation: q(1.0, Unit::Meter),
        lower: Some(q(0.0, Unit::Meter)),
        upper: Some(q(1e-12, Unit::Meter)),
    };
    assert!(impossible.draw("x", &mut random).is_err());
}

#[test]
fn requirement_probabilities_use_finite_sample_intervals_and_never_claim_satisfied() {
    let requirements = json!({"loose":requirement(9.0, 0.8),"tight":requirement(6.0, 0.9)});
    let record = plan(2000)
        .run(&session(model(requirements.clone())))
        .unwrap();
    let loose = &record.requirements["loose"];
    assert_eq!(loose.status, ProbabilityStatus::Pass);
    assert!((loose.probability - 0.977).abs() < 0.01);
    assert!(loose.lower <= loose.probability && loose.probability <= loose.upper);
    assert_eq!(record.requirements["tight"].status, ProbabilityStatus::Fail);
    assert!((record.requirements["tight"].probability - 0.5).abs() < 0.04);
    assert_eq!(record.feasibility, Feasibility::Violated);
    assert!((record.metrics["reach"].mean - 6.0).abs() < 0.05);
    assert!((record.metrics["reach"].standard_deviation - 1.5).abs() < 0.07);
    let passing = plan(2000)
        .run(&session(model(json!({"loose":requirement(9.0, 0.8)}))))
        .unwrap();
    assert_eq!(passing.feasibility, Feasibility::Provisional);
    let few = plan(40)
        .run(&session(model(json!({"near":requirement(6.3, 0.58)}))))
        .unwrap();
    assert_eq!(
        few.requirements["near"].status,
        ProbabilityStatus::Indeterminate
    );
    assert_eq!(few.feasibility, Feasibility::Unknown);
}

#[test]
fn identical_plans_reproduce_identical_evidence_and_tampering_is_detected() {
    let baseline = session(model(json!({"loose":requirement(9.0, 0.8)})));
    let first = plan(50).run(&baseline).unwrap();
    let second = plan(50).run(&baseline).unwrap();
    assert_eq!(first.id, second.id);
    assert!(baseline.snapshot().runs.is_empty());
    let mut other = plan(50);
    other.seed = 8;
    assert_ne!(other.run(&baseline).unwrap().id, first.id);
    let mut tampered: UncertaintyRecord =
        serde_json::from_slice(&serde_json::to_vec(&first).unwrap()).unwrap();
    tampered.verify_integrity().unwrap();
    tampered.requirements.get_mut("loose").unwrap().satisfied = 50;
    assert!(tampered.verify_integrity().is_err());
}

#[test]
fn only_independent_inputs_with_known_distributions_may_be_uncertain() {
    let mut computed = model(json!({}));
    let declared = computed.parameters["x"].uncertainty.clone();
    computed.parameters.get_mut("reach").unwrap().uncertainty = declared;
    assert!(plan(5).run(&session(computed)).is_err());
    let mut exact = model(json!({}));
    exact.parameters.get_mut("x").unwrap().uncertainty = None;
    assert!(plan(5).run(&session(exact)).is_err());
    let mut unknown = model(json!({}));
    if let Some(item) = unknown
        .parameters
        .get_mut("x")
        .unwrap()
        .uncertainty
        .as_mut()
    {
        item.distribution = "lognormal".into();
    }
    assert!(unknown.validate().is_err());
    for confidence in [0.0, 1.0, f64::NAN] {
        let mut model = model(json!({"loose":requirement(9.0, 0.5)}));
        model.requirements.get_mut("loose").unwrap().confidence = Some(confidence);
        assert!(model.validate().is_err());
    }
    assert!(plan(0).run(&session(model(json!({})))).is_err());
}

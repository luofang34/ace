use super::*;
use serde_json::json;

fn resources() -> AnalysisResources {
    let object=serde_json::from_value(json!({"name":"body","role":"pressure_vessel",
        "source":{"kind":"test","reference":"elliptical cylinder"},
        "shape":{"kind":"body","length":{"value":10,"unit":"m"},"width":{"value":4,"unit":"m"},
        "height":{"value":2,"unit":"m"},"nose":{"value":2,"unit":"m"},"tail":{"value":2,"unit":"m"}}})).unwrap();
    AnalysisResources {
        geometry: BTreeMap::from([("fuselage".into(), object)]),
        ..Default::default()
    }
}

#[test]
fn generated_body_has_canonical_extents_and_independent_section_area() {
    let report = evaluate_concept_geometry(&resources(), &BTreeMap::new()).unwrap();
    let body = &report.objects["fuselage"];
    assert_eq!(body.minimum_m, [0.0, -2.0, -1.0]);
    assert_eq!(body.maximum_m, [10.0, 2.0, 1.0]);
    let area = concept_section_area(body, 5.0);
    let expected = 32.0 * 2.0 * (std::f64::consts::TAU / 32.0).sin() / 2.0;
    assert!((area - expected).abs() < 1e-10);
    assert!(concept_section(body, 11.0).is_empty());
}

#[test]
fn native_surface_and_envelope_share_placement() {
    let mut r = resources();
    r.geometry
        .get_mut("fuselage")
        .unwrap()
        .placement
        .translation[0] =
        ace_model::Binding::Constant(Quantity::new(3.0, ace_model::Unit::Meter).unwrap());
    let report = evaluate_concept_geometry(&r, &BTreeMap::new()).unwrap();
    let envelopes = crate::evaluate_geometry(&r, &BTreeMap::new()).unwrap();
    assert_eq!(
        report.objects["fuselage"].minimum_m,
        envelopes.envelopes["fuselage"].minimum_m
    );
    assert_eq!(report.objects["fuselage"].maximum_m, [13.0, 2.0, 1.0]);
}

#[test]
fn invalid_shape_domain_fails_instead_of_drawing_an_invented_surface() {
    let mut r = resources();
    if let Some(ace_model::ConceptShape::Body { nose, .. }) =
        r.geometry.get_mut("fuselage").unwrap().shape.as_deref_mut()
    {
        *nose = ace_model::Binding::Constant(Quantity::new(20.0, ace_model::Unit::Meter).unwrap());
    }
    assert!(evaluate_concept_geometry(&r, &BTreeMap::new()).is_err());
}

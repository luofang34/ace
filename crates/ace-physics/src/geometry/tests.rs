use super::*;
use serde_json::json;

fn geometry(minimum: [f64; 3], maximum: [f64; 3]) -> ace_model::GeometryObject {
    serde_json::from_value(json!({"role":"fuel_tank", "name":"Test envelope",
        "source":{"kind":"test","reference":"geometry fixture"},
        "bounds":{"minimum":minimum.map(|v| json!({"value":v,"unit":"m"})),
        "maximum":maximum.map(|v| json!({"value":v,"unit":"m"}))}}))
    .unwrap()
}

fn fixture() -> AnalysisResources {
    let mut resources = AnalysisResources::default();
    resources
        .geometry
        .insert("a".into(), geometry([0.0; 3], [2.0; 3]));
    resources
        .geometry
        .insert("b".into(), geometry([3.0, 0.0, 0.0], [4.0, 2.0, 2.0]));
    resources.clearances.insert(
        "gap".into(),
        serde_json::from_value(json!({
        "first":"a","second":"b","minimum":{"value":0.5,"unit":"m"}}))
        .unwrap(),
    );
    resources
}

#[test]
fn signed_clearance_and_automatic_section_use_closest_witnesses() {
    let resources = fixture();
    let report = evaluate_geometry(&resources, &BTreeMap::new()).unwrap();
    assert_eq!(report.clearances["gap"].clearance_m, 1.0);
    assert_eq!(report.clearances["gap"].margin_m, 0.5);
    let section = &report.sections[0];
    assert_eq!(section.clearance.as_deref(), Some("gap"));
    assert_eq!(section.slices.len(), 2);
    assert_eq!(report.envelopes["a"].envelope_volume_m3, 8.0);
    assert!(report.envelopes["a"].usable_volume_m3.is_none());
}

#[test]
fn overlapping_envelopes_are_negative_and_sections_rank_by_margin() {
    let mut resources = fixture();
    resources
        .geometry
        .insert("c".into(), geometry([1.5, 0.0, 0.0], [3.5, 2.0, 2.0]));
    let mut check = resources.clearances["gap"].clone();
    check.second = "c".into();
    resources.clearances.insert("collision".into(), check);
    let report = evaluate_geometry(&resources, &BTreeMap::new()).unwrap();
    assert_eq!(report.clearances["collision"].clearance_m, -0.5);
    assert_eq!(report.sections[0].clearance.as_deref(), Some("collision"));
}

#[test]
fn sectional_union_does_not_double_count_nested_volumes() {
    let mut resources = fixture();
    resources
        .geometry
        .insert("inner".into(), geometry([0.5; 3], [1.5; 3]));
    let report = evaluate_geometry(&resources, &BTreeMap::new()).unwrap();
    let station = report
        .area_distribution
        .iter()
        .find(|station| station.station_m == 1.0)
        .unwrap();
    assert_eq!(station.envelope_area_m2, 4.0);
}

#[test]
fn diagonal_separation_uses_euclidean_distance() {
    let mut resources = fixture();
    resources
        .geometry
        .insert("b".into(), geometry([5.0, 6.0, 0.0], [6.0, 7.0, 1.0]));
    let report = evaluate_geometry(&resources, &BTreeMap::new()).unwrap();
    assert_eq!(report.clearances["gap"].clearance_m, 5.0);
}

#[test]
fn usable_volume_gets_its_smallest_envelope_cross_section() {
    let mut resources = fixture();
    let mut tank = geometry([0.0; 3], [2.0, 10.0, 3.0]);
    tank.volume = Some(ace_model::Binding::Constant(
        Quantity::new(50.0, Unit::CubicMeter).unwrap(),
    ));
    resources.geometry.insert("tank".into(), tank);
    let report = evaluate_geometry(&resources, &BTreeMap::new()).unwrap();
    let section = report
        .sections
        .iter()
        .find(|section| section.id == "volume.tank")
        .unwrap();
    assert_eq!(section.axis, 1);
    assert_eq!(section.station_m, 5.0);
    assert_eq!(section.slices.len(), 1);
    assert_eq!(section.slices[0].geometry, "tank");
    assert_eq!(section.slices[0].minimum_m, [0.0, 0.0]);
    assert_eq!(section.slices[0].maximum_m, [2.0, 3.0]);
    assert!(
        !report
            .sections
            .iter()
            .any(|section| section.id == "volume.a")
    );
}

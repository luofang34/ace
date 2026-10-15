use super::*;
use serde_json::json;

fn item(mass: f64, cg: [f64; 3]) -> MassItem {
    serde_json::from_value(json!({"kind":"empty","mass":{"value":mass,"unit":"kg"},
        "cg":cg.map(|value| json!({"value":value,"unit":"m"})),
        "inertia":{"xx":{"value":0,"unit":"kg*m^2"},"yy":{"value":0,"unit":"kg*m^2"},
        "zz":{"value":0,"unit":"kg*m^2"},"xy":{"value":0,"unit":"kg*m^2"},
        "xz":{"value":0,"unit":"kg*m^2"},"yz":{"value":0,"unit":"kg*m^2"}},
        "source":{"kind":"test","reference":"point mass"}}))
    .unwrap()
}

#[test]
fn three_dimensional_cg_and_parallel_axis_tensor() {
    let items = BTreeMap::from([
        ("a".into(), item(2.0, [0.0; 3])),
        ("b".into(), item(2.0, [2.0, 4.0, 6.0])),
    ]);
    let state = mass_state(&items, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(state.cg_m, [1.0, 2.0, 3.0]);
    assert_eq!(
        state.inertia_kg_m2.unwrap(),
        [
            [52.0, -8.0, -12.0],
            [-8.0, 40.0, -24.0],
            [-12.0, -24.0, 20.0]
        ]
    );
}

#[test]
fn absent_intrinsic_tensor_remains_unknown() {
    let mut component = item(10.0, [0.0; 3]);
    component.inertia = None;
    let state = mass_state(
        &BTreeMap::from([("a".into(), component)]),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(state.inertia_kg_m2.is_none());
}

#[test]
fn changed_loading_cannot_reuse_intrinsic_tensor_for_a_different_mass() {
    let items = BTreeMap::from([("a".into(), item(10.0, [0.0; 3]))]);
    let overrides = BTreeMap::from([("a".into(), Quantity::new(5.0, Unit::Kilogram).unwrap())]);
    let state = mass_state(&items, &BTreeMap::new(), &overrides).unwrap();
    assert_eq!(state.total_mass_kg, 5.0);
    assert!(state.inertia_kg_m2.is_none());
}

#[test]
fn tank_fill_interpolates_cg_and_rejects_over_capacity() {
    let mut tank = item(5.0, [0.0; 3]);
    tank.kind = MassKind::Fuel;
    tank.capacity = Some(ace_model::Binding::Constant(
        Quantity::new(10.0, Unit::Kilogram).unwrap(),
    ));
    tank.fill_curve = serde_json::from_value(json!([
        {"fraction":0.0,"cg":[{"value":0,"unit":"m"},{"value":0,"unit":"m"},{"value":0,"unit":"m"}]},
        {"fraction":1.0,"cg":[{"value":4,"unit":"m"},{"value":2,"unit":"m"},{"value":6,"unit":"m"}]}
    ])).unwrap();
    let items = BTreeMap::from([("tank".into(), tank)]);
    let state = mass_state(&items, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(state.cg_m, [2.0, 1.0, 3.0]);
    let overload = BTreeMap::from([("tank".into(), Quantity::new(11.0, Unit::Kilogram).unwrap())]);
    assert!(mass_state(&items, &BTreeMap::new(), &overload).is_err());
}

#[test]
fn impossible_inertia_is_rejected() {
    let mut component = item(10.0, [0.0; 3]);
    component.inertia.as_mut().unwrap().xx =
        ace_model::Binding::Constant(Quantity::new(3.0, Unit::Inertia).unwrap());
    assert!(
        mass_state(
            &BTreeMap::from([("a".into(), component)]),
            &BTreeMap::new(),
            &BTreeMap::new()
        )
        .is_err()
    );
}

#[test]
fn invalid_loading_is_reported_instead_of_omitted() {
    let resources = AnalysisResources {
        mass_items: BTreeMap::from([("a".into(), item(10.0, [0.0; 3]))]),
        loading_cases: BTreeMap::from([(
            "negative".into(),
            serde_json::from_value(json!({"masses":{
            "a":{"value":-1,"unit":"kg"}}}))
            .unwrap(),
        )]),
        ..Default::default()
    };
    let report = evaluate_mass(&resources, &BTreeMap::new()).unwrap();
    assert!(report.loadings["negative"].state.is_none());
    assert!(report.loadings["negative"].failure.is_some());
}

#[test]
fn cg_limits_retain_the_loading_that_produced_each_extreme() {
    let resources = AnalysisResources {
        mass_items: BTreeMap::from([
            ("forward".into(), item(10.0, [0.0; 3])),
            ("aft".into(), item(10.0, [10.0, 0.0, 0.0])),
        ]),
        loading_cases: serde_json::from_value(json!({
            "aft_only":{"masses":{"forward":{"value":0,"unit":"kg"}}},
            "forward_only":{"masses":{"aft":{"value":0,"unit":"kg"}}}
        }))
        .unwrap(),
        ..Default::default()
    };
    let report = evaluate_mass(&resources, &BTreeMap::new()).unwrap();
    assert_eq!(report.current.cg_m[0], 5.0);
    assert_eq!(report.minimum_cg_x_m, 0.0);
    assert_eq!(report.maximum_cg_x_m, 10.0);
    assert_eq!(report.minimum_cg_x_loading.as_deref(), Some("forward_only"));
    assert_eq!(report.maximum_cg_x_loading.as_deref(), Some("aft_only"));
    let current = evaluate_mass(
        &AnalysisResources {
            loading_cases: BTreeMap::new(),
            ..resources
        },
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(current.minimum_cg_x_loading.is_none());
    assert!(current.maximum_cg_x_loading.is_none());
}

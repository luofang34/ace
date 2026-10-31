use super::*;
use ace_engine::Registry;
use ace_model::{AnalysisResources, CatalogEntry, Source};
use std::path::PathBuf;

fn yaml(path: &str, key: &str) -> serde_json::Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let text = std::fs::read_to_string(root.join(path)).unwrap();
    let value: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
    let json = serde_json::to_value(value).unwrap();
    key.split('.').fold(json, |value, part| value[part].clone())
}

fn entry(kind: &str, document: serde_json::Value) -> CatalogEntry {
    CatalogEntry {
        kind: kind.into(),
        version: 1,
        source: Source {
            kind: "test".into(),
            reference: "legacy example".into(),
            revision: None,
        },
        document,
    }
}

/// Vehicle-port and catalog form of a legacy example.
fn fixture(name: &str) -> (BTreeMap<String, Quantity>, AnalysisResources) {
    let scenario = crate::test_support::example_scenario(name).unwrap();
    let aircraft = &scenario.aircraft;
    let mut catalog = BTreeMap::from([
        (
            "polars".to_owned(),
            entry(
                crate::services::synthesis::catalog::POLARS,
                yaml(&format!("{name}/aircraft.yaml"), "aircraft.aerodynamics"),
            ),
        ),
        (
            "limits".to_owned(),
            entry(
                crate::services::synthesis::catalog::LIMITS,
                yaml(&format!("{name}/aircraft.yaml"), "aircraft.limits"),
            ),
        ),
    ]);
    let engine = yaml(&format!("{name}/profiles/engine.yaml"), "profile");
    let kind = engine["type"].as_str().unwrap().to_owned();
    catalog.insert("engine".into(), entry(&kind, engine));
    if aircraft.propulsion.propeller_profile.is_some() {
        let propeller = yaml(&format!("{name}/profiles/propeller.yaml"), "profile");
        catalog.insert(
            "propeller".into(),
            entry(crate::services::synthesis::catalog::PROPELLER, propeller),
        );
    }
    let field = &scenario.mission.field;
    let mass = &aircraft.mass;
    let values = [
        aircraft.wing.area_m2,
        aircraft.wing.span_m,
        aircraft.wing.sweep_quarter_chord_rad.to_degrees(),
        mass.maximum_takeoff_mass_kg,
        mass.operating_empty_mass_kg,
        mass.maximum_payload_mass_kg,
        mass.maximum_fuel_mass_kg,
        field
            .takeoff_mass_kg
            .unwrap_or(mass.maximum_takeoff_mass_kg),
        field
            .landing_mass_kg
            .unwrap_or(mass.maximum_takeoff_mass_kg),
        f64::from(aircraft.propulsion.engine_count),
        aircraft.propulsion.sizing_factor,
        field.elevation_m,
        field.obstacle_height_m,
        field.braking_coefficient,
        field.runway_slope,
        field.headwind_m_s,
    ];
    let inputs: BTreeMap<String, Quantity> = INPUTS
        .iter()
        .zip(values)
        .map(|((port, unit), value)| ((*port).to_owned(), Quantity::new(value, *unit).unwrap()))
        .collect();
    let resources = AnalysisResources {
        catalog,
        ..AnalysisResources::default()
    };
    (inputs, resources)
}

fn run(
    plugin: &dyn AnalysisPlugin,
    inputs: &BTreeMap<String, Quantity>,
    resources: AnalysisResources,
) -> Result<PluginOutput, SolverError> {
    let registry = Registry::default();
    let context = AnalysisContext {
        resources,
        values: inputs,
        registry: &registry,
    };
    plugin.evaluate(inputs, &context)
}

/// The envelope plugin on a legacy example.
fn evaluate(name: &str) -> PluginOutput {
    let (inputs, resources) = fixture(name);
    run(&PerformanceEnvelope::new(), &inputs, resources).unwrap()
}

/// The constraint plugin on a legacy example, with the scenario's own targets.
fn diagram(
    name: &str,
    targets: crate::models::constraints::ConstraintTargets,
) -> Result<PluginOutput, SolverError> {
    let (mut inputs, resources) = fixture(name);
    for (port, value, unit) in [
        (
            "design_speed",
            targets.design_speed_m_s,
            Unit::MeterPerSecond,
        ),
        (
            "landing_stall_speed_limit",
            targets.landing_stall_speed_m_s,
            Unit::MeterPerSecond,
        ),
        (
            "takeoff_distance_limit",
            targets.takeoff_distance_m,
            Unit::Meter,
        ),
        ("wing_loading_minimum", 300.0, Unit::Pascal),
        ("wing_loading_maximum", 9_000.0, Unit::Pascal),
    ] {
        inputs.insert(port.into(), Quantity::new(value, unit).unwrap());
    }
    run(&ConstraintDiagram::new(), &inputs, resources)
}

#[test]
fn vehicle_constraint_diagram_reproduces_the_scenario_diagram() {
    use crate::models::constraints::{ConstraintAnalyzer, ConstraintTargets};
    for name in ["c172", "b777"] {
        let scenario = crate::test_support::example_scenario(name).unwrap();
        let targets = ConstraintTargets::from_scenario(&scenario);
        let legacy = ConstraintAnalyzer::new(scenario)
            .analyze(300.0, 9_000.0, 80)
            .unwrap();
        let output = diagram(name, targets).unwrap();
        let curves = &output.details["diagram"]["constraints"];
        for (curve, expected) in &legacy.constraints {
            let actual = curves[curve].as_array().unwrap();
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.iter().zip(expected) {
                let actual = actual.as_f64().unwrap();
                assert!(
                    (actual - expected).abs() <= 1e-9 * expected.abs().max(1.0),
                    "{name} {curve}"
                );
            }
        }
        let stall = output.values["stall_wing_loading_limit"].value;
        assert!((stall - legacy.stall_wing_loading_limit_n_m2).abs() <= 1e-9 * stall);
        let selected = output.values["selected_wing_loading"].value;
        assert!((selected - legacy.selected_wing_loading_n_m2).abs() <= 1e-9 * selected);
        assert_eq!(
            output.details["diagram"]["active_controlling_constraint"],
            legacy.active_controlling_constraint.as_str()
        );
        let required = output.details["required_loading_at_selection"]
            .as_f64()
            .unwrap();
        let ratio = output.values["installed_to_required_loading"].value;
        assert!(
            (ratio * required - legacy.selected_loading).abs() <= 1e-9 * legacy.selected_loading
        );
    }
}

#[test]
fn constraint_diagram_takes_no_default_targets_or_inverted_ranges() {
    let targets = crate::models::constraints::ConstraintTargets {
        design_speed_m_s: 120.0,
        landing_stall_speed_m_s: 70.0,
        takeoff_distance_m: 3_000.0,
    };
    let (mut inputs, resources) = fixture("b777");
    inputs.insert(
        "landing_stall_speed_limit".into(),
        Quantity::new(70.0, Unit::MeterPerSecond).unwrap(),
    );
    inputs.insert(
        "takeoff_distance_limit".into(),
        Quantity::new(3_000.0, Unit::Meter).unwrap(),
    );
    inputs.insert(
        "wing_loading_minimum".into(),
        Quantity::new(300.0, Unit::Pascal).unwrap(),
    );
    inputs.insert(
        "wing_loading_maximum".into(),
        Quantity::new(9_000.0, Unit::Pascal).unwrap(),
    );
    let Err(SolverError::Failed(message)) = run(&ConstraintDiagram::new(), &inputs, resources)
    else {
        panic!("a missing design speed must not fall back to a default");
    };
    assert!(message.contains("design_speed"), "{message}");
    assert!(diagram("b777", targets).is_ok());
    let (mut inputs, resources) = fixture("b777");
    for (port, value, unit) in [
        ("design_speed", 120.0, Unit::MeterPerSecond),
        ("landing_stall_speed_limit", 70.0, Unit::MeterPerSecond),
        ("takeoff_distance_limit", 3_000.0, Unit::Meter),
        ("wing_loading_minimum", 9_000.0, Unit::Pascal),
        ("wing_loading_maximum", 300.0, Unit::Pascal),
    ] {
        inputs.insert(port.into(), Quantity::new(value, unit).unwrap());
    }
    assert!(run(&ConstraintDiagram::new(), &inputs, resources).is_err());
}

#[test]
fn vehicle_envelope_reproduces_the_reference_aircraft_performance() {
    for name in ["c172", "b777"] {
        let scenario = crate::test_support::example_scenario(name).unwrap();
        let legacy = PointAnalyzer::new(scenario).summary(None).unwrap();
        let output = evaluate(name);
        let expected = [
            ("stall_speed_clean", legacy.stall_speed_clean_m_s),
            ("stall_speed_landing", legacy.stall_speed_landing_m_s),
            ("best_glide_speed", legacy.best_glide_speed_m_s),
            ("maximum_level_speed", legacy.maximum_level_speed_m_s),
            ("maximum_lift_to_drag", legacy.maximum_lift_to_drag_ratio),
            ("service_ceiling", legacy.service_ceiling_m.unwrap()),
            ("absolute_ceiling", legacy.absolute_ceiling_m.unwrap()),
            (
                "takeoff_field_length",
                legacy.takeoff_field_length_m.unwrap(),
            ),
            (
                "landing_field_length",
                legacy.landing_field_length_m.unwrap(),
            ),
        ];
        for (port, value) in expected {
            let actual = output.values[port].value;
            assert!(
                (actual - value).abs() <= 1e-9 * value.abs().max(1.0),
                "{name} {port}: {actual} != {value}"
            );
        }
    }
}

#[test]
fn reference_bands_hold_on_the_vehicle_envelope() {
    let b777 = evaluate("b777");
    let ceiling_ft = b777.values["service_ceiling"].value / 0.3048;
    assert!((39_000.0..=45_000.0).contains(&ceiling_ft), "{ceiling_ft}");
    assert!((16.0..=22.0).contains(&b777.values["maximum_lift_to_drag"].value));
    let c172 = evaluate("c172");
    let stall_kt = c172.values["stall_speed_landing"].value / 0.514_444;
    assert!((35.0..=60.0).contains(&stall_kt), "{stall_kt}");
}

#[test]
fn missing_or_ambiguous_catalog_data_fails_before_analysis() {
    let registry = Registry::default();
    let inputs = BTreeMap::new();
    let context = AnalysisContext {
        resources: AnalysisResources::default(),
        values: &inputs,
        registry: &registry,
    };
    assert!(
        PerformanceEnvelope::new()
            .evaluate(&inputs, &context)
            .is_err()
    );
}

/// The operating-point plugin on a legacy example at one cruise condition.
fn operating_point(
    name: &str,
    condition: [f64; 3],
    stations: Option<[f64; 3]>,
) -> Result<PluginOutput, SolverError> {
    let (mut inputs, resources) = fixture(name);
    let ports = [
        ("altitude", Unit::Meter),
        ("true_airspeed", Unit::MeterPerSecond),
        ("mass", Unit::Kilogram),
    ];
    for ((port, unit), value) in ports.into_iter().zip(condition) {
        inputs.insert(port.into(), Quantity::new(value, unit).unwrap());
    }
    let plugin = match stations {
        Some(stations) => {
            for (port, value) in [
                "center_of_gravity",
                "wing_aerodynamic_center",
                "tail_aerodynamic_center",
            ]
            .into_iter()
            .zip(stations)
            {
                inputs.insert(port.into(), Quantity::new(value, Unit::Meter).unwrap());
            }
            OperatingPoint::trimmed()
        }
        None => OperatingPoint::new(),
    };
    run(&plugin, &inputs, resources)
}

const CRUISE: [(&str, [f64; 3]); 2] = [
    ("c172", [2_438.4, 59.16, 1_111.0]),
    ("b777", [10_668.0, 249.1, 300_000.0]),
];

#[test]
fn vehicle_operating_point_reproduces_the_scenario_point() {
    for (name, [altitude, speed, mass]) in CRUISE {
        let scenario = crate::test_support::example_scenario(name).unwrap();
        let legacy = PointAnalyzer::new(scenario)
            .point(altitude, speed, mass, "clean")
            .unwrap();
        let output = operating_point(name, [altitude, speed, mass], None).unwrap();
        for (port, expected) in [
            ("mach", legacy.mach),
            ("stall_speed", legacy.stall_speed_m_s),
            ("thrust_required", legacy.thrust_required_n),
            ("power_required", legacy.power_required_w),
            ("excess_power", legacy.excess_power_w),
            ("rate_of_climb", legacy.rate_of_climb_m_s),
            ("climb_gradient", legacy.climb_gradient),
            ("fuel_flow_required", legacy.fuel_flow_required_kg_s),
            ("fuel_flow_maximum_power", legacy.fuel_flow_max_power_kg_s),
        ] {
            let actual = output.values[port].value;
            assert!(
                (actual - expected).abs() <= 1e-9 * expected.abs().max(1.0),
                "{name} {port}: {actual} != {expected}"
            );
        }
        assert!(
            output.values["excess_power"].value > 0.0,
            "{name} cannot hold cruise"
        );
        assert!(!output.values.contains_key("trim_wing_lift_fraction"));
    }
}

#[test]
fn trimmed_operating_point_reproduces_the_scenario_cg_point() {
    let scenario = crate::test_support::example_scenario("b777").unwrap();
    let stations = crate::models::mass_properties::longitudinal_stations(&scenario).unwrap();
    let cg = stations.wing_aerodynamic_center_m - 1.0;
    let [altitude, speed, mass] = CRUISE[1].1;
    let legacy = PointAnalyzer::new(scenario)
        .point_at_cg(altitude, speed, mass, "clean", Some(cg))
        .unwrap();
    let output = operating_point(
        "b777",
        [altitude, speed, mass],
        Some([
            cg,
            stations.wing_aerodynamic_center_m,
            stations.tail_aerodynamic_center_m,
        ]),
    )
    .unwrap();
    let fraction = output.values["trim_wing_lift_fraction"].value;
    assert!((fraction - legacy.trim_wing_lift_fraction.unwrap()).abs() < 1e-12);
    assert!(
        fraction > 1.0,
        "a CG ahead of the wing loads the wing beyond the weight"
    );
    let induced = output.values["thrust_required"].value;
    assert!((induced - legacy.thrust_required_n).abs() <= 1e-9 * induced);
}

#[test]
fn operating_point_rejects_untrimmable_cg_and_speed_below_stall() {
    let [altitude, speed, mass] = CRUISE[1].1;
    assert!(operating_point("b777", [altitude, speed, mass], Some([70.0, 33.0, 64.0])).is_err());
    assert!(operating_point("b777", [altitude, speed, mass], Some([30.0, 33.0, 20.0])).is_err());
    let Err(SolverError::External { source, .. }) =
        operating_point("b777", [0.0, 40.0, mass], None)
    else {
        panic!("40 m/s at MTOM is below the clean stall speed");
    };
    assert!(
        source.to_string().to_lowercase().contains("stall"),
        "{source}"
    );
}

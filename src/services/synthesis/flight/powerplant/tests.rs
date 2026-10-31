use super::*;
use crate::models::atmosphere::Isa1976;
use crate::services::synthesis::flight::{
    FlightPhase, PhaseKind, integration, parameters::FlightModel, path, tests,
};
use ace_engine::{AnalysisPlugin, RunControl};

fn inputs(shaft_power: f64, lapse_exponent: f64) -> BTreeMap<String, Quantity> {
    use Unit::*;
    [
        ("area", 20.0, SquareMeter),
        ("span", 14.0, Meter),
        ("cd0", 0.03, Ratio),
        ("oswald", 0.8, Ratio),
        ("cl_max", 1.4, Ratio),
        ("shaft_power", shaft_power, Watt),
        ("propeller_efficiency", 0.8, Ratio),
        ("bsfc", 0.27, KilogramPerKilowattHour),
        ("power_lapse_exponent", lapse_exponent, Ratio),
        ("fuel_price", 1.5, DollarPerKilogram),
        ("maximum_step", 60.0, Second),
    ]
    .into_iter()
    .map(|(key, value, unit)| (key.into(), Quantity::new(value, unit).unwrap()))
    .collect()
}

fn state(entries: &[(&str, f64, Unit)]) -> ace_model::VehicleState {
    let mut state = tests::state(2_000.0);
    for (key, value, unit) in entries {
        state
            .conditions
            .insert((*key).into(), Quantity::new(*value, *unit).unwrap());
    }
    state
}

#[test]
fn propeller_level_flight_matches_closed_form_shaft_power_fuel_equation() {
    let model = FlightModel::new(&inputs(150_000.0, 1.0)).unwrap();
    let entry = state(&[
        ("speed", 50.0, Unit::MeterPerSecond),
        ("altitude", 3_000.0, Unit::Meter),
        ("duration", 3_600.0, Unit::Second),
    ]);
    let output = integration::evaluate(&model, &entry, &RunControl::default()).unwrap();
    // dm/dt = -(BSFC * V / eta) * (q S CD0 + (m g)^2 / (q S pi e AR)), solved analytically.
    let rho = Isa1976::new(0.0).evaluate(3_000.0).unwrap().density_kg_m3;
    let q_area = 0.5 * rho * 50.0_f64.powi(2) * 20.0;
    let c = 0.27 / 3.6e6 * 50.0 / 0.8;
    let a = c * q_area * 0.03;
    let b =
        c * 9.80665_f64.powi(2) / (q_area * std::f64::consts::PI * 0.8 * 14.0_f64.powi(2) / 20.0);
    let final_mass =
        (a / b).sqrt() * ((2_000.0 * (b / a).sqrt()).atan() - (a * b).sqrt() * 3_600.0).tan();
    let burn: f64 = output.increments.iter().map(|p| p.fuel_burn.value).sum();
    assert!((2_000.0 - burn - final_mass).abs() < 1e-9);
    assert!(
        output.details["powerplant"]
            .as_str()
            .unwrap()
            .contains("BSFC")
    );
}

#[test]
fn shaft_power_lapse_limits_thrust_available_at_altitude() {
    let point = |n| Powerplant::new(&inputs(260_000.0, n)).unwrap();
    let air = Isa1976::new(0.0).evaluate(5_000.0).unwrap();
    let sigma = air.density_kg_m3 / SEA_LEVEL_DENSITY_KG_M3;
    let at = Operating {
        altitude: 5_000.0,
        speed: 50.0,
        mach: 50.0 / air.speed_of_sound_m_s,
        mode: propulsion::OperatingMode::Cruise,
    };
    let thrust = |n| point(n).available_thrust(&air, &at).unwrap();
    assert!((thrust(0.0) - 0.8 * 260_000.0 / 50.0).abs() < 1e-9);
    assert!((thrust(1.0) - sigma * 0.8 * 260_000.0 / 50.0).abs() < 1e-9);
    let climb = state(&[
        ("speed", 50.0, Unit::MeterPerSecond),
        ("altitude", 0.0, Unit::Meter),
        ("target_altitude", 4_500.0, Unit::Meter),
        ("target_speed", 50.0, Unit::MeterPerSecond),
        ("path_distance", 40_000.0, Unit::Meter),
    ]);
    let normalized = FlightModel::new(&inputs(260_000.0, 0.0)).unwrap();
    path::evaluate(&normalized, &climb, &RunControl::default()).unwrap();
    let aspirated = FlightModel::new(&inputs(260_000.0, 1.0)).unwrap();
    let Err(SolverError::Failed(message)) =
        path::evaluate(&aspirated, &climb, &RunControl::default())
    else {
        panic!("a sigma-lapsed engine cannot hold the climb near 4500 m");
    };
    assert!(message.contains("available"));
}

#[test]
fn propeller_inputs_are_validated_and_variants_expose_power_ports() {
    for (key, value) in [
        ("propeller_efficiency", 1.2),
        ("power_lapse_exponent", -0.5),
    ] {
        let mut invalid = inputs(150_000.0, 1.0);
        invalid.insert(key.into(), Quantity::new(value, Unit::Ratio).unwrap());
        assert!(Powerplant::new(&invalid).is_err());
    }
    let phase = FlightPhase::with_kind(PhaseKind::EnergyPath)
        .propeller()
        .vspaero();
    let spec = phase.spec();
    assert_eq!(
        spec.identity.id,
        "aircraft.vspaero_propeller_energy_path_phase"
    );
    assert_eq!(spec.inputs["bsfc"].unit, Unit::KilogramPerKilowattHour);
    assert_eq!(spec.inputs["shaft_power"].unit, Unit::Watt);
    for removed in ["tsfc", "available_thrust", "oswald"] {
        assert!(!spec.inputs.contains_key(removed));
    }
    assert!(spec.inputs.contains_key("k_020"));
}

fn catalog_engine(example: &str) -> AnalysisResources {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(example)
        .join("profiles/engine.yaml");
    let text = std::fs::read_to_string(path).unwrap();
    let value: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
    let document = serde_json::to_value(value).unwrap()["profile"].clone();
    let entry = ace_model::CatalogEntry {
        kind: document["type"].as_str().unwrap().into(),
        version: 1,
        source: ace_model::Source {
            kind: "test".into(),
            reference: format!("examples/{example}"),
            revision: None,
        },
        document,
    };
    AnalysisResources {
        catalog: BTreeMap::from([("engine".to_owned(), entry)]),
        ..AnalysisResources::default()
    }
}

fn installation(count: f64, sizing: f64) -> BTreeMap<String, Quantity> {
    BTreeMap::from([
        (
            "engine_count".into(),
            Quantity::new(count, Unit::Ratio).unwrap(),
        ),
        (
            "sizing_factor".into(),
            Quantity::new(sizing, Unit::Ratio).unwrap(),
        ),
    ])
}

fn deck_point(altitude: f64, mach: f64) -> (AtmosphereState, Operating) {
    let air = Isa1976::new(0.0).evaluate(altitude).unwrap();
    let speed = mach * air.speed_of_sound_m_s;
    let point = Operating {
        altitude,
        speed,
        mach,
        mode: propulsion::OperatingMode::Cruise,
    };
    (air, point)
}

#[test]
fn catalog_deck_thrust_and_fuel_follow_the_tabulated_engine() {
    let deck = Powerplant::deck(&installation(2.0, 1.1), &catalog_engine("b777")).unwrap();
    // 35 000 ft and Mach 0.9 are grid nodes of the GE90-class table.
    let (air, point) = deck_point(35_000.0 * 0.3048, 0.9);
    let thrust = deck.available_thrust(&air, &point).unwrap();
    let expected = 161_361.853_992 * 2.0 * 1.1 * (1.0 - 0.03);
    assert!((thrust - expected).abs() < 1e-6 * expected);
    let flow = deck.fuel_flow(200_000.0, &point).unwrap();
    assert!((flow - 0.0372 * 200_000.0 / 3600.0).abs() < 1e-12);
    assert!(deck.basis().contains("deck"));
}

#[test]
fn catalog_deck_rejects_extrapolation_piston_engines_and_fractional_counts() {
    let deck = Powerplant::deck(&installation(2.0, 1.0), &catalog_engine("b777")).unwrap();
    for (altitude, mach) in [(15_000.0, 0.8), (10_000.0, 0.95)] {
        let (air, point) = deck_point(altitude, mach);
        let Err(SolverError::Failed(message)) = deck.available_thrust(&air, &point) else {
            panic!("deck point at {altitude} m, Mach {mach} must not extrapolate");
        };
        assert!(message.contains("outside its range"), "{message}");
    }
    assert!(Powerplant::deck(&installation(2.0, 1.0), &catalog_engine("c172")).is_err());
    assert!(Powerplant::deck(&installation(1.5, 1.0), &catalog_engine("b777")).is_err());
    assert!(Powerplant::deck(&installation(2.0, 1.0), &AnalysisResources::default()).is_err());
}

#[test]
fn deck_variants_replace_constant_thrust_ports_with_installation_ports() {
    for phase in [
        FlightPhase::with_kind(PhaseKind::RouteCruise).deck(),
        FlightPhase::with_kind(PhaseKind::RouteCruise)
            .deck()
            .vspaero(),
    ] {
        let spec = phase.spec();
        assert!(spec.identity.id.contains("deck_route_cruise_phase"));
        for removed in ["tsfc", "available_thrust", "shaft_power"] {
            assert!(!spec.inputs.contains_key(removed));
        }
        for port in ["engine_count", "sizing_factor"] {
            assert_eq!(spec.inputs[port].unit, Unit::Ratio);
            assert!(
                spec.domain
                    .bounds
                    .iter()
                    .any(|bound| bound.variable == ace_model::DomainVariable::Input(port.into()))
            );
        }
    }
}

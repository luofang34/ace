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
    let rho = 0.6 * SEA_LEVEL_DENSITY_KG_M3;
    assert!((point(0.0).available_thrust(rho, 50.0) - 0.8 * 260_000.0 / 50.0).abs() < 1e-9);
    assert!((point(1.0).available_thrust(rho, 50.0) - 0.6 * 0.8 * 260_000.0 / 50.0).abs() < 1e-9);
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
    assert_eq!(spec.inputs["bsfc"], Unit::KilogramPerKilowattHour);
    assert_eq!(spec.inputs["shaft_power"], Unit::Watt);
    for removed in ["tsfc", "available_thrust", "oswald"] {
        assert!(!spec.inputs.contains_key(removed));
    }
    assert!(spec.inputs.contains_key("k_020"));
}

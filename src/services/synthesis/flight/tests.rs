use super::*;
use crate::models::atmosphere::Isa1976;

pub(super) fn inputs() -> BTreeMap<String, Quantity> {
    use Unit::*;
    [
        ("area", 100.0, SquareMeter),
        ("span", 35.0, Meter),
        ("cd0", 0.02, Ratio),
        ("oswald", 0.8, Ratio),
        ("cl_max", 1.5, Ratio),
        ("tsfc", 0.000016, KilogramPerNewtonSecond),
        ("available_thrust", 100_000.0, Newton),
        ("fuel_price", 1.25, DollarPerKilogram),
        ("maximum_step", 60.0, Second),
    ]
    .into_iter()
    .map(|(key, value, unit)| (key.into(), Quantity::new(value, unit).unwrap()))
    .collect()
}

pub(super) fn state(mass: f64) -> VehicleState {
    VehicleState {
        time: Quantity::new(0.0, Unit::Second).unwrap(),
        distance: Quantity::new(0.0, Unit::Meter).unwrap(),
        cost: Quantity::new(0.0, Unit::Dollar).unwrap(),
        masses: BTreeMap::from([(
            "vehicle".into(),
            Quantity::new(mass, Unit::Kilogram).unwrap(),
        )]),
        conditions: [
            ("speed", 230.0, Unit::MeterPerSecond),
            ("altitude", 10_000.0, Unit::Meter),
            ("duration", 10_000.0, Unit::Second),
        ]
        .into_iter()
        .map(|(id, value, unit)| (id.into(), Quantity::new(value, unit).unwrap()))
        .collect(),
    }
}

#[test]
fn integrated_fuel_matches_closed_form_parabolic_polar_mass_equation() {
    let model = parameters::FlightModel::new(&inputs()).unwrap();
    let output =
        integration::evaluate(&model, &state(80_000.0), &ace_engine::RunControl::default())
            .unwrap();
    // Independent analytic solution of dm/dt = -(a + b*m^2), using the declared polar.
    let rho = Isa1976::new(0.0).evaluate(10_000.0).unwrap().density_kg_m3;
    let q = 0.5 * rho * 230.0_f64.powi(2);
    let a = 0.000016 * q * 100.0 * 0.02;
    let b = 0.000016 * 9.80665_f64.powi(2) / (q * std::f64::consts::PI * 0.8 * 35.0_f64.powi(2));
    let final_mass =
        (a / b).sqrt() * ((80_000.0 * (b / a).sqrt()).atan() - (a * b).sqrt() * 10_000.0).tan();
    let burn: f64 = output
        .increments
        .iter()
        .map(|point| point.fuel_burn.value)
        .sum();
    let cost: f64 = output.increments.iter().map(|point| point.cost.value).sum();
    assert!((80_000.0 - burn - final_mass).abs() < 1e-6);
    assert!((cost - burn * 1.25).abs() < 1e-8);
    assert!(output.approximate);
    assert_eq!(output.details["method"], "rk4_level_flight");
}

#[test]
fn actual_entry_mass_drives_drag_and_invalid_conditions_are_rejected() {
    let model = parameters::FlightModel::new(&inputs()).unwrap();
    let light = integration::evaluate(&model, &state(70_000.0), &ace_engine::RunControl::default())
        .unwrap();
    let heavy = integration::evaluate(&model, &state(80_000.0), &ace_engine::RunControl::default())
        .unwrap();
    assert!(heavy.increments[0].fuel_burn.value > light.increments[0].fuel_burn.value);
    let mut invalid = state(80_000.0);
    invalid.conditions.insert(
        "speed".into(),
        Quantity::new(400.0, Unit::MeterPerSecond).unwrap(),
    );
    assert!(integration::evaluate(&model, &invalid, &ace_engine::RunControl::default()).is_err());
}

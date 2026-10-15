use super::super::tests::{inputs, state};
use super::*;

fn entry(mass: f64, start: (f64, f64), end: (f64, f64), distance: f64) -> VehicleState {
    let mut state = state(mass);
    for (key, value, unit) in [
        ("altitude", start.0, Unit::Meter),
        ("speed", start.1, Unit::MeterPerSecond),
        ("target_altitude", end.0, Unit::Meter),
        ("target_speed", end.1, Unit::MeterPerSecond),
        ("path_distance", distance, Unit::Meter),
    ] {
        state
            .conditions
            .insert(key.into(), Quantity::new(value, unit).unwrap());
    }
    state
}

#[test]
fn climb_and_descent_match_energy_and_kinematic_reference() {
    let model = FlightModel::new(&inputs()).unwrap();
    for (start, end, distance) in [
        ((35.0, 125.0), (10_000.0, 230.0), 250_000.0),
        ((10_000.0, 230.0), (1500.0, 125.0), 300_000.0),
    ] {
        let state = entry(70_000.0, start, end, distance);
        let output = evaluate(&model, &state, &RunControl::default()).unwrap();
        let length = distance.hypot(end.0 - start.0);
        let duration = length * (end.1 / start.1).ln() / (end.1 - start.1);
        assert!(
            (output
                .increments
                .iter()
                .map(|p| p.duration.value)
                .sum::<f64>()
                - duration)
                .abs()
                < 1e-9
        );
        assert!(
            (output
                .increments
                .iter()
                .map(|p| p.distance.value)
                .sum::<f64>()
                - distance)
                .abs()
                < 1e-8
        );
        let final_point = output.increments.last().unwrap();
        assert!((final_point.conditions["altitude"].value - end.0).abs() < 1e-10);
        assert!((final_point.conditions["speed"].value - end.1).abs() < 1e-10);
        for point in &output.increments {
            let work = point.details["specific_excess_work_j_kg"].as_f64().unwrap();
            let energy = point.details["specific_energy_change_j_kg"]
                .as_f64()
                .unwrap();
            assert!((work - energy).abs() < 1e-8);
            assert!((point.cost.value - 1.25 * point.fuel_burn.value).abs() < 1e-9);
            assert!(point.duration.value <= model.maximum_step + 1e-9);
        }
    }
}

#[test]
fn climb_first_point_matches_independent_force_balance_and_mass_drives_burn() {
    let model = FlightModel::new(&inputs()).unwrap();
    let entry = entry(70_000.0, (35.0, 125.0), (10_000.0, 230.0), 250_000.0);
    let path = Path::new(&entry).unwrap();
    let rate = rate(&model, &path, 70_000.0, 0.0).unwrap();
    let gamma = (9965.0_f64 / 250_000.0).atan();
    let rho = Isa1976::new(0.0).evaluate(35.0).unwrap().density_kg_m3;
    let q = 0.5 * rho * 125.0_f64.powi(2);
    let lift = 70_000.0 * 9.80665 * gamma.cos();
    let drag = q * 100.0 * 0.02 + lift * lift / (q * std::f64::consts::PI * 0.8 * 35.0_f64.powi(2));
    let thrust =
        drag + 70_000.0 * (9.80665 * gamma.sin() + 125.0 * 105.0 / 250_000.0_f64.hypot(9965.0));
    assert!((rate.thrust - thrust).abs() < 1e-8);
    assert!((rate.burn_per_meter - 0.000016 * thrust / 125.0).abs() < 1e-12);
    let burn = |mass| {
        let mut entry = entry.clone();
        entry.masses.insert(
            "vehicle".into(),
            Quantity::new(mass, Unit::Kilogram).unwrap(),
        );
        evaluate(&model, &entry, &RunControl::default())
            .unwrap()
            .increments
            .iter()
            .map(|p| p.fuel_burn.value)
            .sum::<f64>()
    };
    assert!(burn(75_000.0) > burn(70_000.0));
}

#[test]
fn impossible_path_and_insufficient_thrust_are_rejected_without_clamping() {
    let model = FlightModel::new(&inputs()).unwrap();
    let steep = entry(70_000.0, (10_000.0, 230.0), (0.0, 125.0), 100_000.0);
    assert!(evaluate(&model, &steep, &RunControl::default()).is_err());
    let climb = entry(70_000.0, (35.0, 125.0), (10_000.0, 230.0), 250_000.0);
    let mut weak = inputs();
    weak.insert(
        "available_thrust".into(),
        Quantity::new(1000.0, Unit::Newton).unwrap(),
    );
    let model = FlightModel::new(&weak).unwrap();
    assert!(evaluate(&model, &climb, &RunControl::default()).is_err());
}

#[test]
fn fuel_integral_agrees_with_independent_fine_euler_reference() {
    let model = FlightModel::new(&inputs()).unwrap();
    let initial = entry(70_000.0, (35.0, 125.0), (10_000.0, 230.0), 250_000.0);
    let output = evaluate(&model, &initial, &RunControl::default()).unwrap();
    let length = 250_000.0_f64.hypot(9965.0);
    let gamma = (9965.0_f64 / 250_000.0).atan();
    let ds = length / 100_000.0;
    let mut mass = 70_000.0;
    for i in 0..100_000 {
        let f = f64::from(i) / 100_000.0;
        let speed = 125.0 + 105.0 * f;
        let rho = Isa1976::new(0.0)
            .evaluate(35.0 + 9965.0 * f)
            .unwrap()
            .density_kg_m3;
        let q = 0.5 * rho * speed * speed;
        let lift = mass * 9.80665 * gamma.cos();
        let drag =
            q * 100.0 * 0.02 + lift * lift / (q * std::f64::consts::PI * 0.8 * 35.0_f64.powi(2));
        let thrust = drag + mass * (9.80665 * gamma.sin() + speed * 105.0 / length);
        mass -= 0.000016 * thrust * ds / speed;
    }
    let burn = output
        .increments
        .iter()
        .map(|p| p.fuel_burn.value)
        .sum::<f64>();
    assert!((70_000.0 - mass - burn).abs() < 0.02);
}

#[test]
fn declared_idle_floor_burns_idle_fuel_and_reports_drag_device_demand() {
    let model = FlightModel::new(&inputs()).unwrap();
    let mut steep = entry(70_000.0, (10_000.0, 230.0), (0.0, 125.0), 100_000.0);
    assert!(evaluate(&model, &steep, &RunControl::default()).is_err());
    steep.conditions.insert(
        "idle_thrust".into(),
        Quantity::new(4_000.0, Unit::Newton).unwrap(),
    );
    let output = evaluate(&model, &steep, &RunControl::default()).unwrap();
    assert!(output.details["maximum_drag_device_n"].as_f64().unwrap() > 0.0);
    let mut floored = 0;
    for point in &output.increments {
        let required = point.details["required_thrust_n"].as_f64().unwrap();
        let engine = point.details["engine_thrust_n"].as_f64().unwrap();
        let device = point.details["drag_device_n"].as_f64().unwrap();
        assert!((engine - required.max(4_000.0)).abs() < 1e-9);
        assert!((device - (engine - required)).abs() < 1e-9);
        if required < 4_000.0 {
            floored += 1;
            // Fuel follows the idle setting, not the (negative) path requirement.
            let idle_burn = 0.000016 * 4_000.0 * point.duration.value;
            assert!((point.fuel_burn.value - idle_burn).abs() < 1e-6 * idle_burn);
        }
        // The path still closes its energy balance with the device drag included.
        let work = point.details["specific_excess_work_j_kg"].as_f64().unwrap();
        let energy = point.details["specific_energy_change_j_kg"]
            .as_f64()
            .unwrap();
        assert!((work - energy).abs() < 1e-8);
    }
    assert!(floored > 0);
    steep.conditions.insert(
        "idle_thrust".into(),
        Quantity::new(200_000.0, Unit::Newton).unwrap(),
    );
    let Err(SolverError::Failed(message)) = evaluate(&model, &steep, &RunControl::default()) else {
        panic!("an idle floor above available thrust must be rejected");
    };
    assert!(message.contains("engine thrust 200000 N"));
    assert!(!message.contains("below zero"));
}

#[test]
fn idle_floor_does_not_change_paths_that_need_more_than_idle() {
    let model = FlightModel::new(&inputs()).unwrap();
    let climb = entry(70_000.0, (35.0, 125.0), (10_000.0, 230.0), 250_000.0);
    let mut floored = climb.clone();
    floored.conditions.insert(
        "idle_thrust".into(),
        Quantity::new(4_000.0, Unit::Newton).unwrap(),
    );
    let burn = |state: &VehicleState| {
        evaluate(&model, state, &RunControl::default())
            .unwrap()
            .increments
            .iter()
            .map(|p| p.fuel_burn.value)
            .sum::<f64>()
    };
    assert_eq!(burn(&climb), burn(&floored));
    floored.conditions.insert(
        "idle_thrust".into(),
        Quantity::new(-1.0, Unit::Newton).unwrap(),
    );
    assert!(evaluate(&model, &floored, &RunControl::default()).is_err());
}

#[test]
fn path_angle_limit_rejects_short_paths_and_names_the_minimum_distance() {
    let model = FlightModel::new(&inputs()).unwrap();
    let minimum = 4_500.0 / 10.0_f64.to_radians().tan();
    let short = entry(9_000.0, (0.0, 60.0), (4_500.0, 60.0), minimum * 0.99);
    let Err(SolverError::Failed(message)) = evaluate(&model, &short, &RunControl::default()) else {
        panic!("a path steeper than 10 deg must be rejected");
    };
    assert!(message.contains(&format!("path_distance >= {minimum:.0} m")));
    let long = entry(9_000.0, (0.0, 60.0), (4_500.0, 60.0), minimum * 1.01);
    assert!(Path::new(&long).is_ok());
}

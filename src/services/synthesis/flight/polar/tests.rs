use super::*;
use crate::models::atmosphere::Isa1976;
use crate::services::synthesis::flight::{integration, tests};

fn inputs() -> BTreeMap<String, Quantity> {
    let mut inputs = tests::inputs();
    inputs.remove("oswald");
    for (key, value) in [
        ("k_020", 0.02),
        ("k_050", 0.03),
        ("k_080", 0.04),
        ("cl_limit", 1.0),
    ] {
        inputs.insert(key.into(), Quantity::new(value, Unit::Ratio).unwrap());
    }
    inputs
}

#[test]
fn interpolated_solver_drag_drives_mass_ode_without_extrapolation() {
    let model = model(&inputs()).unwrap();
    let mut state = tests::state(70_000.0);
    let air = Isa1976::new(0.0).evaluate(10_000.0).unwrap();
    let speed = 0.65 * air.speed_of_sound_m_s;
    state.conditions.insert(
        "speed".into(),
        Quantity::new(speed, Unit::MeterPerSecond).unwrap(),
    );
    let output = integration::evaluate(&model, &state, &ace_engine::RunControl::default()).unwrap();
    let q_area = 0.5 * air.density_kg_m3 * speed * speed * 100.0;
    let a = 0.000016 * q_area * 0.02;
    let b = 0.000016 * 0.035 * 9.80665_f64.powi(2) / q_area;
    let final_mass =
        (a / b).sqrt() * ((70_000.0 * (b / a).sqrt()).atan() - (a * b).sqrt() * 10_000.0).tan();
    let burn: f64 = output.increments.iter().map(|p| p.fuel_burn.value).sum();
    assert!((70_000.0 - burn - final_mass).abs() < 1e-6);
    assert_eq!(
        output.increments[0].details["aerodynamics"]["model"]["model_id"],
        "aero.polar_table"
    );
    state.conditions.insert(
        "speed".into(),
        Quantity::new(0.81 * air.speed_of_sound_m_s, Unit::MeterPerSecond).unwrap(),
    );
    assert!(integration::evaluate(&model, &state, &ace_engine::RunControl::default()).is_err());
}

#[test]
fn low_mach_holds_lowest_sampled_polar_and_reports_basis() {
    let model = model(&inputs()).unwrap();
    let air = Isa1976::new(0.0).evaluate(1_000.0).unwrap();
    for mach in [0.10, 0.19, 0.2] {
        let mut state = tests::state(3_000.0);
        state.conditions.insert(
            "altitude".into(),
            Quantity::new(1_000.0, Unit::Meter).unwrap(),
        );
        state.conditions.insert(
            "speed".into(),
            Quantity::new(mach * air.speed_of_sound_m_s, Unit::MeterPerSecond).unwrap(),
        );
        state.conditions.insert(
            "duration".into(),
            Quantity::new(60.0, Unit::Second).unwrap(),
        );
        let output =
            integration::evaluate(&model, &state, &ace_engine::RunControl::default()).unwrap();
        let aero = &output.increments[0].details["aerodynamics"];
        assert_eq!(aero["model"]["validity_status"], "valid");
        let cl = aero["lift_coefficient"].as_f64().unwrap();
        let induced = aero["induced_drag_coefficient"].as_f64().unwrap();
        // k_020 from the fixture; the lower sample is held, not extrapolated.
        assert!((induced - 0.02 * cl * cl).abs() < 1e-12);
        assert!(
            output.details["polar_basis"]
                .as_str()
                .unwrap()
                .contains("held at the lowest sampled Mach")
        );
    }
}

#[test]
fn sampled_cl_limit_is_enforced_independently_of_user_stall_limit() {
    let mut inputs = inputs();
    inputs.insert("cl_limit".into(), Quantity::new(0.1, Unit::Ratio).unwrap());
    let model = model(&inputs).unwrap();
    assert!(
        integration::evaluate(
            &model,
            &tests::state(70_000.0),
            &ace_engine::RunControl::default()
        )
        .is_err()
    );
}

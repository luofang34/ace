use crate::domain::aerodynamics::{PolarTable, TABLE_POLAR_MODEL_ID};
use crate::models::atmosphere::Isa1976;
use crate::test_support::example_scenario;

use super::{
    FlightCondition, coefficients_at_mach, evaluate, maximum_lift_to_drag_ratio, stall_speed_m_s,
};

#[test]
fn c172_stall_speed_is_in_calibration_band() {
    let scenario = example_scenario("c172");
    assert!(scenario.is_ok());
    if let Ok(resolved) = scenario {
        let atmosphere = Isa1976::new(0.0).evaluate(0.0);
        assert!(atmosphere.is_ok());
        if let Ok(state) = atmosphere {
            let speed = stall_speed_m_s(
                &resolved.aircraft,
                "clean",
                resolved.aircraft.mass.maximum_takeoff_mass_kg,
                state.density_kg_m3,
            );
            assert!(speed.is_ok());
            if let Ok(value) = speed {
                assert!((45.0..=60.0).contains(&(value / 0.514_444)));
            }
        }
    }
}

#[test]
fn induced_drag_behavior_yields_expected_glide_ratio() {
    let scenario = example_scenario("c172");
    if let Ok(resolved) = scenario {
        let ratio = maximum_lift_to_drag_ratio(&resolved.aircraft, "clean");
        assert!(ratio.is_ok());
        if let Ok(value) = ratio {
            assert!((7.0..=12.0).contains(&value));
        }
    }
}

#[test]
fn constant_table_reproduces_legacy_parabolic_polar() -> Result<(), Box<dyn std::error::Error>> {
    let legacy = example_scenario("c172")?;
    let atmosphere = Isa1976::new(0.0).evaluate(2_000.0)?;
    let condition = FlightCondition {
        density_kg_m3: atmosphere.density_kg_m3,
        speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
        true_airspeed_m_s: atmosphere.speed_of_sound_m_s * 0.5,
        mass_kg: 1_000.0,
    };
    let expected = evaluate(&legacy.aircraft, "clean", condition)?;
    let mut tabulated = legacy.clone();
    let clean = &mut tabulated.aircraft.aerodynamics.clean;
    clean.polar_table = Some(PolarTable {
        mach: vec![0.0, 0.9],
        cd0: vec![clean.cd0; 2],
        cl_max: vec![clean.cl_max; 2],
        oswald_efficiency: Some(vec![clean.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });
    let actual = evaluate(&tabulated.aircraft, "clean", condition)?;

    assert!((actual.drag_coefficient - expected.drag_coefficient).abs() < 1.0e-12);
    assert!((actual.drag_n - expected.drag_n).abs() < 1.0e-9);
    assert!((actual.lift_to_drag_ratio - expected.lift_to_drag_ratio).abs() < 1.0e-12);
    assert_eq!(actual.model.model_id, TABLE_POLAR_MODEL_ID);
    Ok(())
}

#[test]
fn table_extrapolation_has_one_structured_warning() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let clean = &mut scenario.aircraft.aerodynamics.clean;
    clean.polar_table = Some(PolarTable {
        mach: vec![0.2, 0.8],
        cd0: vec![0.03, 0.04],
        cl_max: vec![1.4, 1.0],
        oswald_efficiency: None,
        induced_drag_factor: Some(vec![0.05, 0.08]),
    });
    let atmosphere = Isa1976::new(0.0).evaluate(0.0)?;
    let at_mach = |mach| FlightCondition {
        density_kg_m3: atmosphere.density_kg_m3,
        speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
        true_airspeed_m_s: atmosphere.speed_of_sound_m_s * mach,
        mass_kg: 1_000.0,
    };

    assert!(
        evaluate(&scenario.aircraft, "clean", at_mach(0.5))?
            .warnings
            .is_empty()
    );
    let result = evaluate(&scenario.aircraft, "clean", at_mach(0.9))?;
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].code, "MODEL_EXTRAPOLATION");
    assert_eq!(
        result.warnings[0].path.as_deref(),
        Some("aircraft.aerodynamics.clean.polar_table.mach")
    );
    assert_eq!(result.warnings[0].context["basis"], "tabulated_data");
    assert_eq!(result.model.validity_status, "extrapolated");
    Ok(())
}

#[test]
fn sr71_table_has_physical_subsonic_and_supersonic_glide_bands()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("sr71")?;
    let lift_to_drag = |mach| -> Result<f64, Box<dyn std::error::Error>> {
        let polar = coefficients_at_mach(&scenario.aircraft, "clean", mach)?;
        Ok(1.0 / (2.0 * (polar.cd0 * polar.induced_drag_factor).sqrt()))
    };

    assert!((8.0..=10.0).contains(&lift_to_drag(0.5)?));
    assert!((5.0..=6.5).contains(&lift_to_drag(3.2)?));
    Ok(())
}

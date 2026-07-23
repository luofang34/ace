use crate::models::atmosphere::Isa1976;
use crate::test_support::example_scenario;

use super::{maximum_lift_to_drag_ratio, stall_speed_m_s};

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

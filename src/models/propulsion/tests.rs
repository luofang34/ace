use crate::models::atmosphere::Isa1976;
use crate::test_support::example_scenario;

use super::{OperatingMode, PropulsionQuery, evaluate};

#[test]
fn piston_power_and_turbofan_thrust_lapse_with_altitude() {
    for name in ["c172", "b777"] {
        let scenario = example_scenario(name);
        assert!(scenario.is_ok());
        if let Ok(resolved) = scenario {
            let sea_level = Isa1976::new(0.0).evaluate(0.0);
            let altitude = Isa1976::new(0.0).evaluate(8000.0);
            if let (Ok(low_atmosphere), Ok(high_atmosphere)) = (sea_level, altitude) {
                let query = |altitude_m| PropulsionQuery {
                    altitude_m,
                    true_airspeed_m_s: 100.0,
                    mach: 0.3,
                    throttle: 1.0,
                    mode: OperatingMode::Cruise,
                };
                let low = evaluate(&resolved, &low_atmosphere, query(0.0));
                let high = evaluate(&resolved, &high_atmosphere, query(8000.0));
                assert!(low.is_ok() && high.is_ok());
                if let (Ok(low_value), Ok(high_value)) = (low, high) {
                    let low_power = low_value.propulsive_power_available_w.unwrap_or(0.0);
                    let high_power = high_value.propulsive_power_available_w.unwrap_or(0.0);
                    assert!(high_power < low_power);
                }
            }
        }
    }
}

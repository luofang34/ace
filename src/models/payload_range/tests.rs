#![allow(clippy::expect_used, clippy::panic)]

use approx::assert_relative_eq;

use crate::models::atmosphere::Isa1976;
use crate::test_support::example_scenario;

use super::cruise_condition;

#[test]
fn representative_cruise_uses_propagated_altitude_and_converts_ias()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let climb_altitude_m = scenario
        .mission
        .segments
        .iter()
        .find(|segment| segment.id == "climb")
        .and_then(|segment| segment.target_altitude_m)
        .ok_or("missing climb target")?;
    let cruise = scenario
        .mission
        .segments
        .iter_mut()
        .find(|segment| segment.id == "cruise")
        .ok_or("missing cruise segment")?;
    cruise.altitude_m = None;
    cruise.true_airspeed_m_s = None;
    cruise.mach = None;
    cruise.indicated_airspeed_m_s = Some(50.0);

    let (altitude_m, true_airspeed_m_s) = cruise_condition(&scenario)?;
    let atmosphere = Isa1976::new(0.0).evaluate(climb_altitude_m)?;
    let expected_speed_m_s = 50.0 * (1.225 / atmosphere.density_kg_m3).sqrt();

    assert_relative_eq!(altitude_m, climb_altitude_m, epsilon = 1.0e-9);
    assert_relative_eq!(true_airspeed_m_s, expected_speed_m_s, epsilon = 1.0e-9);
    Ok(())
}

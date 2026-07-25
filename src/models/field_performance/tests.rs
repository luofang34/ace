use crate::domain::schema::EngineProfile;
use crate::test_support::example_scenario;

use super::estimate_takeoff_distance_m;

#[test]
fn turbofan_field_screen_uses_sizing_and_installation_loss()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let baseline = estimate_takeoff_distance_m(&scenario);

    scenario.aircraft.propulsion.sizing_factor = 1.5;
    let resized = estimate_takeoff_distance_m(&scenario);
    assert!(resized < baseline);

    let EngineProfile::Turbofan(profile) = &mut scenario.engine else {
        return Err("B777 requires a turbofan profile".into());
    };
    profile.thrust_loss_fraction = 0.20;
    let installation_loss = estimate_takeoff_distance_m(&scenario);
    assert!(installation_loss > resized);
    Ok(())
}

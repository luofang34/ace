use crate::test_support::example_scenario;

use super::installed_loading;

#[test]
fn turbofan_comparison_loading_includes_sizing_factor() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let baseline = installed_loading(&scenario);
    scenario.aircraft.propulsion.sizing_factor = 1.5;
    let resized = installed_loading(&scenario);
    assert!((resized - 1.5 * baseline).abs() < 1.0e-12);
    Ok(())
}

#[test]
fn piston_comparison_loading_includes_sizing_factor() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let baseline = installed_loading(&scenario);
    scenario.aircraft.propulsion.sizing_factor = 1.2;
    let resized = installed_loading(&scenario);
    assert!((resized - 1.2 * baseline).abs() < 1.0e-12);
    Ok(())
}

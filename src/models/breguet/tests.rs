use crate::domain::propulsion::{PropulsionMode, TableFuelSchedule};
use crate::domain::schema::EngineProfile;
use crate::test_support::example_scenario;

use super::estimate;

#[test]
fn specific_impulse_deck_names_its_actual_breguet_fuel_basis()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let EngineProfile::Turbofan(profile) = &mut scenario.engine else {
        return Err("B777 requires a turbofan profile".into());
    };
    let deck = profile
        .table_deck
        .as_mut()
        .ok_or("B777 requires a table deck")?;
    let rows = deck.altitude_axis_m.len();
    let columns = deck.mach_axis.len();
    let cruise = deck
        .modes
        .iter_mut()
        .find(|mode| mode.mode == PropulsionMode::Cruise)
        .ok_or("B777 requires a cruise mode")?;
    cruise.fuel = TableFuelSchedule::SpecificImpulseS(vec![vec![3_000.0; columns]; rows]);

    let result = estimate(&scenario, 17.0, 50_000.0)?;
    assert!(
        result
            .assumptions
            .iter()
            .any(|assumption| assumption.contains("specific impulse"))
    );
    assert!(
        result
            .assumptions
            .iter()
            .all(|assumption| !assumption.contains("TSFC"))
    );
    Ok(())
}

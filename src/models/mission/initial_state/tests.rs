use std::io;

use crate::domain::schema::{MissionInitialState, SegmentKind};
use crate::models::mission::MissionSimulator;
use crate::models::mission_power;
use crate::services::resolver::usable_initial_fuel_kg;
use crate::test_support::example_scenario;

fn state_with_fuel(fuel_mass_kg: Option<f64>, fuel_fraction: Option<f64>) -> MissionInitialState {
    MissionInitialState {
        altitude_m: None,
        indicated_airspeed_m_s: None,
        true_airspeed_m_s: None,
        mach: None,
        fuel_fraction,
        fuel_mass_kg,
    }
}

#[test]
fn omitted_state_preserves_ground_and_full_usable_fuel() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["c172", "b777"] {
        let scenario = example_scenario(name)?;
        assert!(scenario.mission.initial_state.is_none());
        let usable = usable_initial_fuel_kg(&scenario.aircraft, &scenario.mission);
        let expected_mass = scenario.aircraft.mass.operating_empty_mass_kg
            + scenario.mission.payload_mass_kg
            + usable;
        let mission = MissionSimulator::new(scenario).simulate()?;
        let first = mission
            .segments
            .first()
            .ok_or_else(|| io::Error::other("reference mission has no segment"))?;
        assert_eq!(first.start_altitude_m, 0.0);
        assert!((mission.initial_takeoff_mass_kg - expected_mass).abs() < 1.0e-8);
    }
    Ok(())
}

#[test]
fn fuel_mass_and_fraction_seed_the_same_mass_and_reserve() -> Result<(), Box<dyn std::error::Error>>
{
    let baseline = example_scenario("c172")?;
    let usable = usable_initial_fuel_kg(&baseline.aircraft, &baseline.mission);
    let mut by_mass = baseline.clone();
    by_mass.mission.initial_state = Some(state_with_fuel(Some(usable * 0.5), None));
    let mut by_fraction = baseline;
    by_fraction.mission.initial_state = Some(state_with_fuel(None, Some(0.5)));

    let mass_result = MissionSimulator::new(by_mass).simulate()?;
    let fraction_result = MissionSimulator::new(by_fraction).simulate()?;
    assert!(
        (mass_result.initial_takeoff_mass_kg - fraction_result.initial_takeoff_mass_kg).abs()
            < 1.0e-8
    );
    assert!(
        (mass_result.reserve_fuel_remaining_kg - fraction_result.reserve_fuel_remaining_kg).abs()
            < 1.0e-8
    );
    Ok(())
}

#[test]
fn initial_speed_is_inherited_until_a_segment_declares_one()
-> Result<(), Box<dyn std::error::Error>> {
    let mut inherited = example_scenario("c172")?;
    let mut cruise = inherited
        .mission
        .segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Cruise)
        .cloned()
        .ok_or_else(|| io::Error::other("C172 mission has no cruise"))?;
    cruise.distance_m = Some(1_200.0);
    cruise.true_airspeed_m_s = None;
    cruise.indicated_airspeed_m_s = None;
    cruise.mach = None;
    cruise.power_fraction = Some(0.0);
    inherited.mission.segments = vec![cruise.clone()];
    let mut state = state_with_fuel(None, Some(1.0));
    state.true_airspeed_m_s = Some(60.0);
    inherited.mission.initial_state = Some(state);

    let inherited_result = MissionSimulator::new(inherited.clone()).simulate()?;
    let inherited_cruise = inherited_result
        .segments
        .first()
        .ok_or_else(|| io::Error::other("inherited cruise result is missing"))?;
    assert!((inherited_cruise.duration_s - 20.0).abs() < 1.0e-8);

    cruise.true_airspeed_m_s = Some(40.0);
    inherited.mission.segments = vec![cruise];
    let explicit_result = MissionSimulator::new(inherited).simulate()?;
    let explicit_cruise = explicit_result
        .segments
        .first()
        .ok_or_else(|| io::Error::other("explicit cruise result is missing"))?;
    assert!((explicit_cruise.duration_s - 30.0).abs() < 1.0e-8);
    Ok(())
}

#[test]
fn zero_initial_fuel_is_a_valid_depleted_start() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.mission.initial_state = Some(state_with_fuel(None, Some(0.0)));
    let empty_mass =
        scenario.aircraft.mass.operating_empty_mass_kg + scenario.mission.payload_mass_kg;
    let mission = MissionSimulator::new(scenario).simulate()?;

    assert!((mission.initial_takeoff_mass_kg - empty_mass).abs() < 1.0e-8);
    assert!(mission.fuel_exhausted);
    Ok(())
}

#[test]
fn mission_power_uses_the_simulated_inherited_speed() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let cruise = scenario
        .mission
        .segments
        .iter_mut()
        .find(|segment| segment.kind == SegmentKind::Cruise)
        .ok_or_else(|| io::Error::other("C172 mission has no cruise"))?;
    cruise.distance_m = Some(100.0);
    cruise.true_airspeed_m_s = None;
    cruise.indicated_airspeed_m_s = None;
    cruise.mach = None;
    scenario.mission.segments = vec![cruise.clone()];
    let mut state = state_with_fuel(None, Some(1.0));
    state.true_airspeed_m_s = Some(60.0);
    scenario.mission.initial_state = Some(state);

    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let screen = mission_power::evaluate(&scenario, &mission)?;
    let point = screen
        .points
        .first()
        .ok_or_else(|| io::Error::other("mission-power point is missing"))?;
    assert!((point.true_airspeed.value - 60.0).abs() < 1.0e-8);
    Ok(())
}

#![allow(clippy::expect_used, clippy::panic)]

use crate::domain::schema::MissionInitialState;
use crate::models::mission::MissionSimulator;
use crate::test_support::{example_scenario, low_landing_fuel_sr71_scenario};

use super::evaluate;

#[test]
fn advisory_boundary_is_exclusive_and_requires_completion() {
    let below = evaluate(true, 4.999, 100.0);
    assert_eq!(
        below.warning.as_ref().map(|warning| warning.code.as_str()),
        Some("LOW_LANDING_FUEL")
    );
    assert!(below.value.is_some());

    let threshold = evaluate(true, 5.0, 100.0);
    assert!(threshold.warning.is_none());
    assert!(threshold.value.is_some());

    let incomplete = evaluate(false, 0.0, 100.0);
    assert!(incomplete.warning.is_none());
    assert!(incomplete.value.is_none());
}

#[test]
fn sr71_fixture_reports_low_landing_fuel() -> Result<(), Box<dyn std::error::Error>> {
    let mission = MissionSimulator::new(low_landing_fuel_sr71_scenario()?).simulate()?;

    assert!(
        mission.completed,
        "SR-71 failed at {:?} with {} kg remaining",
        mission.failed_segment, mission.reserve_fuel_remaining_kg
    );
    assert!(
        mission
            .landing_fuel
            .as_ref()
            .is_some_and(|fuel| fuel.value < 0.05 * 46_180.0)
    );
    assert!(mission.warnings.iter().any(|warning| {
        warning.code == "LOW_LANDING_FUEL"
            && warning.path.as_deref() == Some("mission.landing_fuel")
    }));
    Ok(())
}

#[test]
fn incomplete_mission_has_no_landing_metric_or_advisory() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("c172")?;
    scenario.mission.initial_state = Some(MissionInitialState {
        altitude_m: None,
        indicated_airspeed_m_s: None,
        true_airspeed_m_s: None,
        mach: None,
        fuel_fraction: None,
        fuel_mass_kg: Some(0.0),
    });
    let mission = MissionSimulator::new(scenario).simulate()?;

    assert!(!mission.completed);
    assert!(mission.landing_fuel.is_none());
    assert!(
        mission
            .warnings
            .iter()
            .all(|warning| warning.code != "LOW_LANDING_FUEL")
    );
    Ok(())
}

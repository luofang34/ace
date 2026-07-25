#![allow(clippy::expect_used, clippy::panic)]

use crate::test_support::example_scenario;

use super::super::MissionSimulator;

#[test]
fn nonpositive_excess_power_is_rejected_without_clamping() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("b777")?;
    let segment = scenario
        .mission
        .segments
        .iter_mut()
        .find(|segment| segment.energy_schedule.is_some())
        .ok_or("missing B777 energy climb")?;
    segment.thrust_fraction = Some(0.1);
    let segment_id = segment.id.clone();
    let error = MissionSimulator::new(scenario)
        .simulate()
        .expect_err("insufficient energy-climb power must fail");

    assert_eq!(error.detail().code, "NONPOSITIVE_EXCESS_POWER");
    assert_eq!(
        error.detail().path.as_deref(),
        Some(format!("mission.segments.{segment_id}.schedule.1").as_str())
    );
    Ok(())
}

#[test]
fn energy_climb_propagates_model_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    let mission = MissionSimulator::new(example_scenario("x15")?).simulate()?;
    let boost = mission
        .segments
        .iter()
        .find(|segment| segment.segment_id == "boost_climb")
        .ok_or("missing X-15 boost")?;

    assert!(boost.warnings.iter().any(|warning| {
        warning.code == "MODEL_EXTRAPOLATION"
            && warning.path.as_deref() == Some("mission.segments.boost_climb.condition.mach")
    }));
    Ok(())
}

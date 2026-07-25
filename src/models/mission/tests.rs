use std::io;

use crate::domain::result::MissionResult;
use crate::domain::schema::{MissionSegment, SegmentKind};
use crate::test_support::{example_scenario, fuel_exhaustion_scenario};

use super::{MissionSimulator, initial_fuel_load};

fn cruise_probe(
    scenario: &crate::domain::schema::ResolvedScenario,
    id: &str,
    altitude_m: f64,
    mach: Option<f64>,
) -> Result<MissionSegment, Box<dyn std::error::Error>> {
    let mut segment = scenario
        .mission
        .segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Cruise)
        .cloned()
        .ok_or_else(|| io::Error::other("fixture has no cruise segment"))?;
    segment.id = id.to_owned();
    segment.distance_m = Some(1_000.0);
    segment.altitude_m = Some(altitude_m);
    segment.mach = mach;
    segment.true_airspeed_m_s = mach.is_none().then_some(45.0);
    segment.indicated_airspeed_m_s = None;
    Ok(segment)
}

#[test]
fn mission_mass_is_continuous_and_non_increasing() {
    let scenario = example_scenario("c172");
    assert!(scenario.is_ok());
    if let Ok(resolved) = scenario {
        let result = MissionSimulator::new(resolved).simulate();
        assert!(result.is_ok());
        if let Ok(mission) = result {
            for pair in mission.segments.windows(2) {
                assert!((pair[0].end_mass_kg - pair[1].start_mass_kg).abs() < 1.0e-8);
                assert!(pair[0].end_mass_kg <= pair[0].start_mass_kg);
            }
        }
    }
}

#[test]
fn timed_altitude_controls_fuel_and_propagates_state() -> Result<(), Box<dyn std::error::Error>> {
    let sea_level = MissionSimulator::new(example_scenario("c172")?).simulate()?;
    let mut elevated_scenario = example_scenario("c172")?;
    let timed = elevated_scenario
        .mission
        .segments
        .first_mut()
        .ok_or_else(|| io::Error::other("C172 fixture has no timed segment"))?;
    timed.altitude_m = Some(2_438.4);
    let elevated = MissionSimulator::new(elevated_scenario).simulate()?;
    let sea_segment = sea_level
        .segments
        .first()
        .ok_or_else(|| io::Error::other("sea-level mission has no segment result"))?;
    let elevated_segment = elevated
        .segments
        .first()
        .ok_or_else(|| io::Error::other("elevated mission has no segment result"))?;
    let following = elevated
        .segments
        .get(1)
        .ok_or_else(|| io::Error::other("elevated mission has no following segment"))?;

    assert!((elevated_segment.end_altitude_m - 2_438.4).abs() < 1.0e-8);
    assert!((following.start_altitude_m - 2_438.4).abs() < 1.0e-8);
    assert!((elevated_segment.fuel_burn_kg - sea_segment.fuel_burn_kg).abs() > 1.0e-6);
    Ok(())
}

#[test]
fn x15_captive_carry_establishes_air_launch_altitude() -> Result<(), Box<dyn std::error::Error>> {
    let mission = MissionSimulator::new(example_scenario("x15")?).simulate()?;
    let captive = mission
        .segments
        .first()
        .ok_or_else(|| io::Error::other("X-15 mission has no captive-carry result"))?;
    let drop = mission
        .segments
        .get(1)
        .ok_or_else(|| io::Error::other("X-15 mission has no drop result"))?;

    assert!((captive.start_altitude_m - 13_716.0).abs() < 1.0e-8);
    assert!((captive.end_altitude_m - 13_716.0).abs() < 1.0e-8);
    assert!((drop.start_altitude_m - captive.end_altitude_m).abs() < 1.0e-8);
    assert!((captive.start_mass_kg - 15_370.0).abs() < 1.0e-8);
    Ok(())
}

#[test]
fn x15_engine_off_segments_burn_no_fuel_and_keep_kinematics()
-> Result<(), Box<dyn std::error::Error>> {
    let mission = MissionSimulator::new(example_scenario("x15")?).simulate()?;
    assert!(mission.completed);
    assert!(!mission.fuel_exhausted);

    for id in ["captive_carry", "glide_descent", "pattern_glide", "landing"] {
        let segment = mission
            .segments
            .iter()
            .find(|segment| segment.segment_id == id)
            .ok_or_else(|| io::Error::other(format!("missing engine-off segment {id}")))?;
        assert_eq!(segment.fuel_burn_kg, 0.0);
        assert!(segment.duration_s > 0.0);
    }
    for id in ["glide_descent", "pattern_glide"] {
        let segment = mission
            .segments
            .iter()
            .find(|segment| segment.segment_id == id)
            .ok_or_else(|| io::Error::other(format!("missing glide segment {id}")))?;
        assert!(segment.distance_m > 0.0);
        assert!(segment.end_altitude_m < segment.start_altitude_m);
    }
    for id in ["drop_and_light", "boost_climb", "speed_run"] {
        let segment = mission
            .segments
            .iter()
            .find(|segment| segment.segment_id == id)
            .ok_or_else(|| io::Error::other(format!("missing powered segment {id}")))?;
        assert!(segment.fuel_burn_kg > 0.0);
    }
    Ok(())
}

#[test]
fn engine_off_cruise_keeps_distance_without_fuel_burn() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let cruise = scenario
        .mission
        .segments
        .iter_mut()
        .find(|segment| segment.kind == SegmentKind::Cruise)
        .ok_or_else(|| io::Error::other("C172 fixture has no cruise segment"))?;
    cruise.power_fraction = Some(0.0);
    cruise.thrust_fraction = None;
    let cruise_id = cruise.id.clone();
    let mission = MissionSimulator::new(scenario).simulate()?;
    let result = mission
        .segments
        .iter()
        .find(|segment| segment.segment_id == cruise_id)
        .ok_or_else(|| io::Error::other("engine-off cruise result is missing"))?;

    assert_eq!(result.fuel_burn_kg, 0.0);
    assert!(result.distance_m > 0.0);
    assert!(result.duration_s > 0.0);
    Ok(())
}

#[test]
fn engine_off_climb_to_higher_altitude_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("x15")?;
    let climb = scenario
        .mission
        .segments
        .iter_mut()
        .find(|segment| segment.kind == SegmentKind::Climb)
        .ok_or_else(|| io::Error::other("X-15 fixture has no climb segment"))?;
    let climb_id = climb.id.clone();
    climb.thrust_fraction = Some(0.0);
    climb.power_fraction = None;
    let error = match MissionSimulator::new(scenario).simulate() {
        Err(error) => error,
        Ok(_) => return Err(io::Error::other("engine-off climb unexpectedly succeeded").into()),
    };
    let expected_path = format!("mission.segments.{climb_id}");

    assert_eq!(error.detail().code, "ENGINE_OFF_CLIMB_UNSUPPORTED");
    assert_eq!(error.detail().path.as_deref(), Some(expected_path.as_str()));
    Ok(())
}

#[test]
fn payload_drop_reduces_mass_without_burning_fuel() {
    let scenario = example_scenario("c172");
    assert!(scenario.is_ok());
    if let Ok(mut resolved) = scenario {
        let payload_drop = MissionSegment {
            id: "deliver_payload".to_owned(),
            kind: SegmentKind::PayloadDrop,
            duration_s: None,
            distance_m: None,
            target_altitude_m: None,
            altitude_m: None,
            indicated_airspeed_m_s: None,
            true_airspeed_m_s: None,
            mach: None,
            power_fraction: None,
            thrust_fraction: None,
            fuel_fraction: None,
            fuel_mass_kg: None,
            payload_mass_kg: Some(100.0),
        };
        resolved.mission.segments.insert(1, payload_drop);
        let expected_payload = resolved.mission.payload_mass_kg - 100.0;
        let result = MissionSimulator::new(resolved).simulate();
        assert!(result.is_ok());
        if let Ok(mission) = result {
            let drop = mission
                .segments
                .iter()
                .find(|segment| segment.segment_id == "deliver_payload");
            assert!(drop.is_some());
            if let Some(segment) = drop {
                assert!((segment.payload_removed_kg - 100.0).abs() < 1.0e-8);
                assert!(segment.fuel_burn_kg.abs() < 1.0e-8);
                assert!((segment.start_mass_kg - segment.end_mass_kg - 100.0).abs() < 1.0e-8);
            }
            assert!((mission.final_payload_mass_kg - expected_payload).abs() < 1.0e-6);
        }
    }
}

#[test]
fn in_flight_depletion_is_reported_as_fuel_exhaustion() -> Result<(), Box<dyn std::error::Error>> {
    let mission = MissionSimulator::new(fuel_exhaustion_scenario()?).simulate()?;

    assert!(!mission.completed);
    assert!(mission.fuel_exhausted);
    assert!(!mission.fuel_capacity_violation);
    assert_eq!(mission.failed_segment.as_deref(), Some("depletion_probe"));
    assert!(mission.warnings.iter().any(|warning| {
        warning.code == "FUEL_EXHAUSTED"
            && warning.path.as_deref() == Some("mission.segments.depletion_probe")
    }));
    assert!(
        !mission
            .warnings
            .iter()
            .any(|warning| warning.code == "FUEL_CAPACITY_EXCEEDED")
    );
    Ok(())
}

#[test]
fn initial_load_capacity_has_an_independent_diagnostic() {
    let full_tank = initial_fuel_load(100.0, 100.0);
    assert!(!full_tank.capacity_exceeded);
    assert!(full_tank.warning.is_none());

    let overfill = initial_fuel_load(101.0, 100.0);
    assert!(overfill.capacity_exceeded);
    assert_eq!(overfill.loaded_kg, 100.0);
    assert!(overfill.warning.as_ref().is_some_and(|warning| {
        warning.code == "FUEL_CAPACITY_EXCEEDED" && warning.code != "FUEL_EXHAUSTED"
    }));
}

#[test]
fn stored_mission_without_exhaustion_flag_defaults_to_false()
-> Result<(), Box<dyn std::error::Error>> {
    let mission = MissionSimulator::new(example_scenario("c172")?).simulate()?;
    let mut stored = serde_json::to_value(mission)?;
    let object = stored
        .as_object_mut()
        .ok_or_else(|| io::Error::other("mission did not serialize as an object"))?;
    assert!(object.remove("fuel_exhausted").is_some());

    let decoded: MissionResult = serde_json::from_value(stored)?;

    assert!(!decoded.fuel_exhausted);
    Ok(())
}

#[test]
fn cruise_model_diagnostics_are_scoped_and_deduplicated() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("sr71")?;
    scenario.mission.segments = vec![cruise_probe(
        &scenario,
        "strict_probe",
        19_507.2,
        Some(3.2),
    )?];

    let mission = MissionSimulator::new(scenario).simulate()?;
    let segment = mission
        .segments
        .first()
        .ok_or_else(|| io::Error::other("probe segment did not complete"))?;
    let extrapolations = segment
        .warnings
        .iter()
        .filter(|warning| warning.code == "MODEL_EXTRAPOLATION")
        .collect::<Vec<_>>();

    assert_eq!(extrapolations.len(), 1);
    assert_eq!(
        extrapolations[0].path.as_deref(),
        Some("mission.segments.strict_probe.condition.mach")
    );
    assert!(mission.warnings.contains(extrapolations[0]));
    Ok(())
}

#[test]
fn identical_diagnostics_remain_distinct_across_segments() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("sr71")?;
    scenario.mission.segments = vec![
        cruise_probe(&scenario, "high_speed_one", 19_507.2, Some(3.2))?,
        cruise_probe(&scenario, "high_speed_two", 19_507.2, Some(3.2))?,
    ];

    let mission = MissionSimulator::new(scenario).simulate()?;
    let paths = mission
        .warnings
        .iter()
        .filter(|warning| warning.code == "MODEL_EXTRAPOLATION")
        .filter_map(|warning| warning.path.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(
        paths,
        [
            "mission.segments.high_speed_one.condition.mach",
            "mission.segments.high_speed_two.condition.mach",
        ]
    );
    Ok(())
}

#[test]
fn cruise_retains_propulsion_deck_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.mission.segments = vec![cruise_probe(&scenario, "high_cruise", 5_800.0, None)?];

    let mission = MissionSimulator::new(scenario).simulate()?;
    let segment = mission
        .segments
        .first()
        .ok_or_else(|| io::Error::other("probe segment did not complete"))?;

    assert!(segment.warnings.iter().any(|warning| {
        warning.code == "MODEL_EXTRAPOLATION"
            && warning.path.as_deref()
                == Some("mission.segments.high_cruise.aircraft.propulsion.profile")
    }));
    Ok(())
}

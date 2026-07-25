#![allow(clippy::expect_used, clippy::panic)]

use crate::domain::schema::MissionDocument;
use crate::test_support::example_scenario;

use super::{validate_energy_schedule_limits, validate_energy_schedule_transitions};
use crate::services::resolver::resolve_mission;

fn resolve(
    yaml: &str,
) -> Result<crate::domain::schema::Mission, crate::domain::diagnostic::AexError> {
    let document: MissionDocument =
        serde_yaml::from_str(yaml).map_err(|source| crate::domain::diagnostic::AexError::Yaml {
            path: "test.mission".into(),
            source,
        })?;
    resolve_mission(document)
}

fn mission(schedule: &str) -> String {
    format!(
        "schema_version: 1\nmission:\n  id: energy_test\n  name: Energy test\n  payload:\n    mass: 10 kg\n  segments:\n    - id: boost\n      type: energy_climb\n      thrust_fraction: 0.8\n      schedule:\n{schedule}"
    )
}

#[test]
fn valid_schedule_resolves_and_matches_the_initial_altitude()
-> Result<(), Box<dyn std::error::Error>> {
    let resolved = resolve(&mission(
        "        - altitude: 0 ft\n          true_airspeed: 100 kt\n        - altitude: 1000 ft\n          mach: 0.3\n",
    ))?;
    let schedule = resolved.segments[0]
        .energy_schedule
        .as_deref()
        .ok_or("missing resolved schedule")?;

    assert_eq!(schedule.len(), 2);
    assert_eq!(schedule[0].altitude_m, 0.0);
    assert!(schedule[1].mach == Some(0.3));
    validate_energy_schedule_transitions(&resolved)?;
    Ok(())
}

#[test]
fn point_speed_is_required_and_exclusive() {
    for (schedule, code) in [
        (
            "        - altitude: 0 ft\n          true_airspeed: 100 kt\n        - altitude: 1000 ft\n",
            "ENERGY_SCHEDULE_FIELD_EXCLUSIVITY",
        ),
        (
            "        - altitude: 0 ft\n          true_airspeed: 100 kt\n          mach: 0.2\n        - altitude: 1000 ft\n          mach: 0.3\n",
            "ENERGY_SCHEDULE_FIELD_EXCLUSIVITY",
        ),
    ] {
        let error = resolve(&mission(schedule)).expect_err("invalid speed fields must fail");
        assert_eq!(error.detail().code, code);
    }
}

#[test]
fn nonmonotonic_and_mismatched_schedules_have_stable_paths() {
    let nonmonotonic = resolve(&mission(
        "        - altitude: 0 ft\n          true_airspeed: 100 kt\n        - altitude: 1000 ft\n          true_airspeed: 90 kt\n",
    ))
    .expect_err("decreasing schedule speed must fail");
    assert_eq!(nonmonotonic.detail().code, "NONMONOTONIC_ENERGY_SCHEDULE");
    assert_eq!(
        nonmonotonic.detail().path.as_deref(),
        Some("mission.segments.0.schedule.1")
    );

    let mismatch = resolve(&mission(
        "        - altitude: 100 ft\n          true_airspeed: 100 kt\n        - altitude: 1000 ft\n          true_airspeed: 110 kt\n",
    ))
    .expect_err("schedule must start at propagated altitude");
    assert_eq!(mismatch.detail().code, "ENERGY_SCHEDULE_START_MISMATCH");
    assert_eq!(
        mismatch.detail().path.as_deref(),
        Some("mission.segments.boost.schedule.0.altitude")
    );
}

#[test]
fn schedule_conditions_respect_aircraft_limits() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let point = scenario
        .mission
        .segments
        .iter_mut()
        .find_map(|segment| segment.energy_schedule.as_mut())
        .and_then(|schedule| schedule.last_mut())
        .ok_or("missing B777 energy schedule")?;
    point.mach = Some(0.95);
    point.indicated_airspeed_m_s = None;
    point.true_airspeed_m_s = None;
    let error = validate_energy_schedule_limits(&scenario.aircraft, &scenario.mission)
        .expect_err("schedule Mach above the aircraft limit must fail");

    assert_eq!(error.detail().code, "ENERGY_SCHEDULE_MACH_LIMIT_EXCEEDED");
    assert_eq!(
        error.detail().path.as_deref(),
        Some("mission.segments.climb_to_cruise.schedule.2.mach")
    );
    Ok(())
}

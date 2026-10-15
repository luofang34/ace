use crate::models::mission::MissionSimulator;
use crate::test_support::example_scenario;

#[test]
fn headwind_raises_cruise_fuel_per_ground_distance() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resolver = crate::services::resolver::ScenarioResolver::new(std::sync::Arc::new(
        crate::storage::profile_store::FileProfileStore,
    ));
    let resolve = |overrides: &[(&str, &str)]| {
        resolver.resolve_blocking(
            &root.join("examples/c172/scenario.yaml"),
            &overrides
                .iter()
                .map(|(path, value)| ((*path).to_owned(), (*value).to_owned()))
                .collect(),
        )
    };
    let cruise = |scenario| -> Result<(f64, f64), Box<dyn std::error::Error>> {
        let mission = MissionSimulator::new(scenario).simulate()?;
        let segment = mission
            .segments
            .iter()
            .find(|segment| segment.segment_id == "cruise")
            .ok_or("C172 mission has a cruise segment")?;
        Ok((
            segment.fuel_burn_kg / segment.distance_m,
            segment.duration_s,
        ))
    };
    let (calm, calm_time) = cruise(resolve(&[])?)?;
    let (windy, windy_time) = cruise(resolve(&[("mission.segments.cruise.headwind", "10 m/s")])?)?;
    assert!(windy > calm);
    assert!(windy_time > calm_time);
    let stopped = resolve(&[("mission.segments.cruise.headwind", "100 m/s")])?;
    assert!(MissionSimulator::new(stopped).simulate().is_err());
    let unsupported = resolve(&[("mission.segments.climb.headwind", "10 m/s")]);
    assert!(unsupported.is_err());
    Ok(())
}

#[test]
fn speed_schedules_select_polar_optima_and_save_fuel() -> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::schema::SpeedSchedule;
    use crate::models::aerodynamics::{minimum_drag_speed_m_s, minimum_power_speed_m_s};

    let declared = example_scenario("c172")?;
    let cruise = |schedule: Option<SpeedSchedule>| -> Result<_, Box<dyn std::error::Error>> {
        let mut scenario = declared.clone();
        let segment = scenario
            .mission
            .segments
            .iter_mut()
            .find(|segment| segment.id == "cruise")
            .ok_or("C172 mission has a cruise segment")?;
        if schedule.is_some() {
            segment.true_airspeed_m_s = None;
            segment.speed_schedule = schedule;
        }
        let altitude = segment.altitude_m.ok_or("C172 cruise declares altitude")?;
        let mission = MissionSimulator::new(scenario.clone()).simulate()?;
        let result = mission
            .segments
            .iter()
            .find(|segment| segment.segment_id == "cruise")
            .cloned()
            .ok_or("cruise result")?;
        Ok((scenario, altitude, result))
    };
    let (_, _, baseline) = cruise(None)?;
    let (scenario, altitude, range) = cruise(Some(SpeedSchedule::BestRange))?;
    let (_, _, endurance) = cruise(Some(SpeedSchedule::BestEndurance))?;
    let density = crate::models::atmosphere::Isa1976::new(0.0)
        .evaluate(altitude)?
        .density_kg_m3;
    let mass = range.start_mass_kg;
    let expected_range = minimum_drag_speed_m_s(&scenario.aircraft, "clean", mass, density)?;
    let expected_endurance = minimum_power_speed_m_s(&scenario.aircraft, "clean", mass, density)?;
    let speed = |result: &crate::domain::result::MissionSegmentResult| {
        result.distance_m / result.duration_s
    };
    let stall_floor = 1.2
        * crate::models::aerodynamics::stall_speed_m_s(&scenario.aircraft, "clean", mass, density)?;
    assert!((speed(&range) - expected_range.max(stall_floor)).abs() < 1e-9);
    assert!((speed(&endurance) - expected_endurance.max(stall_floor)).abs() < 1e-9);
    assert!(range.fuel_burn_kg < baseline.fuel_burn_kg);
    assert!(endurance.fuel_burn_kg / endurance.duration_s <= range.fuel_burn_kg / range.duration_s);
    Ok(())
}

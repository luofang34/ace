use crate::test_support::example_scenario;

use super::MissionSimulator;

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

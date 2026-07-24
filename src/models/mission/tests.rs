use crate::domain::schema::{MissionSegment, SegmentKind};
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

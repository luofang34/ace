use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::result::MissionPowerPoint;
use crate::domain::schema::SegmentKind;
use crate::models::mission::MissionSimulator;
use crate::test_support::example_scenario;

use super::evaluate;

fn point(kind: SegmentKind, throttle: f64) -> AexResult<MissionPowerPoint> {
    let mut scenario = example_scenario("c172")?;
    let source_kind = if kind == SegmentKind::Cruise {
        SegmentKind::Cruise
    } else {
        SegmentKind::Loiter
    };
    let segment = scenario
        .mission
        .segments
        .iter_mut()
        .find(|segment| segment.kind == source_kind)
        .ok_or_else(|| AexError::analysis("MISSING_TEST_SEGMENT", "example segment is missing"))?;
    segment.kind = kind;
    segment.power_fraction = Some(throttle);
    segment.thrust_fraction = None;
    let segment_id = segment.id.clone();
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    evaluate(&scenario, &mission)?
        .points
        .into_iter()
        .find(|candidate| candidate.segment_id == segment_id)
        .ok_or_else(|| AexError::analysis("MISSING_TEST_POINT", "mission-power point is missing"))
}

#[test]
fn advertised_in_flight_throttle_fields_change_power_feasibility() -> AexResult<()> {
    for kind in [
        SegmentKind::Cruise,
        SegmentKind::Loiter,
        SegmentKind::Reserve,
    ] {
        let low = point(kind, 0.2)?;
        let high = point(kind, 0.9)?;
        assert_eq!(low.throttle, 0.2);
        assert_eq!(high.throttle, 0.9);
        assert!(high.excess_power.value > low.excess_power.value);
    }
    Ok(())
}

use crate::domain::diagnostic::AexResult;
use crate::domain::schema::{EngineProfile, MissionSegment, ResolvedScenario, SegmentKind};
use crate::models::atmosphere::Isa1976;

pub(crate) fn representative_speed(
    segment: &MissionSegment,
    scenario: &ResolvedScenario,
    altitude_m: f64,
) -> AexResult<f64> {
    if let Some(speed) = segment.true_airspeed_m_s {
        return Ok(speed);
    }
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    if let Some(mach) = segment.mach {
        return Ok(mach * atmosphere.speed_of_sound_m_s);
    }
    if let Some(indicated) = segment.indicated_airspeed_m_s {
        return Ok(indicated * (1.225 / atmosphere.density_kg_m3).sqrt());
    }
    match scenario.engine {
        EngineProfile::Piston(_) => Ok(45.0),
        EngineProfile::Turbofan(_) => Ok(120.0),
    }
}

pub(crate) fn segment_operating_altitude(segment: &MissionSegment, current_altitude_m: f64) -> f64 {
    match segment.kind {
        SegmentKind::Climb => {
            0.5 * (current_altitude_m + segment.target_altitude_m.unwrap_or(current_altitude_m))
        }
        SegmentKind::Descent => {
            0.5 * (current_altitude_m + segment.target_altitude_m.unwrap_or(0.0))
        }
        SegmentKind::StartAndTaxi
        | SegmentKind::FixedTime
        | SegmentKind::Takeoff
        | SegmentKind::Cruise
        | SegmentKind::Loiter
        | SegmentKind::Reserve => segment.altitude_m.unwrap_or(current_altitude_m),
        _ => current_altitude_m,
    }
}

pub(crate) fn segment_end_altitude(segment: &MissionSegment, current_altitude_m: f64) -> f64 {
    match segment.kind {
        SegmentKind::Climb => segment.target_altitude_m.unwrap_or(current_altitude_m),
        SegmentKind::Descent => segment.target_altitude_m.unwrap_or(0.0),
        SegmentKind::StartAndTaxi
        | SegmentKind::FixedTime
        | SegmentKind::Takeoff
        | SegmentKind::Cruise
        | SegmentKind::Loiter
        | SegmentKind::Reserve => segment.altitude_m.unwrap_or(current_altitude_m),
        _ => current_altitude_m,
    }
}

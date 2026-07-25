use crate::domain::diagnostic::AexResult;
use crate::domain::schema::{
    EnergySchedulePoint, EngineProfile, MissionSegment, ResolvedScenario, SegmentKind,
};
use crate::models::atmosphere::Isa1976;

pub(crate) fn representative_speed(
    segment: &MissionSegment,
    scenario: &ResolvedScenario,
    altitude_m: f64,
) -> AexResult<f64> {
    representative_speed_with_fallback(segment, scenario, altitude_m, None)
}

pub(super) fn representative_speed_with_fallback(
    segment: &MissionSegment,
    scenario: &ResolvedScenario,
    altitude_m: f64,
    fallback_speed_m_s: Option<f64>,
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
    if let Some(speed) = fallback_speed_m_s {
        return Ok(speed);
    }
    match scenario.engine {
        EngineProfile::Piston(_) => Ok(45.0),
        EngineProfile::Turbofan(_) => Ok(120.0),
    }
}

pub(crate) fn energy_schedule_speed(point: &EnergySchedulePoint) -> AexResult<f64> {
    if let Some(speed) = point.true_airspeed_m_s {
        return Ok(speed);
    }
    let atmosphere = Isa1976::new(0.0).evaluate(point.altitude_m)?;
    if let Some(mach) = point.mach {
        return Ok(mach * atmosphere.speed_of_sound_m_s);
    }
    point
        .indicated_airspeed_m_s
        .map(|speed| speed * (1.225 / atmosphere.density_kg_m3).sqrt())
        .ok_or_else(|| {
            crate::domain::diagnostic::AexError::validation(
                "MISSING_ENERGY_SCHEDULE_SPEED",
                "mission.segments.schedule",
                "schedule point requires a speed",
            )
        })
}

pub(crate) fn segment_operating_altitude(segment: &MissionSegment, current_altitude_m: f64) -> f64 {
    match segment.kind {
        SegmentKind::Climb => {
            0.5 * (current_altitude_m + segment.target_altitude_m.unwrap_or(current_altitude_m))
        }
        SegmentKind::EnergyClimb => segment
            .energy_schedule
            .as_deref()
            .and_then(|schedule| schedule.first().zip(schedule.last()))
            .map_or(current_altitude_m, |(first, last)| {
                0.5 * (first.altitude_m + last.altitude_m)
            }),
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
        SegmentKind::EnergyClimb => segment
            .energy_schedule
            .as_deref()
            .and_then(|schedule| schedule.last())
            .map_or(current_altitude_m, |point| point.altitude_m),
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

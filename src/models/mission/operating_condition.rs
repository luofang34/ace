use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::schema::{
    EnergySchedulePoint, EngineProfile, MissionSegment, ResolvedScenario, SegmentKind,
    SpeedSchedule,
};
use crate::domain::warning::WarningCode;
use crate::models::aerodynamics::{
    minimum_drag_speed_m_s, minimum_power_speed_m_s, stall_speed_m_s,
};
use crate::models::atmosphere::{Isa1976, scenario_atmosphere};

/// Margin above clean stall that a scheduled speed never undercuts.
const SCHEDULE_STALL_MARGIN: f64 = 1.2;

/// Segment speed at entry mass: a declared `speed_schedule` picks the polar optimum,
/// otherwise the declared or carried-over speed applies. A diagnostic reports when
/// the stall margin, not the polar optimum, sets a scheduled speed.
pub(super) fn segment_speed(
    segment: &MissionSegment,
    scenario: &ResolvedScenario,
    altitude_m: f64,
    mass_kg: f64,
    fallback_speed_m_s: Option<f64>,
) -> AexResult<(f64, Option<Diagnostic>)> {
    let Some(schedule) = segment.speed_schedule else {
        let speed =
            representative_speed_with_fallback(segment, scenario, altitude_m, fallback_speed_m_s)?;
        return Ok((speed, None));
    };
    let aircraft = &scenario.aircraft;
    let density = scenario_atmosphere(scenario)
        .evaluate(altitude_m)?
        .density_kg_m3;
    let minimum_drag = minimum_drag_speed_m_s(aircraft, "clean", mass_kg, density)?;
    // Shaft engines burn per unit power, jets per unit thrust: range and endurance
    // optima shift accordingly (Breguet).
    let optimum = match (&scenario.engine, schedule) {
        (EngineProfile::Piston(_), SpeedSchedule::BestRange)
        | (EngineProfile::Turbofan(_), SpeedSchedule::BestEndurance) => minimum_drag,
        (EngineProfile::Piston(_), SpeedSchedule::BestEndurance) => {
            minimum_power_speed_m_s(aircraft, "clean", mass_kg, density)?
        }
        (EngineProfile::Turbofan(_), SpeedSchedule::BestRange) => 3.0_f64.powf(0.25) * minimum_drag,
    };
    let floor = SCHEDULE_STALL_MARGIN * stall_speed_m_s(aircraft, "clean", mass_kg, density)?;
    if optimum >= floor {
        return Ok((optimum, None));
    }
    Ok((
        floor,
        Some(Diagnostic::warning_with_context(
            WarningCode::SpeedScheduleStallMargin,
            format!(
                "{} speed {optimum:.1} m/s is below {SCHEDULE_STALL_MARGIN} V_stall; flying {floor:.1} m/s",
                segment.id
            ),
            // Mission records scope segment diagnostics under the segment path.
            "speed_schedule",
            serde_json::json!({"optimum_speed_m_s": optimum, "flown_speed_m_s": floor}),
        )),
    ))
}

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
    let atmosphere = scenario_atmosphere(scenario).evaluate(altitude_m)?;
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

/// True airspeed of a schedule point; indicated and Mach speeds convert through
/// `atmosphere`, so a declared ISA offset changes them.
pub(crate) fn energy_schedule_speed(
    point: &EnergySchedulePoint,
    atmosphere: &Isa1976,
) -> AexResult<f64> {
    if let Some(speed) = point.true_airspeed_m_s {
        return Ok(speed);
    }
    let atmosphere = atmosphere.evaluate(point.altitude_m)?;
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

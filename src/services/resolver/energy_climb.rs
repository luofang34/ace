//! Energy-climb schedule resolution and invariant validation.

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{Dimension, GRAVITY_M_S2, parse_quantity};
use crate::domain::schema::{
    Aircraft, EnergySchedulePoint, Mission, RawEnergySchedulePoint, SegmentKind,
};
use crate::models::atmosphere::Isa1976;
use crate::models::mission::{energy_schedule_speed, segment_end_altitude};

use super::positive;

const ALTITUDE_TOLERANCE_M: f64 = 1.0e-6;

pub(super) fn resolve_energy_schedule(
    raw: Option<Vec<RawEnergySchedulePoint>>,
    segment_path: &str,
) -> AexResult<Option<Vec<EnergySchedulePoint>>> {
    raw.map(|schedule| resolve_schedule(schedule, segment_path))
        .transpose()
}

pub(super) fn validate_energy_schedule_transitions(mission: &Mission) -> AexResult<()> {
    let mut altitude_m = mission
        .initial_state
        .as_ref()
        .and_then(|state| state.altitude_m)
        .unwrap_or(0.0);
    for segment in &mission.segments {
        if segment.kind == SegmentKind::EnergyClimb {
            let schedule = schedule(segment.energy_schedule.as_deref(), &segment.id)?;
            let first = schedule
                .first()
                .ok_or_else(|| schedule_too_short(&segment.id))?;
            if (first.altitude_m - altitude_m).abs() > ALTITUDE_TOLERANCE_M {
                return Err(AexError::validation(
                    "ENERGY_SCHEDULE_START_MISMATCH",
                    format!("mission.segments.{}.schedule.0.altitude", segment.id),
                    format!(
                        "schedule starts at {} m but propagated mission altitude is {altitude_m} m",
                        first.altitude_m
                    ),
                ));
            }
        }
        altitude_m = segment_end_altitude(segment, altitude_m);
    }
    Ok(())
}

pub(super) fn validate_energy_schedule_limits(
    aircraft: &Aircraft,
    mission: &Mission,
) -> AexResult<()> {
    let atmosphere = Isa1976::new(0.0);
    for segment in &mission.segments {
        let Some(schedule) = segment.energy_schedule.as_deref() else {
            continue;
        };
        for (index, point) in schedule.iter().enumerate() {
            let root = format!("mission.segments.{}.schedule.{index}", segment.id);
            validate_altitude_limit(aircraft, point.altitude_m, &root)?;
            let speed_m_s = energy_schedule_speed(point)?;
            let mach = speed_m_s / atmosphere.evaluate(point.altitude_m)?.speed_of_sound_m_s;
            validate_speed_limits(aircraft, point, speed_m_s, mach, &root)?;
        }
    }
    Ok(())
}

fn resolve_schedule(
    raw: Vec<RawEnergySchedulePoint>,
    segment_path: &str,
) -> AexResult<Vec<EnergySchedulePoint>> {
    if raw.len() < 2 {
        return Err(AexError::validation(
            "ENERGY_SCHEDULE_TOO_SHORT",
            format!("{segment_path}.schedule"),
            "energy_climb schedule requires at least two points",
        ));
    }
    let points = raw
        .into_iter()
        .enumerate()
        .map(|(index, point)| resolve_point(point, segment_path, index))
        .collect::<AexResult<Vec<_>>>()?;
    validate_monotonic(&points, segment_path)?;
    Ok(points)
}

fn resolve_point(
    raw: RawEnergySchedulePoint,
    segment_path: &str,
    index: usize,
) -> AexResult<EnergySchedulePoint> {
    let root = format!("{segment_path}.schedule.{index}");
    if let Some(field) = raw.additional_fields.keys().next() {
        return Err(AexError::validation(
            "UNSUPPORTED_ENERGY_SCHEDULE_FIELD",
            format!("{root}.{field}"),
            format!("{field} is not supported in an energy_climb schedule point"),
        ));
    }
    let altitude = raw.altitude.as_deref().ok_or_else(|| {
        AexError::validation(
            "MISSING_ENERGY_SCHEDULE_ALTITUDE",
            format!("{root}.altitude"),
            "energy_climb schedule point requires altitude",
        )
    })?;
    validate_speed_exclusivity(&raw, &root)?;
    Ok(EnergySchedulePoint {
        altitude_m: non_negative_quantity(
            altitude,
            Dimension::Length,
            &format!("{root}.altitude"),
        )?,
        indicated_airspeed_m_s: positive_quantity(
            raw.indicated_airspeed.as_deref(),
            Dimension::Speed,
            &format!("{root}.indicated_airspeed"),
        )?,
        true_airspeed_m_s: positive_quantity(
            raw.true_airspeed.as_deref(),
            Dimension::Speed,
            &format!("{root}.true_airspeed"),
        )?,
        mach: raw
            .mach
            .map(|value| positive(value, &format!("{root}.mach")))
            .transpose()?,
    })
}

fn validate_speed_exclusivity(raw: &RawEnergySchedulePoint, root: &str) -> AexResult<()> {
    let count = [
        raw.indicated_airspeed.is_some(),
        raw.true_airspeed.is_some(),
        raw.mach.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if count == 1 {
        Ok(())
    } else {
        Err(AexError::validation(
            "ENERGY_SCHEDULE_FIELD_EXCLUSIVITY",
            root,
            "schedule point requires exactly one speed representation",
        ))
    }
}

fn validate_monotonic(points: &[EnergySchedulePoint], segment_path: &str) -> AexResult<()> {
    for (index, pair) in points.windows(2).enumerate() {
        let [start, end] = pair else {
            return Err(AexError::validation(
                "ENERGY_SCHEDULE_TOO_SHORT",
                format!("{segment_path}.schedule"),
                "energy_climb schedule requires at least two points",
            ));
        };
        let start_speed = energy_schedule_speed(start)?;
        let end_speed = energy_schedule_speed(end)?;
        let delta_energy = GRAVITY_M_S2 * (end.altitude_m - start.altitude_m)
            + 0.5 * (end_speed.powi(2) - start_speed.powi(2));
        if end.altitude_m <= start.altitude_m || end_speed < start_speed || delta_energy <= 0.0 {
            return Err(AexError::validation(
                "NONMONOTONIC_ENERGY_SCHEDULE",
                format!("{segment_path}.schedule.{}", index + 1),
                "schedule altitude must increase, speed must not decrease, and specific energy must increase",
            ));
        }
    }
    Ok(())
}

fn validate_altitude_limit(aircraft: &Aircraft, altitude_m: f64, root: &str) -> AexResult<()> {
    if aircraft
        .limits
        .maximum_operating_altitude_m
        .is_none_or(|limit| altitude_m <= limit)
    {
        return Ok(());
    }
    Err(AexError::validation(
        "ENERGY_SCHEDULE_ALTITUDE_LIMIT_EXCEEDED",
        format!("{root}.altitude"),
        "energy_climb schedule altitude exceeds the aircraft operating limit",
    ))
}

fn validate_speed_limits(
    aircraft: &Aircraft,
    point: &EnergySchedulePoint,
    speed_m_s: f64,
    mach: f64,
    root: &str,
) -> AexResult<()> {
    if aircraft
        .limits
        .maximum_operating_speed_m_s
        .is_some_and(|limit| speed_m_s > limit)
    {
        return Err(AexError::validation(
            "ENERGY_SCHEDULE_SPEED_LIMIT_EXCEEDED",
            format!("{root}.{}", speed_field(point)),
            "energy_climb schedule speed exceeds the aircraft operating limit",
        ));
    }
    if aircraft
        .limits
        .maximum_operating_mach
        .is_some_and(|limit| mach > limit)
    {
        return Err(AexError::validation(
            "ENERGY_SCHEDULE_MACH_LIMIT_EXCEEDED",
            format!("{root}.{}", speed_field(point)),
            "energy_climb schedule Mach exceeds the aircraft operating limit",
        ));
    }
    Ok(())
}

fn speed_field(point: &EnergySchedulePoint) -> &'static str {
    if point.true_airspeed_m_s.is_some() {
        "true_airspeed"
    } else if point.mach.is_some() {
        "mach"
    } else {
        "indicated_airspeed"
    }
}

fn positive_quantity(
    raw: Option<&str>,
    dimension: Dimension,
    path: &str,
) -> AexResult<Option<f64>> {
    raw.map(|value| parse_quantity(value, dimension))
        .transpose()?
        .map(|value| positive(value, path))
        .transpose()
}

fn non_negative_quantity(raw: &str, dimension: Dimension, path: &str) -> AexResult<f64> {
    let value = parse_quantity(raw, dimension)?;
    if value >= 0.0 && value.is_finite() {
        Ok(value)
    } else {
        Err(AexError::validation(
            "NEGATIVE_VALUE",
            path,
            "value must be non-negative and finite",
        ))
    }
}

fn schedule<'a>(
    value: Option<&'a [EnergySchedulePoint]>,
    segment_id: &str,
) -> AexResult<&'a [EnergySchedulePoint]> {
    value.ok_or_else(|| {
        AexError::validation(
            "MISSING_ENERGY_SCHEDULE",
            format!("mission.segments.{segment_id}.schedule"),
            "energy_climb requires schedule",
        )
    })
}

fn schedule_too_short(segment_id: &str) -> AexError {
    AexError::validation(
        "ENERGY_SCHEDULE_TOO_SHORT",
        format!("mission.segments.{segment_id}.schedule"),
        "energy_climb schedule requires at least two points",
    )
}

#[cfg(test)]
mod tests;

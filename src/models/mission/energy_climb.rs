//! Deterministic specific-energy climb and acceleration integration.

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::GRAVITY_M_S2;
use crate::domain::schema::{EnergySchedulePoint, MissionSegment};
use crate::models::performance::PointAnalyzer;
use crate::models::propulsion::OperatingMode;

use super::{
    MissionSimulator, MissionState, SegmentComputation, energy_schedule_speed,
    extend_unique_diagnostics, fuel, segment_engine_off,
};

const STEPS_PER_LEG: u32 = 16;

#[derive(Debug, Default)]
struct Integration {
    fuel_kg: f64,
    distance_m: f64,
    duration_s: f64,
    speed_time_m: f64,
    warnings: Vec<Diagnostic>,
}

pub(super) fn compute(
    simulator: &MissionSimulator,
    segment: &MissionSegment,
    state: MissionState,
) -> AexResult<SegmentComputation> {
    if segment_engine_off(segment) {
        return Err(AexError::validation(
            "ENGINE_OFF_CLIMB_UNSUPPORTED",
            format!("mission.segments.{}", segment.id),
            "engine-off energy climb cannot increase altitude or speed",
        ));
    }
    let schedule = segment.energy_schedule.as_deref().ok_or_else(|| {
        AexError::validation(
            "MISSING_ENERGY_SCHEDULE",
            format!("mission.segments.{}.schedule", segment.id),
            "energy_climb requires schedule",
        )
    })?;
    if schedule.len() < 2 {
        return Err(AexError::validation(
            "ENERGY_SCHEDULE_TOO_SHORT",
            format!("mission.segments.{}.schedule", segment.id),
            "energy_climb schedule requires at least two points",
        ));
    }
    validate_start(schedule, segment, state)?;
    let throttle = segment
        .power_fraction
        .or(segment.thrust_fraction)
        .ok_or_else(|| {
            AexError::validation(
                "MISSING_SEGMENT_FIELD",
                format!("mission.segments.{}", segment.id),
                "energy_climb requires one throttle setting",
            )
        })?;
    let analyzer = PointAnalyzer::new(simulator.scenario.clone());
    let mut integration = Integration::default();
    for (leg_index, pair) in schedule.windows(2).enumerate() {
        integrate_leg(
            simulator,
            &analyzer,
            segment,
            pair,
            leg_index,
            throttle,
            state.mass_kg,
            &mut integration,
        )?;
    }
    let last = schedule.last().ok_or_else(|| {
        AexError::validation(
            "ENERGY_SCHEDULE_TOO_SHORT",
            format!("mission.segments.{}.schedule", segment.id),
            "energy_climb schedule requires at least two points",
        )
    })?;
    let end_speed = energy_schedule_speed(last)?;
    Ok(SegmentComputation {
        fuel_burn_kg: integration.fuel_kg,
        payload_removed_kg: 0.0,
        distance_m: integration.distance_m,
        duration_s: integration.duration_s,
        end_altitude_m: last.altitude_m,
        end_speed_m_s: Some(end_speed),
        operating_speed_m_s: (integration.duration_s > 0.0)
            .then_some(integration.speed_time_m / integration.duration_s),
        warnings: integration.warnings,
    })
}

fn validate_start(
    schedule: &[EnergySchedulePoint],
    segment: &MissionSegment,
    state: MissionState,
) -> AexResult<()> {
    let first = schedule.first().ok_or_else(|| {
        AexError::validation(
            "ENERGY_SCHEDULE_TOO_SHORT",
            format!("mission.segments.{}.schedule", segment.id),
            "energy_climb schedule requires at least two points",
        )
    })?;
    if (first.altitude_m - state.altitude_m).abs() <= 1.0e-6 {
        return Ok(());
    }
    Err(AexError::validation(
        "ENERGY_SCHEDULE_START_MISMATCH",
        format!("mission.segments.{}.schedule.0.altitude", segment.id),
        format!(
            "schedule starts at {} m but mission state altitude is {} m",
            first.altitude_m, state.altitude_m
        ),
    ))
}

#[allow(clippy::too_many_arguments)]
fn integrate_leg(
    simulator: &MissionSimulator,
    analyzer: &PointAnalyzer,
    segment: &MissionSegment,
    pair: &[EnergySchedulePoint],
    leg_index: usize,
    throttle: f64,
    start_mass_kg: f64,
    integration: &mut Integration,
) -> AexResult<()> {
    let [start, end] = pair else {
        return Err(AexError::analysis(
            "INVALID_ENERGY_SCHEDULE_LEG",
            "energy-climb integration requires two points per leg",
        ));
    };
    let start_speed = energy_schedule_speed(start)?;
    let end_speed = energy_schedule_speed(end)?;
    for step in 0..STEPS_PER_LEG {
        let lower = f64::from(step) / f64::from(STEPS_PER_LEG);
        let upper = f64::from(step + 1) / f64::from(STEPS_PER_LEG);
        let start_altitude = interpolate(start.altitude_m, end.altitude_m, lower);
        let end_altitude = interpolate(start.altitude_m, end.altitude_m, upper);
        let step_start_speed = interpolate(start_speed, end_speed, lower);
        let step_end_speed = interpolate(start_speed, end_speed, upper);
        integrate_step(
            simulator,
            analyzer,
            segment,
            leg_index,
            throttle,
            start_mass_kg,
            start_altitude,
            end_altitude,
            step_start_speed,
            step_end_speed,
            integration,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn integrate_step(
    simulator: &MissionSimulator,
    analyzer: &PointAnalyzer,
    segment: &MissionSegment,
    leg_index: usize,
    throttle: f64,
    start_mass_kg: f64,
    start_altitude_m: f64,
    end_altitude_m: f64,
    start_speed_m_s: f64,
    end_speed_m_s: f64,
    integration: &mut Integration,
) -> AexResult<()> {
    let altitude_m = 0.5 * (start_altitude_m + end_altitude_m);
    let speed_m_s = 0.5 * (start_speed_m_s + end_speed_m_s);
    let mass_kg = start_mass_kg - integration.fuel_kg;
    let (excess_power_w, warnings) = analyzer.excess_power_with_diagnostics(
        altitude_m,
        speed_m_s,
        mass_kg,
        OperatingMode::Climb,
        throttle,
    )?;
    if excess_power_w <= 0.0 || !excess_power_w.is_finite() {
        return Err(AexError::validation(
            "NONPOSITIVE_EXCESS_POWER",
            format!("mission.segments.{}.schedule.{}", segment.id, leg_index + 1),
            format!("energy_climb has {excess_power_w} W excess power at the step midpoint"),
        ));
    }
    let specific_energy_j_kg = GRAVITY_M_S2 * (end_altitude_m - start_altitude_m)
        + 0.5 * (end_speed_m_s.powi(2) - start_speed_m_s.powi(2));
    let duration_s = mass_kg * specific_energy_j_kg / excess_power_w;
    let flow = fuel::available(
        simulator,
        altitude_m,
        speed_m_s,
        throttle,
        OperatingMode::Climb,
    )?;
    extend_unique_diagnostics(&mut integration.warnings, warnings);
    extend_unique_diagnostics(&mut integration.warnings, flow.warnings);
    let burn_kg = flow.flow_kg_s * duration_s;
    integration.fuel_kg += burn_kg;
    integration.duration_s += duration_s;
    integration.distance_m += 0.75 * speed_m_s * duration_s;
    integration.speed_time_m += speed_m_s * duration_s;
    Ok(())
}

fn interpolate(start: f64, end: f64, fraction: f64) -> f64 {
    start + fraction * (end - start)
}

#[cfg(test)]
mod tests;

//! Mission declarations and inherited operating-state traversal.

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::{MissionSegment, ResolvedScenario, SegmentKind};
use crate::domain::validity::ValidityVariable;
use crate::models::mission::{segment_end_altitude, segment_operating_altitude};
use crate::models::validity::ModelDomainRole;
use crate::services::resolver::usable_initial_fuel_kg;

use super::speed::{EffectiveSpeed, effective_segment_speed, effective_speed};
use super::{Declaration, push_effective_speed, push_scopes};

#[derive(Debug, Clone, Copy)]
struct MissionTraversalState {
    altitude_m: f64,
    speed_m_s: Option<f64>,
    tracks_speed: bool,
}

pub(super) fn mission_declarations(
    scenario: &ResolvedScenario,
    values: &mut Vec<Declaration>,
) -> AexResult<()> {
    push_scopes(
        values,
        &[ModelDomainRole::Aerodynamics, ModelDomainRole::Structure],
        ValidityVariable::Mass,
        "mission.payload.mass",
        scenario.mission.payload_mass_kg,
    );
    let mut state = push_initial_state_declarations(scenario, values)?;
    for segment in &scenario.mission.segments {
        let root = format!("mission.segments.{}", segment.id);
        push_segment_altitude(values, &root, "altitude", segment.altitude_m);
        push_segment_altitude(values, &root, "target_altitude", segment.target_altitude_m);
        state.speed_m_s = push_mission_segment_speed(values, &root, segment, scenario, state)?;
        push_segment_mass(values, &root, "fuel_mass", segment.fuel_mass_kg);
        push_segment_mass(values, &root, "payload_mass", segment.payload_mass_kg);
        state.altitude_m = segment_end_altitude(segment, state.altitude_m);
    }
    Ok(())
}

fn push_initial_state_declarations(
    scenario: &ResolvedScenario,
    values: &mut Vec<Declaration>,
) -> AexResult<MissionTraversalState> {
    let Some(state) = &scenario.mission.initial_state else {
        return Ok(MissionTraversalState {
            altitude_m: 0.0,
            speed_m_s: None,
            tracks_speed: false,
        });
    };
    let root = "mission.initial_state";
    push_segment_altitude(values, root, "altitude", state.altitude_m);
    let altitude_m = state.altitude_m.unwrap_or(0.0);
    let speed = effective_speed(
        altitude_m,
        state.true_airspeed_m_s,
        state.mach,
        state.indicated_airspeed_m_s,
    )?;
    validate_initial_speed_limits(scenario, speed)?;
    push_effective_speed(values, root, speed);
    let usable_fuel = usable_initial_fuel_kg(&scenario.aircraft, &scenario.mission);
    let initial_fuel = state
        .fuel_mass_kg
        .or_else(|| state.fuel_fraction.map(|fraction| fraction * usable_fuel))
        .unwrap_or(usable_fuel);
    push_segment_mass(
        values,
        root,
        "mass",
        Some(
            scenario.aircraft.mass.operating_empty_mass_kg
                + scenario.mission.payload_mass_kg
                + initial_fuel,
        ),
    );
    Ok(MissionTraversalState {
        altitude_m,
        speed_m_s: speed.and_then(|value| value.true_airspeed_m_s),
        tracks_speed: speed.is_some(),
    })
}

fn validate_initial_speed_limits(
    scenario: &ResolvedScenario,
    speed: Option<EffectiveSpeed>,
) -> AexResult<()> {
    let Some(speed) = speed else {
        return Ok(());
    };
    let path = format!("mission.initial_state.{}", speed.source_field);
    if let (Some(value), Some(limit)) = (
        speed.true_airspeed_m_s,
        scenario.aircraft.limits.maximum_operating_speed_m_s,
    ) && value > limit
    {
        return Err(AexError::validation(
            "INITIAL_SPEED_LIMIT_EXCEEDED",
            &path,
            format!("initial true airspeed {value} m/s exceeds aircraft limit {limit} m/s"),
        ));
    }
    if let (Some(value), Some(limit)) =
        (speed.mach, scenario.aircraft.limits.maximum_operating_mach)
        && value > limit
    {
        return Err(AexError::validation(
            "INITIAL_MACH_LIMIT_EXCEEDED",
            path,
            format!("initial Mach {value} exceeds aircraft limit {limit}"),
        ));
    }
    Ok(())
}

fn push_mission_segment_speed(
    values: &mut Vec<Declaration>,
    root: &str,
    segment: &MissionSegment,
    scenario: &ResolvedScenario,
    state: MissionTraversalState,
) -> AexResult<Option<f64>> {
    if matches!(
        segment.kind,
        SegmentKind::FixedFuel | SegmentKind::PayloadDrop
    ) {
        return Ok(state.speed_m_s);
    }
    let altitude_m = segment_operating_altitude(segment, state.altitude_m);
    if let Some(speed) = effective_segment_speed(segment, scenario, altitude_m)? {
        push_effective_speed(values, root, Some(speed));
        return Ok(state
            .tracks_speed
            .then_some(speed.true_airspeed_m_s)
            .flatten());
    }
    let Some(speed_m_s) = state.speed_m_s else {
        return Ok(None);
    };
    let inherited = effective_speed(altitude_m, Some(speed_m_s), None, None)?;
    push_inherited_speed(values, root, inherited);
    Ok(Some(speed_m_s))
}

fn push_inherited_speed(values: &mut Vec<Declaration>, root: &str, speed: Option<EffectiveSpeed>) {
    let Some(speed) = speed else {
        return;
    };
    let path = format!("{root}.inherited_speed");
    if let Some(value) = speed.true_airspeed_m_s {
        push_scopes(
            values,
            &[ModelDomainRole::Aerodynamics],
            ValidityVariable::TrueAirspeed,
            &path,
            value,
        );
    }
    if let Some(value) = speed.mach {
        push_scopes(
            values,
            &[ModelDomainRole::Aerodynamics, ModelDomainRole::Propulsion],
            ValidityVariable::Mach,
            &path,
            value,
        );
    }
}

fn push_segment_altitude(
    values: &mut Vec<Declaration>,
    root: &str,
    field: &str,
    value: Option<f64>,
) {
    push_segment_value(
        values,
        root,
        field,
        value,
        ValidityVariable::Altitude,
        &[ModelDomainRole::Atmosphere, ModelDomainRole::Propulsion],
    );
}

fn push_segment_mass(values: &mut Vec<Declaration>, root: &str, field: &str, value: Option<f64>) {
    push_segment_value(
        values,
        root,
        field,
        value,
        ValidityVariable::Mass,
        &[ModelDomainRole::Aerodynamics, ModelDomainRole::Structure],
    );
}

fn push_segment_value(
    values: &mut Vec<Declaration>,
    root: &str,
    field: &str,
    value: Option<f64>,
    variable: ValidityVariable,
    scopes: &[ModelDomainRole],
) {
    if let Some(value) = value {
        push_scopes(values, scopes, variable, &format!("{root}.{field}"), value);
    }
}

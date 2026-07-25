//! Mission initial-state resolution and cross-document validation.

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::Dimension;
use crate::domain::schema::{Aircraft, Mission, MissionInitialState, RawMissionInitialState};

use super::{non_negative, optional_quantity, positive};

pub(super) fn resolve_initial_state(
    raw: Option<RawMissionInitialState>,
) -> AexResult<Option<MissionInitialState>> {
    raw.map(resolve).transpose()
}

pub(super) fn validate_initial_state(aircraft: &Aircraft, mission: &Mission) -> AexResult<()> {
    let Some(state) = &mission.initial_state else {
        return Ok(());
    };
    validate_altitude_limit(aircraft, state)?;
    validate_declared_speed_limits(aircraft, state)?;
    validate_fuel_load(aircraft, mission, state)
}

fn resolve(raw: RawMissionInitialState) -> AexResult<MissionInitialState> {
    if let Some(field) = raw.additional_fields.keys().next() {
        return Err(AexError::validation(
            "UNSUPPORTED_INITIAL_STATE_FIELD",
            format!("mission.initial_state.{field}"),
            format!("{field} is not supported in mission initial_state"),
        ));
    }
    validate_exclusivity(&raw)?;
    Ok(MissionInitialState {
        altitude_m: resolve_non_negative_quantity(
            raw.altitude.as_deref(),
            Dimension::Length,
            "mission.initial_state.altitude",
        )?,
        indicated_airspeed_m_s: resolve_positive_quantity(
            raw.indicated_airspeed.as_deref(),
            Dimension::Speed,
            "mission.initial_state.indicated_airspeed",
        )?,
        true_airspeed_m_s: resolve_positive_quantity(
            raw.true_airspeed.as_deref(),
            Dimension::Speed,
            "mission.initial_state.true_airspeed",
        )?,
        mach: raw
            .mach
            .map(|value| positive(value, "mission.initial_state.mach"))
            .transpose()?,
        fuel_fraction: raw.fuel_fraction.map(resolve_fuel_fraction).transpose()?,
        fuel_mass_kg: resolve_non_negative_quantity(
            raw.fuel_mass.as_deref(),
            Dimension::Mass,
            "mission.initial_state.fuel_mass",
        )?,
    })
}

fn validate_exclusivity(raw: &RawMissionInitialState) -> AexResult<()> {
    let speed_count = [
        raw.indicated_airspeed.is_some(),
        raw.true_airspeed.is_some(),
        raw.mach.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    let fuel_count = [raw.fuel_fraction.is_some(), raw.fuel_mass.is_some()]
        .into_iter()
        .filter(|present| *present)
        .count();
    if speed_count > 1 {
        return Err(exclusivity_error("speed"));
    }
    if fuel_count > 1 {
        return Err(exclusivity_error("fuel"));
    }
    Ok(())
}

fn exclusivity_error(group: &str) -> AexError {
    AexError::validation(
        "INITIAL_STATE_FIELD_EXCLUSIVITY",
        "mission.initial_state",
        format!("initial_state accepts at most one {group} representation"),
    )
}

fn resolve_non_negative_quantity(
    raw: Option<&str>,
    dimension: Dimension,
    path: &str,
) -> AexResult<Option<f64>> {
    optional_quantity(raw, dimension)?
        .map(|value| non_negative(value, path))
        .transpose()
}

fn resolve_positive_quantity(
    raw: Option<&str>,
    dimension: Dimension,
    path: &str,
) -> AexResult<Option<f64>> {
    optional_quantity(raw, dimension)?
        .map(|value| positive(value, path))
        .transpose()
}

fn resolve_fuel_fraction(value: f64) -> AexResult<f64> {
    if (0.0..=1.0).contains(&value) && value.is_finite() {
        Ok(value)
    } else {
        Err(AexError::validation(
            "INVALID_INITIAL_FUEL_FRACTION",
            "mission.initial_state.fuel_fraction",
            "initial fuel fraction must be finite and within [0, 1]",
        ))
    }
}

fn validate_altitude_limit(aircraft: &Aircraft, state: &MissionInitialState) -> AexResult<()> {
    let (Some(altitude), Some(limit)) = (
        state.altitude_m,
        aircraft.limits.maximum_operating_altitude_m,
    ) else {
        return Ok(());
    };
    if altitude <= limit {
        return Ok(());
    }
    Err(AexError::validation(
        "INITIAL_ALTITUDE_LIMIT_EXCEEDED",
        "mission.initial_state.altitude",
        format!("initial altitude {altitude} m exceeds aircraft limit {limit} m"),
    ))
}

fn validate_declared_speed_limits(
    aircraft: &Aircraft,
    state: &MissionInitialState,
) -> AexResult<()> {
    if let (Some(speed), Some(limit)) = (
        state.true_airspeed_m_s,
        aircraft.limits.maximum_operating_speed_m_s,
    ) && speed > limit
    {
        return Err(AexError::validation(
            "INITIAL_SPEED_LIMIT_EXCEEDED",
            "mission.initial_state.true_airspeed",
            format!("initial true airspeed {speed} m/s exceeds aircraft limit {limit} m/s"),
        ));
    }
    if let (Some(mach), Some(limit)) = (state.mach, aircraft.limits.maximum_operating_mach)
        && mach > limit
    {
        return Err(AexError::validation(
            "INITIAL_MACH_LIMIT_EXCEEDED",
            "mission.initial_state.mach",
            format!("initial Mach {mach} exceeds aircraft limit {limit}"),
        ));
    }
    Ok(())
}

fn validate_fuel_load(
    aircraft: &Aircraft,
    mission: &Mission,
    state: &MissionInitialState,
) -> AexResult<()> {
    let usable = usable_initial_fuel_kg(aircraft, mission);
    let requested = state
        .fuel_mass_kg
        .or_else(|| state.fuel_fraction.map(|fraction| fraction * usable))
        .unwrap_or(usable);
    if requested > usable + 1.0e-8 {
        return Err(AexError::validation(
            "INITIAL_FUEL_EXCEEDS_USABLE",
            "mission.initial_state.fuel_mass",
            format!("initial fuel {requested} kg exceeds usable load {usable} kg"),
        ));
    }
    let mass = aircraft.mass.operating_empty_mass_kg + mission.payload_mass_kg + requested;
    if mass > aircraft.mass.maximum_takeoff_mass_kg + 1.0e-8 {
        return Err(AexError::validation(
            "INITIAL_MASS_LIMIT_EXCEEDED",
            "mission.initial_state",
            format!(
                "initial mass {mass} kg exceeds aircraft limit {} kg",
                aircraft.mass.maximum_takeoff_mass_kg
            ),
        ));
    }
    Ok(())
}

pub(crate) fn usable_initial_fuel_kg(aircraft: &Aircraft, mission: &Mission) -> f64 {
    let structural = (aircraft.mass.maximum_takeoff_mass_kg
        - aircraft.mass.operating_empty_mass_kg
        - mission.payload_mass_kg)
        .max(0.0);
    aircraft.mass.maximum_fuel_mass_kg.min(structural)
}

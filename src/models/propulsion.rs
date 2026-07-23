use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::result::{AtmosphereState, ModelMetadata, PropulsionState};
use crate::domain::schema::{
    EngineProfile, PistonProfile, PropellerProfile, ResolvedScenario, TurbofanProfile,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum OperatingMode {
    Takeoff,
    Climb,
    Cruise,
    Economy,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PropulsionQuery {
    pub(crate) altitude_m: f64,
    pub(crate) true_airspeed_m_s: f64,
    pub(crate) mach: f64,
    pub(crate) throttle: f64,
    pub(crate) mode: OperatingMode,
}

pub(crate) fn evaluate(
    scenario: &ResolvedScenario,
    atmosphere: &AtmosphereState,
    query: PropulsionQuery,
) -> AexResult<PropulsionState> {
    if !(0.0..=1.0).contains(&query.throttle) {
        return Err(AexError::validation(
            "INVALID_THROTTLE",
            "condition.throttle",
            "throttle must be between zero and one",
        ));
    }
    match &scenario.engine {
        EngineProfile::Piston(profile) => evaluate_piston(
            profile,
            scenario.propeller.as_ref(),
            scenario.aircraft.propulsion.engine_count,
            atmosphere,
            query,
        ),
        EngineProfile::Turbofan(profile) => evaluate_turbofan(
            profile,
            scenario.aircraft.propulsion.engine_count,
            atmosphere,
            query,
        ),
    }
}

fn evaluate_piston(
    profile: &PistonProfile,
    propeller: Option<&PropellerProfile>,
    engine_count: u32,
    atmosphere: &AtmosphereState,
    query: PropulsionQuery,
) -> AexResult<PropulsionState> {
    let propeller = propeller.ok_or_else(|| {
        AexError::validation(
            "MISSING_PROPELLER_PROFILE",
            "aircraft.propulsion.propeller_profile",
            "piston-propeller model requires a propeller profile",
        )
    })?;
    let sea_level_density = 1.225;
    let density_ratio = (atmosphere.density_kg_m3 / sea_level_density).max(0.0);
    let lapse = density_ratio
        .powf(profile.lapse_exponent)
        .max(profile.minimum_power_fraction);
    let shaft_power = profile.rated_power_w * f64::from(engine_count) * lapse * query.throttle;
    let efficiency = propeller_efficiency(propeller, query.true_airspeed_m_s, query.mode);
    let propulsive_power = shaft_power * efficiency;
    let static_reference_speed = 15.0;
    let thrust = propulsive_power / query.true_airspeed_m_s.max(static_reference_speed);
    let bsfc = match query.mode {
        OperatingMode::Takeoff | OperatingMode::Climb => profile.bsfc_takeoff_kg_kwh,
        OperatingMode::Cruise => profile.bsfc_cruise_kg_kwh,
        OperatingMode::Economy => profile.bsfc_economy_kg_kwh,
    };
    let fuel_flow = bsfc * shaft_power / 1000.0 / 3600.0;
    let mut warnings = Vec::new();
    let validity = if profile
        .maximum_altitude_m
        .is_some_and(|maximum| query.altitude_m > maximum)
    {
        warnings.push(Diagnostic::warning(
            "MODEL_EXTRAPOLATION",
            "Piston-engine profile evaluated above its maximum altitude.",
            "aircraft.propulsion.profile",
        ));
        "extrapolated"
    } else {
        "valid"
    };
    Ok(PropulsionState {
        thrust_available_n: Some(thrust),
        shaft_power_available_w: Some(shaft_power),
        propulsive_power_available_w: Some(propulsive_power),
        fuel_flow_kg_s: fuel_flow,
        model: metadata(&profile.model, profile.version, validity),
        warnings,
    })
}

fn propeller_efficiency(profile: &PropellerProfile, speed_m_s: f64, mode: OperatingMode) -> f64 {
    let speed_fraction = (speed_m_s / 45.0).clamp(0.0, 1.0);
    let base = profile.static_efficiency
        + speed_fraction * (profile.cruise_efficiency - profile.static_efficiency);
    let mode_factor = match mode {
        OperatingMode::Climb => profile.climb_installation_factor,
        _ => 1.0,
    };
    (base * mode_factor).min(profile.maximum_efficiency)
}

fn evaluate_turbofan(
    profile: &TurbofanProfile,
    engine_count: u32,
    atmosphere: &AtmosphereState,
    query: PropulsionQuery,
) -> AexResult<PropulsionState> {
    let density_ratio = (atmosphere.density_kg_m3 / 1.225).max(0.0);
    let altitude_lapse = density_ratio.powf(profile.altitude_exponent);
    let mach_lapse = (1.0 - profile.mach_linear_coefficient * query.mach).max(0.0);
    let lapse = (altitude_lapse * mach_lapse).max(profile.minimum_thrust_fraction);
    let installation_factor = 1.0 - profile.thrust_loss_fraction;
    let thrust = profile.sea_level_static_thrust_n
        * f64::from(engine_count)
        * lapse
        * query.throttle
        * installation_factor;
    let tsfc = match query.mode {
        OperatingMode::Takeoff => profile.tsfc_takeoff_kg_n_hr,
        _ => profile.tsfc_cruise_kg_n_hr,
    };
    let fuel_flow = tsfc * thrust / 3600.0;
    let mut warnings = Vec::new();
    let extrapolated =
        query.mach > profile.maximum_mach || query.altitude_m > profile.maximum_altitude_m;
    if extrapolated {
        warnings.push(Diagnostic::warning(
            "MODEL_EXTRAPOLATION",
            format!(
                "Turbofan profile evaluated at {:.0} m and Mach {:.3}.",
                query.altitude_m, query.mach
            ),
            "aircraft.propulsion.profile",
        ));
    }
    Ok(PropulsionState {
        thrust_available_n: Some(thrust),
        shaft_power_available_w: None,
        propulsive_power_available_w: Some(thrust * query.true_airspeed_m_s),
        fuel_flow_kg_s: fuel_flow,
        model: metadata(
            &profile.model,
            profile.version,
            if extrapolated {
                "extrapolated"
            } else {
                "valid"
            },
        ),
        warnings,
    })
}

fn metadata(model_id: &str, version: u32, validity: &str) -> ModelMetadata {
    ModelMetadata {
        model_id: model_id.to_owned(),
        model_version: version.to_string(),
        fidelity_level: 1,
        validity_status: validity.to_owned(),
    }
}

#[cfg(test)]
mod tests;

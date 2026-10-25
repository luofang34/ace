use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
pub(crate) use crate::domain::propulsion::PropulsionMode as OperatingMode;
use crate::domain::propulsion::TablePropulsionDeck;
use crate::domain::result::{AerodynamicState, AtmosphereState, ModelMetadata, PropulsionState};
use crate::domain::schema::{
    EngineProfile, PistonProfile, PropellerProfile, ResolvedScenario, SimpleTurbofanDeck,
    TurbofanProfile,
};
use crate::domain::warning::WarningCode;

const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
const DEFAULT_PROPELLER_EFFICIENCY: f64 = 0.75;

mod table_deck;

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
            scenario.aircraft.propulsion.sizing_factor,
            atmosphere,
            query,
        ),
        EngineProfile::Turbofan(profile) => evaluate_turbofan(
            profile,
            scenario.aircraft.propulsion.engine_count,
            scenario.aircraft.propulsion.sizing_factor,
            atmosphere,
            query,
        ),
    }
}

fn evaluate_piston(
    profile: &PistonProfile,
    propeller: Option<&PropellerProfile>,
    engine_count: u32,
    sizing_factor: f64,
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
    let density_ratio = (atmosphere.density_kg_m3 / SEA_LEVEL_DENSITY_KG_M3).max(0.0);
    let lapse = profile.power_lapse.fraction(density_ratio).fraction;
    let shaft_power =
        profile.rated_power_w * f64::from(engine_count) * sizing_factor * lapse * query.throttle;
    let efficiency = propeller_efficiency(propeller, query.true_airspeed_m_s, query.mode);
    let propulsive_power = shaft_power * efficiency;
    let static_reference_speed = 15.0;
    let thrust = propulsive_power / query.true_airspeed_m_s.max(static_reference_speed);
    let bsfc = profile.bsfc.at(query.mode, query.altitude_m);
    let fuel_flow = bsfc * shaft_power / 1000.0 / 3600.0;
    let mut warnings = Vec::new();
    let validity = if profile
        .maximum_altitude_m
        .is_some_and(|maximum| query.altitude_m > maximum)
    {
        warnings.push(Diagnostic::warning(
            WarningCode::ModelExtrapolation,
            "Shaft-engine profile evaluated above its maximum altitude.",
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

/// Whether a declared minimum power fraction, not the lapse law, sets shaft power.
pub(crate) fn shaft_power_floored(scenario: &ResolvedScenario, density_kg_m3: f64) -> bool {
    match &scenario.engine {
        EngineProfile::Piston(profile) => {
            profile
                .power_lapse
                .fraction(density_kg_m3 / SEA_LEVEL_DENSITY_KG_M3)
                .floored
        }
        EngineProfile::Turbofan(_) => false,
    }
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

/// Full-throttle installed state of `engine_count` sized copies of one turbofan
/// profile, independent of a resolved scenario.
pub(crate) fn installed_turbofan(
    profile: &TurbofanProfile,
    engine_count: u32,
    sizing_factor: f64,
    atmosphere: &AtmosphereState,
    query: PropulsionQuery,
) -> AexResult<PropulsionState> {
    evaluate_turbofan(profile, engine_count, sizing_factor, atmosphere, query)
}

fn evaluate_turbofan(
    profile: &TurbofanProfile,
    engine_count: u32,
    sizing_factor: f64,
    atmosphere: &AtmosphereState,
    query: PropulsionQuery,
) -> AexResult<PropulsionState> {
    if let Some(deck) = &profile.table_deck {
        return evaluate_table_turbofan(profile, deck, engine_count, sizing_factor, query);
    }
    let deck = profile.simple_deck.as_ref().ok_or_else(|| {
        AexError::validation(
            "MISSING_PROPULSION_DECK",
            "aircraft.propulsion.profile",
            "turbofan profile requires one resolved propulsion deck",
        )
    })?;
    evaluate_simple_turbofan(
        profile,
        deck,
        engine_count,
        sizing_factor,
        atmosphere,
        query,
    )
}

fn evaluate_simple_turbofan(
    profile: &TurbofanProfile,
    deck: &SimpleTurbofanDeck,
    engine_count: u32,
    sizing_factor: f64,
    atmosphere: &AtmosphereState,
    query: PropulsionQuery,
) -> AexResult<PropulsionState> {
    let density_ratio = (atmosphere.density_kg_m3 / 1.225).max(0.0);
    let altitude_lapse = density_ratio.powf(deck.altitude_exponent);
    let mach_lapse = (1.0 - deck.mach_linear_coefficient * query.mach).max(0.0);
    let lapse = (altitude_lapse * mach_lapse).max(deck.minimum_thrust_fraction);
    let installation_factor = 1.0 - profile.thrust_loss_fraction;
    let thrust = deck.sea_level_static_thrust_n
        * f64::from(engine_count)
        * sizing_factor
        * lapse
        * query.throttle
        * installation_factor;
    let tsfc = match query.mode {
        OperatingMode::Takeoff => deck.tsfc_takeoff_kg_n_hr,
        _ => deck.tsfc_cruise_kg_n_hr,
    };
    let fuel_flow = tsfc * thrust / 3600.0;
    let mut warnings = Vec::new();
    let extrapolated = query.mach > deck.maximum_mach || query.altitude_m > deck.maximum_altitude_m;
    if extrapolated {
        warnings.push(Diagnostic::warning(
            WarningCode::ModelExtrapolation,
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

fn evaluate_table_turbofan(
    profile: &TurbofanProfile,
    deck: &TablePropulsionDeck,
    engine_count: u32,
    sizing_factor: f64,
    query: PropulsionQuery,
) -> AexResult<PropulsionState> {
    let point = table_deck::evaluate(deck, query.mode, query.altitude_m, query.mach)?;
    let thrust = point.thrust_per_engine_n
        * f64::from(engine_count)
        * sizing_factor
        * query.throttle
        * (1.0 - profile.thrust_loss_fraction);
    let fuel_flow = point.fuel.flow_for_thrust(thrust);
    Ok(PropulsionState {
        thrust_available_n: Some(thrust),
        shaft_power_available_w: None,
        propulsive_power_available_w: Some(thrust * query.true_airspeed_m_s),
        fuel_flow_kg_s: fuel_flow,
        model: metadata(
            &profile.model,
            profile.version,
            if point.extrapolated {
                "extrapolated"
            } else {
                "valid"
            },
        ),
        warnings: point.warnings,
    })
}

#[derive(Debug)]
pub(crate) struct RequiredThrustFuelFlow {
    pub(crate) flow_kg_s: f64,
    pub(crate) warnings: Vec<Diagnostic>,
}

pub(crate) fn fuel_flow_for_required_thrust(
    profile: &TurbofanProfile,
    altitude_m: f64,
    mach: f64,
    mode: OperatingMode,
    thrust_n: f64,
) -> AexResult<RequiredThrustFuelFlow> {
    if let Some(deck) = &profile.table_deck {
        let point = table_deck::evaluate(deck, mode, altitude_m, mach)?;
        return Ok(RequiredThrustFuelFlow {
            flow_kg_s: point.fuel.flow_for_thrust(thrust_n),
            warnings: point.warnings,
        });
    }
    let deck = profile.simple_deck.as_ref().ok_or_else(|| {
        AexError::validation(
            "MISSING_PROPULSION_DECK",
            "aircraft.propulsion.profile",
            "turbofan profile requires one resolved propulsion deck",
        )
    })?;
    let tsfc = match mode {
        OperatingMode::Takeoff => deck.tsfc_takeoff_kg_n_hr,
        _ => deck.tsfc_cruise_kg_n_hr,
    };
    Ok(RequiredThrustFuelFlow {
        flow_kg_s: tsfc * thrust_n / 3600.0,
        warnings: Vec::new(),
    })
}

/// Fuel flow at the power or thrust needed to fly `aero`'s condition, as opposed to
/// the full-throttle flow in [`PropulsionState`]. Shaft engines convert required
/// power through the propeller's cruise efficiency and the mode BSFC (economy keeps
/// economy BSFC; every other mode uses cruise).
pub(crate) fn required_fuel_flow(
    scenario: &ResolvedScenario,
    aero: &AerodynamicState,
    altitude_m: f64,
    mach: f64,
    mode: OperatingMode,
) -> AexResult<RequiredFuelFlow> {
    match &scenario.engine {
        EngineProfile::Piston(profile) => {
            let efficiency = scenario
                .propeller
                .as_ref()
                .map_or(DEFAULT_PROPELLER_EFFICIENCY, |propeller| {
                    propeller.cruise_efficiency
                });
            let shaft_power_kw = aero.power_required_w / efficiency / 1000.0;
            let bsfc = profile.bsfc.at(
                match mode {
                    OperatingMode::Economy => OperatingMode::Economy,
                    _ => OperatingMode::Cruise,
                },
                altitude_m,
            );
            Ok(RequiredFuelFlow {
                flow_kg_s: bsfc * shaft_power_kw / 3600.0,
                propeller_efficiency: Some(efficiency),
                warnings: Vec::new(),
            })
        }
        EngineProfile::Turbofan(profile) => {
            let result =
                fuel_flow_for_required_thrust(profile, altitude_m, mach, mode, aero.drag_n)?;
            Ok(RequiredFuelFlow {
                flow_kg_s: result.flow_kg_s,
                propeller_efficiency: None,
                warnings: result.warnings,
            })
        }
    }
}

#[derive(Debug)]
pub(crate) struct RequiredFuelFlow {
    pub(crate) flow_kg_s: f64,
    pub(crate) propeller_efficiency: Option<f64>,
    pub(crate) warnings: Vec<Diagnostic>,
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

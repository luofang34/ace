use serde_yaml::Value;

use crate::domain::capabilities::{ProfileRole, profile_type};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::propulsion::TABLE_PROPULSION_MODEL_ID;
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::schema::{
    EngineProfile, PistonProfile, PropellerProfile, RawProfile, SimpleTurbofanDeck, TurbofanProfile,
};

mod table_deck;

pub(crate) fn parse_engine_profile(raw: RawProfile) -> AexResult<EngineProfile> {
    let profile = profile_type(&raw.kind);
    match profile.map(|item| (item.id, item.role)) {
        Some(("piston_engine", ProfileRole::Engine)) => {
            Ok(EngineProfile::Piston(parse_piston_profile(raw)?))
        }
        Some(("turbofan_engine", ProfileRole::Engine)) => {
            Ok(EngineProfile::Turbofan(parse_turbofan_profile(raw)?))
        }
        Some(("turbojet_engine" | "rocket_engine", ProfileRole::Engine)) => {
            require_table_thrust_profile(&raw)?;
            Ok(EngineProfile::Turbofan(parse_turbofan_profile(raw)?))
        }
        _ => Err(AexError::validation(
            "UNSUPPORTED_PROPULSION_PROFILE",
            "profile.type",
            format!("unsupported profile type {}", raw.kind),
        )),
    }
}

pub(crate) fn parse_propeller_profile(raw: RawProfile) -> AexResult<PropellerProfile> {
    if profile_type(&raw.kind).map(|item| item.role) != Some(ProfileRole::Propeller) {
        return Err(AexError::validation(
            "INCOMPATIBLE_PROFILE_TYPE",
            "aircraft.propulsion.propeller_profile",
            format!("expected propeller, got {}", raw.kind),
        ));
    }
    let source = profile_source(&raw);
    Ok(PropellerProfile {
        id: raw.id,
        version: raw.version,
        model: raw.model,
        source: source.0,
        confidence: source.1,
        diameter_m: profile_quantity(&raw.parameters, "diameter", Dimension::Length)?,
        blade_count: profile_u32(&raw.parameters, "blade_count")?,
        static_efficiency: profile_number(&raw.parameters, "static_efficiency")?,
        cruise_efficiency: profile_number(&raw.parameters, "cruise_efficiency")?,
        maximum_efficiency: profile_number(&raw.parameters, "maximum_efficiency")?,
        climb_installation_factor: profile_number(&raw.parameters, "climb_installation_factor")?,
        tip_mach_limit: profile_number(&raw.parameters, "tip_mach_limit")?,
    })
}

fn parse_piston_profile(raw: RawProfile) -> AexResult<PistonProfile> {
    let source = profile_source(&raw);
    Ok(PistonProfile {
        id: raw.id,
        version: raw.version,
        model: raw.model,
        source: source.0,
        confidence: source.1,
        rated_power_w: profile_quantity(&raw.parameters, "rated_power", Dimension::Power)?,
        rated_altitude_m: profile_quantity(&raw.parameters, "rated_altitude", Dimension::Length)?,
        rated_speed_rad_s: profile_quantity(
            &raw.parameters,
            "rated_speed",
            Dimension::RotationSpeed,
        )?,
        dry_mass_kg: profile_quantity(&raw.parameters, "dry_mass", Dimension::Mass)?,
        lapse_exponent: profile_number(&raw.parameters, "power_lapse.exponent")?,
        minimum_power_fraction: profile_number(
            &raw.parameters,
            "power_lapse.minimum_power_fraction",
        )?,
        bsfc_takeoff_kg_kwh: profile_number_with_unit(&raw.parameters, "bsfc.takeoff", "kg/kWh")?,
        bsfc_cruise_kg_kwh: profile_number_with_unit(&raw.parameters, "bsfc.cruise", "kg/kWh")?,
        bsfc_economy_kg_kwh: profile_number_with_unit(&raw.parameters, "bsfc.economy", "kg/kWh")?,
        maximum_altitude_m: profile_optional_quantity(
            &raw.parameters,
            "limits.maximum_altitude",
            Dimension::Length,
        )?,
    })
}

fn parse_turbofan_profile(raw: RawProfile) -> AexResult<TurbofanProfile> {
    let source = profile_source(&raw);
    let table_deck = table_deck::parse(&raw.parameters)?;
    validate_turbofan_deck_selection(&raw, table_deck.is_some())?;
    let simple_deck = if table_deck.is_none() {
        Some(parse_simple_turbofan_deck(&raw.parameters)?)
    } else {
        None
    };
    let reference_thrust_n = reference_turbofan_thrust(&simple_deck, &table_deck)?;
    Ok(TurbofanProfile {
        id: raw.id,
        version: raw.version,
        model: raw.model,
        source: source.0,
        confidence: source.1,
        dry_mass_kg: profile_quantity(&raw.parameters, "dry_mass", Dimension::Mass)?,
        bypass_ratio: if table_deck.is_some() {
            profile_optional_number(&raw.parameters, "bypass_ratio")?
        } else {
            Some(profile_number(&raw.parameters, "bypass_ratio")?)
        },
        thrust_loss_fraction: profile_number(&raw.parameters, "installation.thrust_loss_fraction")?,
        nacelle_drag_area_m2: profile_quantity(
            &raw.parameters,
            "installation.nacelle_drag_area",
            Dimension::Area,
        )?,
        overall_length_m: profile_optional_quantity(
            &raw.parameters,
            "dimensions.overall_length",
            Dimension::Length,
        )?,
        maximum_diameter_m: profile_optional_quantity(
            &raw.parameters,
            "dimensions.maximum_diameter",
            Dimension::Length,
        )?,
        simple_deck,
        table_deck,
        reference_thrust_n,
    })
}

fn reference_turbofan_thrust(
    simple: &Option<SimpleTurbofanDeck>,
    table: &Option<crate::domain::propulsion::TablePropulsionDeck>,
) -> AexResult<f64> {
    if let Some(deck) = table {
        return deck
            .interpolate(crate::domain::propulsion::PropulsionMode::Takeoff, 0.0, 0.0)
            .map(|point| point.thrust_per_engine_n)
            .filter(|thrust| thrust.is_finite() && *thrust > 0.0)
            .ok_or_else(|| {
                AexError::validation(
                    "INVALID_TABLE_EXTRAPOLATION",
                    "profile.parameters.table_deck.modes.takeoff.thrust",
                    "table must produce finite positive takeoff thrust at sea level and Mach zero",
                )
            });
    }
    simple
        .as_ref()
        .map(|deck| deck.sea_level_static_thrust_n)
        .ok_or_else(|| {
            AexError::validation(
                "MISSING_PROPULSION_DECK",
                "profile.parameters",
                "turbofan profile requires one propulsion deck",
            )
        })
}

fn require_table_thrust_profile(raw: &RawProfile) -> AexResult<()> {
    if raw.model == TABLE_PROPULSION_MODEL_ID {
        Ok(())
    } else {
        Err(AexError::validation(
            "INCOMPATIBLE_PROPULSION_MODEL",
            "profile.model",
            format!("{} profiles require propulsion.table_deck", raw.kind),
        ))
    }
}

fn validate_turbofan_deck_selection(raw: &RawProfile, has_table: bool) -> AexResult<()> {
    let has_simple = ["sea_level_static_thrust", "thrust_lapse", "tsfc", "limits"]
        .into_iter()
        .any(|field| raw.parameters.get(field).is_some());
    if has_table && has_simple {
        return Err(AexError::validation(
            "PROPULSION_DECK_EXCLUSIVITY",
            "profile.parameters",
            "table and simple turbofan deck parameters are mutually exclusive",
        ));
    }
    if has_table != (raw.model == TABLE_PROPULSION_MODEL_ID) {
        return Err(AexError::validation(
            "INCOMPATIBLE_PROPULSION_MODEL",
            "profile.model",
            "propulsion.table_deck model and table_deck parameters must be selected together",
        ));
    }
    Ok(())
}

fn parse_simple_turbofan_deck(parameters: &Value) -> AexResult<SimpleTurbofanDeck> {
    Ok(SimpleTurbofanDeck {
        sea_level_static_thrust_n: profile_quantity(
            parameters,
            "sea_level_static_thrust",
            Dimension::Force,
        )?,
        altitude_exponent: profile_number(parameters, "thrust_lapse.altitude_exponent")?,
        mach_linear_coefficient: profile_number(
            parameters,
            "thrust_lapse.mach_linear_coefficient",
        )?,
        minimum_thrust_fraction: profile_number(parameters, "thrust_lapse.minimum_fraction")?,
        tsfc_takeoff_kg_n_hr: profile_number_with_unit(
            parameters,
            "tsfc.sea_level_takeoff",
            "kg/N/hr",
        )?,
        tsfc_cruise_kg_n_hr: profile_number_with_unit(
            parameters,
            "tsfc.cruise_reference",
            "kg/N/hr",
        )?,
        cruise_reference_altitude_m: profile_quantity(
            parameters,
            "tsfc.cruise_reference_altitude",
            Dimension::Length,
        )?,
        cruise_reference_mach: profile_number(parameters, "tsfc.cruise_reference_mach")?,
        maximum_mach: profile_number(parameters, "limits.maximum_mach")?,
        maximum_altitude_m: profile_quantity(
            parameters,
            "limits.maximum_altitude",
            Dimension::Length,
        )?,
    })
}

fn profile_source(raw: &RawProfile) -> (String, String) {
    raw.metadata.as_ref().map_or_else(
        || ("unspecified profile source".to_owned(), "low".to_owned()),
        |metadata| (metadata.source.clone(), metadata.confidence.clone()),
    )
}

fn profile_value<'a>(parameters: &'a Value, path: &str) -> AexResult<&'a Value> {
    let mut current = parameters;
    for part in path.split('.') {
        current = current.get(part).ok_or_else(|| {
            AexError::validation(
                "MISSING_PROFILE_PARAMETER",
                path,
                format!("profile parameter {path} is required"),
            )
        })?;
    }
    Ok(current)
}

fn profile_number(parameters: &Value, path: &str) -> AexResult<f64> {
    profile_value(parameters, path)?
        .as_f64()
        .ok_or_else(|| AexError::validation("INVALID_PROFILE_PARAMETER", path, "expected number"))
}

fn profile_optional_number(parameters: &Value, path: &str) -> AexResult<Option<f64>> {
    match profile_value(parameters, path) {
        Ok(value) => value.as_f64().map(Some).ok_or_else(|| {
            AexError::validation("INVALID_PROFILE_PARAMETER", path, "expected number")
        }),
        Err(AexError::Validation {
            code: "MISSING_PROFILE_PARAMETER",
            ..
        }) => Ok(None),
        Err(error) => Err(error),
    }
}

fn profile_u32(parameters: &Value, path: &str) -> AexResult<u32> {
    let number = profile_value(parameters, path)?.as_u64().ok_or_else(|| {
        AexError::validation("INVALID_PROFILE_PARAMETER", path, "expected integer")
    })?;
    u32::try_from(number).map_err(|source| {
        AexError::validation("INVALID_PROFILE_PARAMETER", path, source.to_string())
    })
}

fn profile_quantity(parameters: &Value, path: &str, dimension: Dimension) -> AexResult<f64> {
    let raw = profile_value(parameters, path)?.as_str().ok_or_else(|| {
        AexError::validation("INVALID_PROFILE_PARAMETER", path, "expected quantity")
    })?;
    parse_quantity(raw, dimension)
}

fn profile_optional_quantity(
    parameters: &Value,
    path: &str,
    dimension: Dimension,
) -> AexResult<Option<f64>> {
    match profile_value(parameters, path) {
        Ok(value) => {
            let raw = value.as_str().ok_or_else(|| {
                AexError::validation("INVALID_PROFILE_PARAMETER", path, "expected quantity")
            })?;
            Ok(Some(parse_quantity(raw, dimension)?))
        }
        Err(AexError::Validation {
            code: "MISSING_PROFILE_PARAMETER",
            ..
        }) => Ok(None),
        Err(error) => Err(error),
    }
}

fn profile_number_with_unit(parameters: &Value, path: &str, unit: &str) -> AexResult<f64> {
    let raw = profile_value(parameters, path)?.as_str().ok_or_else(|| {
        AexError::validation("INVALID_PROFILE_PARAMETER", path, "expected quantity")
    })?;
    let suffix = format!(" {unit}");
    let number = raw.strip_suffix(&suffix).ok_or_else(|| {
        AexError::validation("INCOMPATIBLE_UNITS", path, format!("expected {unit}"))
    })?;
    number.trim().parse::<f64>().map_err(|source| {
        AexError::validation("INVALID_PROFILE_PARAMETER", path, source.to_string())
    })
}

#[cfg(test)]
mod tests;

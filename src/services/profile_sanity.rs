use serde_json::json;

use crate::domain::diagnostic::Diagnostic;
use crate::domain::schema::{EngineProfile, PistonProfile, PropellerProfile, TurbofanProfile};
use crate::domain::warning::WarningCode;

#[derive(Debug, Clone, Copy)]
struct TypicalRange {
    path: &'static str,
    lower: f64,
    upper: f64,
    unit: &'static str,
}

const PISTON_RANGES: &[TypicalRange] = &[
    range("rated_power", 30_000.0, 4_000_000.0, "W"),
    range("rated_altitude", -500.0, 15_000.0, "m"),
    range("rated_speed", 52.0, 628.0, "rad/s"),
    range("dry_mass", 20.0, 5_000.0, "kg"),
    range("power_lapse.exponent", 0.3, 2.0, "1"),
    range("power_lapse.minimum_power_fraction", 0.05, 0.8, "1"),
    range("bsfc.takeoff", 0.12, 0.8, "kg/kWh"),
    range("bsfc.cruise", 0.12, 0.8, "kg/kWh"),
    range("bsfc.economy", 0.12, 0.8, "kg/kWh"),
    range("limits.maximum_altitude", 0.0, 20_000.0, "m"),
];

const TURBOFAN_RANGES: &[TypicalRange] = &[
    range("sea_level_static_thrust", 5_000.0, 800_000.0, "N"),
    range("dry_mass", 100.0, 20_000.0, "kg"),
    range("bypass_ratio", 0.2, 15.0, "1"),
    range("thrust_lapse.altitude_exponent", 0.3, 1.2, "1"),
    range("thrust_lapse.mach_linear_coefficient", 0.05, 0.8, "1"),
    range("thrust_lapse.minimum_fraction", 0.02, 0.5, "1"),
    range("tsfc.sea_level_takeoff", 0.005, 0.25, "kg/N/hr"),
    range("tsfc.cruise_reference", 0.01, 0.25, "kg/N/hr"),
    range("tsfc.cruise_reference_altitude", 0.0, 20_000.0, "m"),
    range("tsfc.cruise_reference_mach", 0.2, 1.0, "1"),
    range("installation.thrust_loss_fraction", 0.0, 0.25, "1"),
    range("installation.nacelle_drag_area", 0.05, 20.0, "m^2"),
    range("dimensions.overall_length", 0.5, 15.0, "m"),
    range("dimensions.maximum_diameter", 0.3, 5.0, "m"),
    range("limits.maximum_mach", 0.3, 1.2, "1"),
    range("limits.maximum_altitude", 3_000.0, 20_000.0, "m"),
];

const PROPELLER_RANGES: &[TypicalRange] = &[
    range("diameter", 0.5, 8.0, "m"),
    range("blade_count", 1.0, 12.0, "1"),
    range("static_efficiency", 0.2, 0.8, "1"),
    range("cruise_efficiency", 0.5, 0.95, "1"),
    range("maximum_efficiency", 0.5, 0.98, "1"),
    range("climb_installation_factor", 0.2, 1.0, "1"),
    range("tip_mach_limit", 0.5, 0.95, "1"),
];

const fn range(path: &'static str, lower: f64, upper: f64, unit: &'static str) -> TypicalRange {
    TypicalRange {
        path,
        lower,
        upper,
        unit,
    }
}

pub(crate) fn profile_warnings(
    engine: &EngineProfile,
    propeller: Option<&PropellerProfile>,
) -> Vec<Diagnostic> {
    let mut warnings = engine_profile_warnings(engine);
    if let Some(profile) = propeller {
        warnings.extend(propeller_profile_warnings(profile));
    }
    warnings
}

pub(crate) fn engine_profile_warnings(engine: &EngineProfile) -> Vec<Diagnostic> {
    match engine {
        EngineProfile::Piston(profile) => warnings_for(&profile.id, PISTON_RANGES, |path| {
            piston_value(profile, path)
        }),
        EngineProfile::Turbofan(profile) => warnings_for(&profile.id, TURBOFAN_RANGES, |path| {
            turbofan_value(profile, path)
        }),
    }
}

pub(crate) fn propeller_profile_warnings(profile: &PropellerProfile) -> Vec<Diagnostic> {
    warnings_for(&profile.id, PROPELLER_RANGES, |path| {
        propeller_value(profile, path)
    })
}

fn warnings_for(
    profile_id: &str,
    ranges: &[TypicalRange],
    value: impl Fn(&str) -> Option<f64>,
) -> Vec<Diagnostic> {
    ranges
        .iter()
        .filter_map(|range| {
            value(range.path)
                .filter(|value| !value.is_finite() || !(range.lower..=range.upper).contains(value))
                .map(|value| outside_typical(profile_id, *range, value))
        })
        .collect()
}

fn outside_typical(profile_id: &str, range: TypicalRange, value: f64) -> Diagnostic {
    Diagnostic::warning_with_context(
        WarningCode::ParameterOutsideTypical,
        format!(
            "{} is {value} {}, outside the inclusive typical range [{}, {}] {}",
            range.path, range.unit, range.lower, range.upper, range.unit
        ),
        format!("profile.parameters.{}", range.path),
        json!({
            "profile_id": profile_id,
            "value": value,
            "range": {
                "minimum": range.lower,
                "maximum": range.upper,
                "inclusive": true,
            },
            "unit": range.unit,
        }),
    )
}

fn piston_value(profile: &PistonProfile, path: &str) -> Option<f64> {
    match path {
        "rated_power" => Some(profile.rated_power_w),
        "rated_altitude" => Some(profile.rated_altitude_m),
        "rated_speed" => Some(profile.rated_speed_rad_s),
        "dry_mass" => Some(profile.dry_mass_kg),
        "power_lapse.exponent" => Some(profile.lapse_exponent),
        "power_lapse.minimum_power_fraction" => Some(profile.minimum_power_fraction),
        "bsfc.takeoff" => Some(profile.bsfc_takeoff_kg_kwh),
        "bsfc.cruise" => Some(profile.bsfc_cruise_kg_kwh),
        "bsfc.economy" => Some(profile.bsfc_economy_kg_kwh),
        "limits.maximum_altitude" => profile.maximum_altitude_m,
        _ => None,
    }
}

fn turbofan_value(profile: &TurbofanProfile, path: &str) -> Option<f64> {
    let simple = profile.simple_deck.as_ref();
    match path {
        "sea_level_static_thrust" => simple.map(|deck| deck.sea_level_static_thrust_n),
        "dry_mass" => Some(profile.dry_mass_kg),
        "bypass_ratio" => profile.bypass_ratio,
        "thrust_lapse.altitude_exponent" => simple.map(|deck| deck.altitude_exponent),
        "thrust_lapse.mach_linear_coefficient" => simple.map(|deck| deck.mach_linear_coefficient),
        "thrust_lapse.minimum_fraction" => simple.map(|deck| deck.minimum_thrust_fraction),
        "tsfc.sea_level_takeoff" => simple.map(|deck| deck.tsfc_takeoff_kg_n_hr),
        "tsfc.cruise_reference" => simple.map(|deck| deck.tsfc_cruise_kg_n_hr),
        "tsfc.cruise_reference_altitude" => simple.map(|deck| deck.cruise_reference_altitude_m),
        "tsfc.cruise_reference_mach" => simple.map(|deck| deck.cruise_reference_mach),
        "installation.thrust_loss_fraction" => Some(profile.thrust_loss_fraction),
        "installation.nacelle_drag_area" => Some(profile.nacelle_drag_area_m2),
        "dimensions.overall_length" => profile.overall_length_m,
        "dimensions.maximum_diameter" => profile.maximum_diameter_m,
        "limits.maximum_mach" => simple.map(|deck| deck.maximum_mach),
        "limits.maximum_altitude" => simple.map(|deck| deck.maximum_altitude_m),
        _ => None,
    }
}

fn propeller_value(profile: &PropellerProfile, path: &str) -> Option<f64> {
    match path {
        "diameter" => Some(profile.diameter_m),
        "blade_count" => Some(f64::from(profile.blade_count)),
        "static_efficiency" => Some(profile.static_efficiency),
        "cruise_efficiency" => Some(profile.cruise_efficiency),
        "maximum_efficiency" => Some(profile.maximum_efficiency),
        "climb_installation_factor" => Some(profile.climb_installation_factor),
        "tip_mach_limit" => Some(profile.tip_mach_limit),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

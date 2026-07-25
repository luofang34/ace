use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::schema::{EngineProfile, ProfileDocument, PropellerProfile};
use crate::services::profile_resolution::{parse_engine_profile, parse_propeller_profile};

use super::{
    PISTON_RANGES, PROPELLER_RANGES, TURBOFAN_RANGES, piston_value, profile_warnings,
    propeller_value, turbofan_value,
};

const PISTON_PATHS: [&str; 10] = [
    "rated_power",
    "rated_altitude",
    "rated_speed",
    "dry_mass",
    "power_lapse.exponent",
    "power_lapse.minimum_power_fraction",
    "bsfc.takeoff",
    "bsfc.cruise",
    "bsfc.economy",
    "limits.maximum_altitude",
];
const TURBOFAN_PATHS: [&str; 16] = [
    "sea_level_static_thrust",
    "dry_mass",
    "bypass_ratio",
    "thrust_lapse.altitude_exponent",
    "thrust_lapse.mach_linear_coefficient",
    "thrust_lapse.minimum_fraction",
    "tsfc.sea_level_takeoff",
    "tsfc.cruise_reference",
    "tsfc.cruise_reference_altitude",
    "tsfc.cruise_reference_mach",
    "installation.thrust_loss_fraction",
    "installation.nacelle_drag_area",
    "dimensions.overall_length",
    "dimensions.maximum_diameter",
    "limits.maximum_mach",
    "limits.maximum_altitude",
];
const PROPELLER_PATHS: [&str; 7] = [
    "diameter",
    "blade_count",
    "static_efficiency",
    "cruise_efficiency",
    "maximum_efficiency",
    "climb_installation_factor",
    "tip_mach_limit",
];

fn repository_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn profile_document(path: &Path) -> Result<ProfileDocument, Box<dyn std::error::Error>> {
    Ok(serde_yaml::from_str(&fs::read_to_string(path)?)?)
}

fn engine(relative: &str) -> Result<EngineProfile, Box<dyn std::error::Error>> {
    let document = profile_document(&repository_path(relative))?;
    Ok(parse_engine_profile(document.profile)?)
}

fn propeller(relative: &str) -> Result<PropellerProfile, Box<dyn std::error::Error>> {
    let document = profile_document(&repository_path(relative))?;
    Ok(parse_propeller_profile(document.profile)?)
}

fn warning_paths(warnings: &[crate::domain::diagnostic::Diagnostic]) -> Vec<&str> {
    warnings
        .iter()
        .filter_map(|warning| warning.path.as_deref())
        .collect()
}

fn numeric_leaf_count(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Number(_) => 1,
        serde_json::Value::Array(items) => items.iter().map(numeric_leaf_count).sum(),
        serde_json::Value::Object(items) => items.values().map(numeric_leaf_count).sum(),
        _ => 0,
    }
}

#[test]
fn stock_profiles_are_inside_registered_typical_ranges() -> Result<(), Box<dyn std::error::Error>> {
    for relative in [
        "profiles/engines/ge90.yaml",
        "profiles/engines/htf7700l.yaml",
        "profiles/engines/lycoming.yaml",
        "examples/b777/profiles/engine.yaml",
        "examples/c172/profiles/engine.yaml",
        "examples/designs/model501/baseline/profiles/passport.yaml",
        "examples/designs/model501/baseline/profiles/pw306d1.yaml",
        "examples/designs/model501/baseline/profiles/pw812d.yaml",
        "examples/designs/model501/candidates/model501_passport20/profiles/passport.yaml",
        "examples/designs/model501/candidates/model501_passport20/profiles/pw306d1.yaml",
        "examples/designs/model501/candidates/model501_passport20/profiles/pw812d.yaml",
        "examples/designs/model501/candidates/model501_pw812d/profiles/passport.yaml",
        "examples/designs/model501/candidates/model501_pw812d/profiles/pw306d1.yaml",
        "examples/designs/model501/candidates/model501_pw812d/profiles/pw812d.yaml",
        "examples/designs/model501/candidates/model501_twin_pw306d1/profiles/passport.yaml",
        "examples/designs/model501/candidates/model501_twin_pw306d1/profiles/pw306d1.yaml",
        "examples/designs/model501/candidates/model501_twin_pw306d1/profiles/pw812d.yaml",
    ] {
        let profile = engine(relative)?;
        assert!(profile_warnings(&profile, None).is_empty(), "{relative}");
    }
    let engine = engine("profiles/engines/lycoming.yaml")?;
    for relative in [
        "profiles/propellers/constant-speed.yaml",
        "examples/c172/profiles/propeller.yaml",
    ] {
        let profile = propeller(relative)?;
        assert!(
            profile_warnings(&engine, Some(&profile)).is_empty(),
            "{relative}"
        );
    }
    Ok(())
}

#[test]
fn every_registered_path_maps_to_a_finite_resolved_value() -> Result<(), Box<dyn std::error::Error>>
{
    let EngineProfile::Piston(piston) = engine("profiles/engines/lycoming.yaml")? else {
        return Err(io::Error::other("expected piston profile").into());
    };
    let EngineProfile::Turbofan(turbofan) = engine("profiles/engines/htf7700l.yaml")? else {
        return Err(io::Error::other("expected turbofan profile").into());
    };
    let propeller = propeller("profiles/propellers/constant-speed.yaml")?;

    assert_eq!(
        PISTON_RANGES
            .iter()
            .map(|range| range.path)
            .collect::<Vec<_>>(),
        PISTON_PATHS
    );
    assert_eq!(
        TURBOFAN_RANGES
            .iter()
            .map(|range| range.path)
            .collect::<Vec<_>>(),
        TURBOFAN_PATHS
    );
    assert_eq!(
        PROPELLER_RANGES
            .iter()
            .map(|range| range.path)
            .collect::<Vec<_>>(),
        PROPELLER_PATHS
    );
    assert!(
        PISTON_RANGES
            .iter()
            .all(|range| piston_value(&piston, range.path).is_some_and(f64::is_finite))
    );
    assert!(
        TURBOFAN_RANGES
            .iter()
            .all(|range| turbofan_value(&turbofan, range.path).is_some_and(f64::is_finite))
    );
    assert!(
        PROPELLER_RANGES
            .iter()
            .all(|range| propeller_value(&propeller, range.path).is_some_and(f64::is_finite))
    );
    assert_eq!(
        numeric_leaf_count(&serde_json::to_value(&piston)?),
        PISTON_RANGES.len() + 1
    );
    assert_eq!(
        numeric_leaf_count(&serde_json::to_value(&turbofan)?),
        TURBOFAN_RANGES.len() + 1
    );
    assert_eq!(
        numeric_leaf_count(&serde_json::to_value(&propeller)?),
        PROPELLER_RANGES.len() + 1
    );
    Ok(())
}

#[test]
fn surrogate_profiles_report_all_non_typical_parameters() -> Result<(), Box<dyn std::error::Error>>
{
    let j58 = engine("examples/sr71/profiles/j58.yaml")?;
    let xlr99 = engine("examples/x15/profiles/xlr99.yaml")?;
    let j58_warnings = profile_warnings(&j58, None);
    let xlr99_warnings = profile_warnings(&xlr99, None);
    let j58_paths = warning_paths(&j58_warnings);
    let xlr99_paths = warning_paths(&xlr99_warnings);

    assert_eq!(
        j58_paths,
        [
            "profile.parameters.bypass_ratio",
            "profile.parameters.thrust_lapse.mach_linear_coefficient",
            "profile.parameters.tsfc.cruise_reference_mach",
            "profile.parameters.installation.nacelle_drag_area",
            "profile.parameters.limits.maximum_mach",
            "profile.parameters.limits.maximum_altitude",
        ]
    );
    assert_eq!(
        xlr99_paths,
        [
            "profile.parameters.bypass_ratio",
            "profile.parameters.thrust_lapse.altitude_exponent",
            "profile.parameters.thrust_lapse.mach_linear_coefficient",
            "profile.parameters.thrust_lapse.minimum_fraction",
            "profile.parameters.tsfc.sea_level_takeoff",
            "profile.parameters.tsfc.cruise_reference",
            "profile.parameters.tsfc.cruise_reference_mach",
            "profile.parameters.installation.nacelle_drag_area",
            "profile.parameters.limits.maximum_mach",
            "profile.parameters.limits.maximum_altitude",
        ]
    );
    assert!(
        j58_warnings
            .iter()
            .chain(&xlr99_warnings)
            .all(|warning| warning.code == "PARAMETER_OUTSIDE_TYPICAL")
    );
    let warning = j58_warnings
        .first()
        .ok_or_else(|| io::Error::other("J58 produced no sanity warning"))?;
    assert_eq!(warning.context["profile_id"], "engine.pw_j58_class");
    assert!(warning.context["value"].is_number());
    assert!(warning.context["range"]["minimum"].is_number());
    assert!(warning.context["range"]["maximum"].is_number());
    assert_eq!(warning.context["range"]["inclusive"], true);
    assert!(warning.context["unit"].is_string());
    Ok(())
}

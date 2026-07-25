use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_yaml::Value;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::schema::{
    Aircraft, EngineProfile, ProfileDocument, PropellerProfile, RawProfile, ResolvedScenario,
};
use crate::domain::study::EmbeddedStudyBaseline;
use crate::services::assumptions::collect_all_assumptions;
use crate::services::overrides::apply_overrides;
use crate::services::profile_resolution::{parse_engine_profile, parse_propeller_profile};
use crate::services::requirement_resolution::resolve_requirements;

use super::{
    deserialize_value, require_schema_version, resolve_aircraft, resolve_mission, validate_payload,
};

pub(crate) fn resolve_embedded_study(
    embedded: &EmbeddedStudyBaseline,
) -> AexResult<ResolvedScenario> {
    require_embedded_versions(embedded)?;
    let raw = &embedded.scenario.scenario;
    let mut aircraft_value = serialize_value(&embedded.aircraft, "embedded.aircraft")?;
    let mut mission_value = serialize_value(&embedded.mission, "embedded.mission")?;
    let mut requirements_value = serialize_value(&embedded.requirements, "embedded.requirements")?;
    apply_overrides(
        &mut aircraft_value,
        &mut mission_value,
        &mut requirements_value,
        &raw.overrides,
    )?;
    let aircraft = resolve_aircraft(deserialize_value(aircraft_value.clone(), "aircraft")?)?;
    let mission = resolve_mission(deserialize_value(mission_value.clone(), "mission")?)?;
    let requirements = resolve_requirements(deserialize_value(
        requirements_value.clone(),
        "requirements",
    )?)?;
    validate_payload(&aircraft, &mission)?;
    let (engine, propeller) = resolve_embedded_profiles(&embedded.profiles, &aircraft)?;
    let assumptions = collect_all_assumptions(
        &aircraft_value,
        &mission_value,
        &requirements_value,
        &engine,
        propeller.as_ref(),
    );
    Ok(ResolvedScenario {
        id: raw.id.clone(),
        name: raw.name.clone(),
        aircraft,
        mission,
        requirements,
        engine,
        propeller,
        assumptions,
        warnings: vec![Diagnostic::limitation(
            "Results use conceptual fidelity-level 0 or 1 equations.",
        )],
        source_path: PathBuf::from(format!("<embedded:{}>", raw.id)),
    })
}

fn require_embedded_versions(embedded: &EmbeddedStudyBaseline) -> AexResult<()> {
    for (version, path) in [
        (embedded.scenario.schema_version, "embedded.scenario"),
        (embedded.aircraft.schema_version, "embedded.aircraft"),
        (embedded.mission.schema_version, "embedded.mission"),
        (
            embedded.requirements.schema_version,
            "embedded.requirements",
        ),
    ] {
        require_schema_version(version, path)?;
    }
    Ok(())
}

fn resolve_embedded_profiles(
    profiles: &[ProfileDocument],
    aircraft: &Aircraft,
) -> AexResult<(EngineProfile, Option<PropellerProfile>)> {
    validate_embedded_profiles(profiles)?;
    let engine = find_embedded_profile(profiles, &aircraft.propulsion.profile)
        .and_then(parse_engine_profile)?;
    let propeller = aircraft
        .propulsion
        .propeller_profile
        .as_deref()
        .map(|profile_id| {
            find_embedded_profile(profiles, profile_id).and_then(parse_propeller_profile)
        })
        .transpose()?;
    Ok((engine, propeller))
}

fn validate_embedded_profiles(profiles: &[ProfileDocument]) -> AexResult<()> {
    let mut ids = BTreeSet::new();
    for profile in profiles {
        require_schema_version(profile.schema_version, "embedded.profiles")?;
        if !ids.insert(profile.profile.id.as_str()) {
            return Err(AexError::validation(
                "DUPLICATE_PROFILE_ID",
                "embedded.profiles",
                format!("duplicate profile {}", profile.profile.id),
            ));
        }
        if profile.profile.kind == "propeller" {
            parse_propeller_profile(profile.profile.clone())?;
        } else {
            parse_engine_profile(profile.profile.clone())?;
        }
    }
    Ok(())
}

fn find_embedded_profile(profiles: &[ProfileDocument], profile_id: &str) -> AexResult<RawProfile> {
    profiles
        .iter()
        .find(|profile| profile.profile.id == profile_id)
        .map(|profile| profile.profile.clone())
        .ok_or_else(|| AexError::ProfileNotFound {
            profile_id: profile_id.to_owned(),
            directory: PathBuf::from("<embedded-profiles>"),
        })
}

fn serialize_value<T: serde::Serialize>(value: &T, path: &str) -> AexResult<Value> {
    serde_yaml::to_value(value).map_err(|source| AexError::Yaml {
        path: path.into(),
        source,
    })
}

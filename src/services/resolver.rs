use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_yaml::Value;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::schema::{
    AeroConfiguration, Aerodynamics, Aircraft, AircraftDocument, AircraftLimits, ConceptMetadata,
    EngineProfile, MassProperties, Mission, MissionDocument, PropellerProfile, Propulsion,
    RawAeroConfiguration, RequirementsDocument, ResolvedScenario, ScenarioDocument,
};
use crate::domain::topology::AircraftTopology;
use crate::services::assumptions::collect_all_assumptions;
use crate::services::overrides::apply_overrides;
use crate::services::profile_resolution::{parse_engine_profile, parse_propeller_profile};
use crate::services::profile_sanity::profile_warnings;
use crate::services::requirement_resolution::resolve_requirements;
use crate::storage::profile_store::ProfileRepository;
use crate::storage::project_store::read_yaml_value_blocking;

mod embedded;
mod energy_climb;
mod initial_state;
mod planform;
mod segments;

pub(crate) use embedded::resolve_embedded_study;
use energy_climb::{validate_energy_schedule_limits, validate_energy_schedule_transitions};
pub(crate) use initial_state::usable_initial_fuel_kg;
use initial_state::{resolve_initial_state, validate_initial_state};
pub(crate) use planform::complete_planform_overrides;
use segments::resolve_segment;

#[derive(Clone)]
pub(crate) struct ScenarioResolver {
    profiles: Arc<dyn ProfileRepository>,
}

impl ScenarioResolver {
    pub(crate) fn new(profiles: Arc<dyn ProfileRepository>) -> Self {
        Self { profiles }
    }

    pub(crate) fn resolve_blocking(
        &self,
        scenario_path: &Path,
        overrides: &BTreeMap<String, String>,
    ) -> AexResult<ResolvedScenario> {
        let scenario_document: ScenarioDocument = read_document_blocking(scenario_path)?;
        require_schema_version(scenario_document.schema_version, "scenario")?;
        let directory = scenario_path.parent().unwrap_or_else(|| Path::new("."));
        let raw = &scenario_document.scenario;
        let mut effective_overrides = raw.overrides.clone();
        effective_overrides.extend(overrides.clone());
        let mut aircraft_value = read_yaml_value_blocking(&directory.join(&raw.aircraft))?;
        let mut mission_value = read_yaml_value_blocking(&directory.join(&raw.mission))?;
        let mut requirements_value = read_yaml_value_blocking(&directory.join(&raw.requirements))?;
        apply_overrides(
            &mut aircraft_value,
            &mut mission_value,
            &mut requirements_value,
            &effective_overrides,
        )?;
        let aircraft_document: AircraftDocument =
            deserialize_value(aircraft_value.clone(), "aircraft")?;
        let mission_document: MissionDocument =
            deserialize_value(mission_value.clone(), "mission")?;
        let requirements_document: RequirementsDocument =
            deserialize_value(requirements_value.clone(), "requirements")?;
        require_schema_version(aircraft_document.schema_version, "aircraft")?;
        require_schema_version(mission_document.schema_version, "mission")?;
        require_schema_version(requirements_document.schema_version, "requirements")?;
        let aircraft = resolve_aircraft(aircraft_document)?;
        let mission = resolve_mission(mission_document)?;
        let requirements = resolve_requirements(requirements_document)?;
        validate_payload(&aircraft, &mission)?;
        validate_initial_state(&aircraft, &mission)?;
        validate_energy_schedule_limits(&aircraft, &mission)?;
        let (engine, propeller) = self.resolve_profiles(directory, &aircraft)?;
        let assumptions = collect_all_assumptions(
            &aircraft_value,
            &mission_value,
            &requirements_value,
            &engine,
            propeller.as_ref(),
        );
        let mut warnings = vec![Diagnostic::limitation(
            "Results use conceptual fidelity-level 0 or 1 equations.",
        )];
        warnings.extend(profile_warnings(&engine, propeller.as_ref()));
        Ok(ResolvedScenario {
            id: raw.id.clone(),
            name: raw.name.clone(),
            aircraft,
            mission,
            requirements,
            engine,
            propeller,
            assumptions,
            warnings,
            source_path: scenario_path.to_path_buf(),
        })
    }

    fn resolve_profiles(
        &self,
        scenario_directory: &Path,
        aircraft: &Aircraft,
    ) -> AexResult<(EngineProfile, Option<PropellerProfile>)> {
        let profile_directory = scenario_directory.join("profiles");
        let raw_engine = self
            .profiles
            .load_profile_blocking(&profile_directory, &aircraft.propulsion.profile)?;
        let engine = parse_engine_profile(raw_engine)?;
        let propeller = match &aircraft.propulsion.propeller_profile {
            Some(profile_id) => {
                let raw = self
                    .profiles
                    .load_profile_blocking(&profile_directory, profile_id)?;
                Some(parse_propeller_profile(raw)?)
            }
            None => None,
        };
        Ok((engine, propeller))
    }
}

fn read_document_blocking<T: DeserializeOwned>(path: &Path) -> AexResult<T> {
    let value = read_yaml_value_blocking(path)?;
    deserialize_value(value, &path.display().to_string())
}

fn deserialize_value<T: DeserializeOwned>(value: Value, path: &str) -> AexResult<T> {
    serde_yaml::from_value(value).map_err(|source| AexError::Yaml {
        path: path.into(),
        source,
    })
}

fn require_schema_version(version: u32, path: &str) -> AexResult<()> {
    if version == 1 {
        Ok(())
    } else {
        Err(AexError::validation(
            "UNSUPPORTED_SCHEMA_VERSION",
            path,
            format!("expected 1, got {version}"),
        ))
    }
}

pub(crate) fn resolve_aircraft(document: AircraftDocument) -> AexResult<Aircraft> {
    let raw = document.aircraft;
    let mass = resolve_mass(&raw.mass)?;
    let topology = AircraftTopology::resolve(
        raw.topology,
        &raw.configuration,
        raw.propulsion.engine_count,
        raw.propulsion.propeller_profile.is_some(),
    )?;
    let wing = planform::resolve_wing(&raw.geometry.wing)?;
    let aerodynamics = Aerodynamics {
        model: raw.aerodynamics.model,
        clean: resolve_aero(raw.aerodynamics.clean, "clean")?,
        takeoff: resolve_aero(raw.aerodynamics.takeoff, "takeoff")?,
        landing: resolve_aero(raw.aerodynamics.landing, "landing")?,
    };
    let limits = AircraftLimits {
        maximum_operating_speed_m_s: optional_quantity(
            raw.limits.maximum_operating_speed.as_deref(),
            Dimension::Speed,
        )?,
        maximum_operating_mach: raw.limits.maximum_operating_mach,
        maximum_operating_altitude_m: optional_quantity(
            raw.limits.maximum_operating_altitude.as_deref(),
            Dimension::Length,
        )?,
        maximum_load_factor: raw.limits.maximum_load_factor,
        minimum_load_factor: raw.limits.minimum_load_factor,
    };
    Ok(Aircraft {
        id: raw.id,
        name: raw.name,
        category: raw.category,
        configuration: raw.configuration,
        propulsion_architecture: raw.propulsion_architecture,
        topology,
        metadata: ConceptMetadata {
            purpose: raw.metadata.purpose,
            certification_use: raw.metadata.certification_use,
        },
        mass,
        wing,
        aerodynamics,
        propulsion: Propulsion {
            profile: raw.propulsion.profile,
            engine_count: raw.propulsion.engine_count,
            propeller_profile: raw.propulsion.propeller_profile,
            sizing_factor: bounded(
                raw.propulsion.sizing_factor,
                0.5,
                2.0,
                "aircraft.propulsion.sizing_factor",
            )?,
        },
        limits,
    })
}

fn resolve_mass(raw: &crate::domain::schema::RawMass) -> AexResult<MassProperties> {
    let result = MassProperties {
        maximum_takeoff_mass_kg: positive_quantity(
            &raw.maximum_takeoff_mass,
            Dimension::Mass,
            "aircraft.mass.maximum_takeoff_mass",
        )?,
        operating_empty_mass_kg: positive_quantity(
            &raw.operating_empty_mass,
            Dimension::Mass,
            "aircraft.mass.operating_empty_mass",
        )?,
        maximum_payload_mass_kg: positive_quantity(
            &raw.maximum_payload_mass,
            Dimension::Mass,
            "aircraft.mass.maximum_payload_mass",
        )?,
        maximum_fuel_mass_kg: positive_quantity(
            &raw.maximum_fuel_mass,
            Dimension::Mass,
            "aircraft.mass.maximum_fuel_mass",
        )?,
    };
    if result.operating_empty_mass_kg >= result.maximum_takeoff_mass_kg {
        return Err(AexError::validation(
            "INVALID_MASS_LIMIT",
            "aircraft.mass.operating_empty_mass",
            "operating empty mass must be below maximum takeoff mass",
        ));
    }
    Ok(result)
}

fn resolve_aero(raw: RawAeroConfiguration, name: &str) -> AexResult<AeroConfiguration> {
    Ok(AeroConfiguration {
        cd0: positive(raw.cd0, &format!("aircraft.aerodynamics.{name}.cd0"))?,
        oswald_efficiency: fraction(
            raw.oswald_efficiency,
            &format!("aircraft.aerodynamics.{name}.oswald_efficiency"),
        )?,
        cl_max: positive(raw.cl_max, &format!("aircraft.aerodynamics.{name}.cl_max"))?,
        additional_cd: non_negative(
            raw.additional_cd,
            &format!("aircraft.aerodynamics.{name}.additional_cd"),
        )?,
        mach_critical: raw.mach_critical,
        wave_drag: raw.wave_drag,
    })
}

pub(crate) fn resolve_mission(document: MissionDocument) -> AexResult<Mission> {
    let raw = document.mission;
    if raw.segments.is_empty() {
        return Err(AexError::validation(
            "EMPTY_MISSION",
            "mission.segments",
            "mission requires at least one segment",
        ));
    }
    let segments = raw
        .segments
        .into_iter()
        .enumerate()
        .map(|(index, segment)| resolve_segment(segment, index))
        .collect::<AexResult<Vec<_>>>()?;
    let mission = Mission {
        id: raw.id,
        name: raw.name,
        payload_mass_kg: positive_quantity(
            &raw.payload.mass,
            Dimension::Mass,
            "mission.payload.mass",
        )?,
        initial_state: resolve_initial_state(raw.initial_state)?,
        segments,
    };
    validate_energy_schedule_transitions(&mission)?;
    Ok(mission)
}

fn validate_payload(aircraft: &Aircraft, mission: &Mission) -> AexResult<()> {
    if mission.payload_mass_kg > aircraft.mass.maximum_payload_mass_kg {
        return Err(AexError::validation(
            "PAYLOAD_LIMIT_EXCEEDED",
            "mission.payload.mass",
            format!(
                "{} kg exceeds structural limit {} kg",
                mission.payload_mass_kg, aircraft.mass.maximum_payload_mass_kg
            ),
        ));
    }
    Ok(())
}

fn positive_quantity(raw: &str, dimension: Dimension, path: &str) -> AexResult<f64> {
    positive(parse_quantity(raw, dimension)?, path)
}

fn optional_quantity(raw: Option<&str>, dimension: Dimension) -> AexResult<Option<f64>> {
    raw.map(|value| parse_quantity(value, dimension))
        .transpose()
}

fn optional_positive_quantity(
    raw: Option<&str>,
    dimension: Dimension,
    path: &str,
) -> AexResult<Option<f64>> {
    optional_quantity(raw, dimension)?
        .map(|value| positive(value, path))
        .transpose()
}

fn positive(value: f64, path: &str) -> AexResult<f64> {
    if value > 0.0 && value.is_finite() {
        Ok(value)
    } else {
        Err(AexError::validation(
            "NON_POSITIVE_VALUE",
            path,
            "value must be positive and finite",
        ))
    }
}

fn non_negative(value: f64, path: &str) -> AexResult<f64> {
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

fn fraction(value: f64, path: &str) -> AexResult<f64> {
    if value > 0.0 && value <= 1.2 && value.is_finite() {
        Ok(value)
    } else {
        Err(AexError::validation(
            "INVALID_FRACTION",
            path,
            "value is outside (0, 1.2]",
        ))
    }
}

fn optional_fraction(value: Option<f64>, path: &str) -> AexResult<Option<f64>> {
    value.map(|item| fraction(item, path)).transpose()
}

fn throttle_fraction(value: f64, path: &str) -> AexResult<f64> {
    if (0.0..=1.2).contains(&value) && value.is_finite() {
        Ok(value)
    } else {
        Err(AexError::validation(
            "INVALID_FRACTION",
            path,
            "value is outside [0, 1.2]",
        ))
    }
}

fn optional_throttle_fraction(value: Option<f64>, path: &str) -> AexResult<Option<f64>> {
    value.map(|item| throttle_fraction(item, path)).transpose()
}

fn bounded(value: f64, lower: f64, upper: f64, path: &str) -> AexResult<f64> {
    if value >= lower && value <= upper && value.is_finite() {
        Ok(value)
    } else {
        Err(AexError::validation(
            "VALUE_OUT_OF_RANGE",
            path,
            format!("value must be between {lower} and {upper}"),
        ))
    }
}

#[cfg(test)]
mod tests;

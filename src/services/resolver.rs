use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_yaml::Value;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::schema::{
    AeroConfiguration, Aerodynamics, Aircraft, AircraftDocument, AircraftLimits, ConceptMetadata,
    EngineProfile, MassProperties, Mission, MissionDocument, MissionSegment, PropellerProfile,
    Propulsion, RawAeroConfiguration, RawMissionSegment, RequirementsDocument, ResolvedScenario,
    ScenarioDocument, SegmentKind, Wing,
};
use crate::services::assumptions::collect_all_assumptions;
use crate::services::profile_resolution::{parse_engine_profile, parse_propeller_profile};
use crate::services::requirement_resolution::resolve_requirements;
use crate::storage::profile_store::ProfileRepository;
use crate::storage::project_store::read_yaml_value_blocking;

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
        let mut aircraft_value = read_yaml_value_blocking(&directory.join(&raw.aircraft))?;
        let mut mission_value = read_yaml_value_blocking(&directory.join(&raw.mission))?;
        let mut requirements_value = read_yaml_value_blocking(&directory.join(&raw.requirements))?;
        apply_overrides(
            &mut aircraft_value,
            &mut mission_value,
            &mut requirements_value,
            overrides,
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
        let (engine, propeller) = self.resolve_profiles(directory, &aircraft)?;
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

fn apply_overrides(
    aircraft: &mut Value,
    mission: &mut Value,
    requirements: &mut Value,
    overrides: &BTreeMap<String, String>,
) -> AexResult<()> {
    for (path, raw_value) in overrides {
        let parts: Vec<&str> = path.split('.').collect();
        let root = parts.first().copied().ok_or_else(|| {
            AexError::validation("INVALID_OVERRIDE", path, "override path is empty")
        })?;
        let document = match root {
            "aircraft" => &mut *aircraft,
            "mission" => &mut *mission,
            "requirements" => &mut *requirements,
            _ => {
                return Err(AexError::validation(
                    "INVALID_OVERRIDE",
                    path,
                    "path must begin with aircraft, mission, or requirements",
                ));
            }
        };
        set_path(document, &parts, Value::String(raw_value.clone()), path)?;
    }
    Ok(())
}

fn set_path(target: &mut Value, parts: &[&str], value: Value, full_path: &str) -> AexResult<()> {
    if parts.is_empty() {
        return Err(AexError::validation(
            "INVALID_OVERRIDE",
            full_path,
            "override must address a field",
        ));
    }
    let mapping = target.as_mapping_mut().ok_or_else(|| {
        AexError::validation("INVALID_OVERRIDE", full_path, "path is not a mapping")
    })?;
    let key = Value::String(parts[0].to_owned());
    if parts.len() == 1 {
        if !mapping.contains_key(&key) {
            return Err(AexError::validation(
                "INVALID_OVERRIDE",
                full_path,
                "field does not exist",
            ));
        }
        mapping.insert(key, value);
        return Ok(());
    }
    let nested = mapping.get_mut(&key).ok_or_else(|| {
        AexError::validation("INVALID_OVERRIDE", full_path, "field does not exist")
    })?;
    set_path(nested, &parts[1..], value, full_path)
}

pub(crate) fn resolve_aircraft(document: AircraftDocument) -> AexResult<Aircraft> {
    let raw = document.aircraft;
    let mass = resolve_mass(&raw.mass)?;
    let wing = Wing {
        area_m2: positive_quantity(
            &raw.geometry.wing.area,
            Dimension::Area,
            "aircraft.geometry.wing.area",
        )?,
        span_m: positive_quantity(
            &raw.geometry.wing.span,
            Dimension::Length,
            "aircraft.geometry.wing.span",
        )?,
        aspect_ratio: positive(
            raw.geometry.wing.aspect_ratio,
            "aircraft.geometry.wing.aspect_ratio",
        )?,
        sweep_quarter_chord_rad: parse_quantity(
            &raw.geometry.wing.sweep_quarter_chord,
            Dimension::Angle,
        )?,
    };
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
    Ok(Mission {
        id: raw.id,
        name: raw.name,
        payload_mass_kg: positive_quantity(
            &raw.payload.mass,
            Dimension::Mass,
            "mission.payload.mass",
        )?,
        segments,
    })
}

fn resolve_segment(raw: RawMissionSegment, index: usize) -> AexResult<MissionSegment> {
    let path = format!("mission.segments.{index}");
    let kind = segment_kind(&raw.kind, &path)?;
    validate_segment_requirements(kind, &raw, &path)?;
    Ok(MissionSegment {
        id: raw.id,
        kind,
        duration_s: optional_quantity(raw.duration.as_deref(), Dimension::Time)?,
        distance_m: optional_quantity(raw.distance.as_deref(), Dimension::Length)?,
        target_altitude_m: optional_quantity(raw.target_altitude.as_deref(), Dimension::Length)?,
        altitude_m: optional_quantity(raw.altitude.as_deref(), Dimension::Length)?,
        indicated_airspeed_m_s: optional_quantity(
            raw.indicated_airspeed.as_deref(),
            Dimension::Speed,
        )?,
        true_airspeed_m_s: optional_quantity(raw.true_airspeed.as_deref(), Dimension::Speed)?,
        mach: raw.mach,
        power_fraction: optional_fraction(raw.power_fraction, &format!("{path}.power_fraction"))?,
        thrust_fraction: optional_fraction(
            raw.thrust_fraction,
            &format!("{path}.thrust_fraction"),
        )?,
        fuel_fraction: optional_fraction(raw.fuel_fraction, &format!("{path}.fuel_fraction"))?,
        fuel_mass_kg: optional_quantity(raw.fuel_mass.as_deref(), Dimension::Mass)?,
    })
}

fn segment_kind(value: &str, path: &str) -> AexResult<SegmentKind> {
    match value {
        "start_and_taxi" => Ok(SegmentKind::StartAndTaxi),
        "fixed_time" => Ok(SegmentKind::FixedTime),
        "fixed_fuel" => Ok(SegmentKind::FixedFuel),
        "takeoff" => Ok(SegmentKind::Takeoff),
        "climb" => Ok(SegmentKind::Climb),
        "cruise" => Ok(SegmentKind::Cruise),
        "loiter" => Ok(SegmentKind::Loiter),
        "descent" => Ok(SegmentKind::Descent),
        "landing" => Ok(SegmentKind::Landing),
        "reserve" => Ok(SegmentKind::Reserve),
        _ => Err(AexError::validation(
            "UNSUPPORTED_SEGMENT_TYPE",
            path,
            format!("unsupported segment type {value}"),
        )),
    }
}

fn validate_segment_requirements(
    kind: SegmentKind,
    raw: &RawMissionSegment,
    path: &str,
) -> AexResult<()> {
    let timed = matches!(
        kind,
        SegmentKind::StartAndTaxi
            | SegmentKind::FixedTime
            | SegmentKind::Takeoff
            | SegmentKind::Loiter
            | SegmentKind::Reserve
    );
    if timed && raw.duration.is_none() {
        return Err(AexError::validation(
            "MISSING_SEGMENT_DURATION",
            path,
            "segment requires duration",
        ));
    }
    if kind == SegmentKind::Cruise && raw.distance.is_none() {
        return Err(AexError::validation(
            "MISSING_SEGMENT_DISTANCE",
            path,
            "cruise segment requires distance",
        ));
    }
    Ok(())
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

//! Assemble the native performance models' resolved inputs from vehicle ports and catalog data.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ace_model::{AnalysisResources, CatalogEntry, Quantity};
use serde::de::DeserializeOwned;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::{
    Aircraft, AircraftGeometry, ComponentMassStatement, ConceptMetadata, EngineProfile,
    FieldConditions, MassProperties, MassPropertyProvenance, MassPropertyValue, Mission,
    Propulsion, RawAerodynamics, RawLimits, RawProfile, Requirements, ResolvedScenario, Wing,
};
use crate::domain::topology::AircraftTopology;
use crate::services::profile_resolution::{parse_engine_profile, parse_propeller_profile};
use crate::services::resolver::{resolve_aerodynamics, resolve_limits};

/// Catalog kinds this adapter reads, besides engine profile kinds.
pub(super) const POLARS: &str = "aerodynamic_polars";
pub(super) const LIMITS: &str = "operating_limits";
pub(super) const PROPELLER: &str = "propeller";

/// Build the resolved aircraft, field and propulsion inputs for one evaluation.
pub(super) fn scenario(
    id: &str,
    inputs: &BTreeMap<String, Quantity>,
    resources: &AnalysisResources,
) -> AexResult<ResolvedScenario> {
    let value = |port: &str| port_value(inputs, port);
    let polars: RawAerodynamics = document(resources, POLARS)?;
    let limits: RawLimits = document(resources, LIMITS)?;
    let engine = engine(resources)?;
    let propeller = optional_document::<RawProfile>(resources, PROPELLER)?
        .map(parse_propeller_profile)
        .transpose()?;
    let area = value("area")?;
    let span = value("span")?;
    let aircraft = Aircraft {
        id: id.to_owned(),
        name: id.to_owned(),
        category: "vehicle".into(),
        configuration: "vehicle".into(),
        propulsion_architecture: "vehicle".into(),
        topology: AircraftTopology {
            version: 1,
            inferred: true,
            components: Vec::new(),
            relationships: Vec::new(),
        },
        metadata: ConceptMetadata {
            purpose: "vehicle performance envelope".into(),
            certification_use: "prohibited".into(),
        },
        mass: MassProperties {
            maximum_takeoff_mass_kg: value("maximum_takeoff_mass")?,
            operating_empty_mass_kg: value("operating_empty_mass")?,
            maximum_payload_mass_kg: value("maximum_payload_mass")?,
            maximum_fuel_mass_kg: value("maximum_fuel_mass")?,
            statement: undeclared_statement(),
        },
        wing: Wing {
            area_m2: area,
            span_m: span,
            aspect_ratio: span * span / area,
            sweep_quarter_chord_rad: value("sweep")?.to_radians(),
            center_body_edge_sweep_rad: None,
            aerodynamic_center_m: None,
        },
        geometry: AircraftGeometry {
            fuselage: None,
            horizontal_tail: None,
            vertical_tail: None,
        },
        aerodynamics: resolve_aerodynamics(polars)?,
        propulsion: Propulsion {
            profile: engine_id(&engine).into(),
            engine_count: whole(value("engine_count")?, "engine_count")?,
            propeller_profile: propeller.as_ref().map(|profile| profile.id.clone()),
            sizing_factor: value("sizing_factor")?,
        },
        limits: resolve_limits(&limits)?,
    };
    Ok(ResolvedScenario {
        id: id.to_owned(),
        name: id.to_owned(),
        mission: Mission {
            id: format!("{id}.performance"),
            name: "Performance envelope conditions".into(),
            payload_mass_kg: aircraft.mass.maximum_payload_mass_kg,
            initial_state: None,
            isa_offset_k: 0.0,
            field: FieldConditions {
                elevation_m: value("field_elevation")?,
                takeoff_mass_kg: Some(value("takeoff_mass")?),
                landing_mass_kg: Some(value("landing_mass")?),
                obstacle_height_m: value("obstacle_height")?,
                braking_coefficient: value("braking_coefficient")?,
                runway_slope: value("runway_slope")?,
                headwind_m_s: value("headwind")?,
            },
            segments: Vec::new(),
        },
        aircraft,
        requirements: Requirements {
            id: format!("{id}.performance"),
            template: None,
            items: Vec::new(),
        },
        engine,
        propeller,
        assumptions: Vec::new(),
        warnings: Vec::new(),
        source_path: PathBuf::new(),
    })
}

fn engine(resources: &AnalysisResources) -> AexResult<EngineProfile> {
    let mut engines = resources
        .catalog
        .values()
        .filter(|entry| !matches!(entry.kind.as_str(), POLARS | LIMITS | PROPELLER));
    let entry = engines.next().ok_or_else(|| missing("engine profile"))?;
    if engines.next().is_some() {
        return Err(invalid("select exactly one engine profile"));
    }
    let raw: RawProfile = decode(entry)?;
    if raw.kind != entry.kind {
        return Err(invalid("catalog kind differs from the profile type"));
    }
    parse_engine_profile(raw)
}

fn engine_id(engine: &EngineProfile) -> &str {
    match engine {
        EngineProfile::Piston(profile) => &profile.id,
        EngineProfile::Turbofan(profile) => &profile.id,
    }
}

fn document<T: DeserializeOwned>(resources: &AnalysisResources, kind: &str) -> AexResult<T> {
    optional_document(resources, kind)?.ok_or_else(|| missing(kind))
}

fn optional_document<T: DeserializeOwned>(
    resources: &AnalysisResources,
    kind: &str,
) -> AexResult<Option<T>> {
    let mut entries = resources
        .catalog
        .values()
        .filter(|entry| entry.kind == kind);
    let entry = entries.next();
    if entries.next().is_some() {
        return Err(invalid(&format!("select at most one {kind} entry")));
    }
    entry.map(decode).transpose()
}

fn decode<T: DeserializeOwned>(entry: &CatalogEntry) -> AexResult<T> {
    serde_json::from_value(entry.document.clone()).map_err(|source| {
        AexError::validation(
            "INVALID_CATALOG_DOCUMENT",
            entry.kind.clone(),
            source.to_string(),
        )
    })
}

fn port_value(inputs: &BTreeMap<String, Quantity>, port: &str) -> AexResult<f64> {
    inputs
        .get(port)
        .map(|quantity| quantity.value)
        .ok_or_else(|| missing(port))
}

fn whole(value: f64, port: &str) -> AexResult<u32> {
    if value.fract() == 0.0 && (1.0..=16.0).contains(&value) {
        Ok(value as u32)
    } else {
        Err(AexError::validation(
            "INVALID_ENGINE_COUNT",
            port,
            "engine count must be a whole number from 1 through 16",
        ))
    }
}

/// The envelope reads no component masses or stations; record them as undeclared.
fn undeclared_statement() -> ComponentMassStatement {
    let undeclared = |unit| MassPropertyValue {
        value: 0.0,
        unit,
        provenance: MassPropertyProvenance {
            kind: "undeclared",
            source: "vehicle performance envelope".into(),
            correlation_id: None,
            correlation_version: None,
            explicitly_provided: false,
        },
    };
    ComponentMassStatement {
        model_id: "performance.undeclared_mass_statement",
        model_version: 1,
        components: Vec::new(),
        fuel_station: undeclared("m"),
        payload_station: undeclared("m"),
        closure_error_kg: 0.0,
    }
}

fn missing(what: &str) -> AexError {
    AexError::validation(
        "MISSING_PERFORMANCE_INPUT",
        what.to_owned(),
        "required by the performance envelope",
    )
}

fn invalid(message: &str) -> AexError {
    AexError::validation("INVALID_PERFORMANCE_CATALOG", "catalog", message.to_owned())
}

//! Decode the solver data an analysis selected from the vehicle catalog.

use ace_model::{AnalysisResources, CatalogEntry};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::domain::capabilities::{ProfileRole, profile_type};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::{EngineProfile, RawAerodynamics, RawLimits, RawProfile};
use crate::services::profile_resolution::{parse_engine_profile, parse_propeller_profile};
use crate::services::resolver::{resolve_aerodynamics, resolve_limits};

/// Catalog kinds read by native analyses, besides engine profile kinds.
pub(super) const POLARS: &str = "aerodynamic_polars";
pub(super) const LIMITS: &str = "operating_limits";
pub(super) const PROPELLER: &str = "propeller";

/// The one selected entry that is not a polar, limit or propeller document.
pub(super) fn engine(resources: &AnalysisResources) -> AexResult<EngineProfile> {
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

pub(super) fn document<T: DeserializeOwned>(
    resources: &AnalysisResources,
    kind: &str,
) -> AexResult<T> {
    optional_document(resources, kind)?.ok_or_else(|| missing(kind))
}

pub(super) fn optional_document<T: DeserializeOwned>(
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

fn missing(what: &str) -> AexError {
    AexError::validation(
        "MISSING_CATALOG_ENTRY",
        what.to_owned(),
        "the analysis must select this catalog entry",
    )
}

fn invalid(message: &str) -> AexError {
    AexError::validation("INVALID_CATALOG_SELECTION", "catalog", message.to_owned())
}

/// How the native adapters read one catalog entry.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum EntryCheck {
    /// A native adapter decodes and resolves the document.
    Decoded { summary: String },
    /// A native adapter reads this kind but rejects the document.
    Rejected { code: String, message: String },
    /// No native adapter reads this kind; only the envelope was validated.
    Unchecked,
}

/// Decode `entry` exactly as the analyses that select it would.
pub(crate) fn check(entry: &CatalogEntry) -> EntryCheck {
    let outcome = match entry.kind.as_str() {
        POLARS => decode::<RawAerodynamics>(entry)
            .and_then(resolve_aerodynamics)
            .map(|polars| format!("{} with clean, takeoff and landing polars", polars.model)),
        LIMITS => decode::<RawLimits>(entry)
            .and_then(|limits| resolve_limits(&limits))
            .map(|_| "operating limits".to_owned()),
        PROPELLER => decode::<RawProfile>(entry)
            .and_then(parse_propeller_profile)
            .map(|profile| profile.id),
        kind if profile_type(kind).is_some_and(|item| item.role == ProfileRole::Engine) => {
            decode::<RawProfile>(entry).and_then(|raw| {
                if raw.kind != entry.kind {
                    return Err(invalid("catalog kind differs from the profile type"));
                }
                let engine = parse_engine_profile(raw)?;
                Ok(engine.profile_id().to_owned())
            })
        }
        _ => return EntryCheck::Unchecked,
    };
    match outcome {
        Ok(summary) => EntryCheck::Decoded { summary },
        Err(error) => {
            let detail = error.detail();
            EntryCheck::Rejected {
                code: detail.code,
                message: detail.message,
            }
        }
    }
}

//! Decode the solver data an analysis selected from the vehicle catalog.

use ace_model::{AnalysisResources, CatalogEntry};
use serde::de::DeserializeOwned;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::{EngineProfile, RawProfile};
use crate::services::profile_resolution::parse_engine_profile;

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

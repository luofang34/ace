//! The reusable solver data a vehicle file carries, and how native analyses read it.

use std::collections::BTreeMap;
use std::path::Path;

use ace_model::{CatalogEntry, Source};
use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::services::synthesis::catalog::{EntryCheck, check};

#[derive(Debug, Serialize)]
pub(crate) struct CatalogListing {
    pub vehicle_id: String,
    pub entries: Vec<ListedEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListedEntry {
    pub id: String,
    pub kind: String,
    pub version: u32,
    pub source: Source,
    pub check: EntryCheck,
    /// Analyses whose resources select this entry.
    pub selected_by: Vec<String>,
}

/// One entry with its full document.
#[derive(Debug, Serialize)]
pub(crate) struct ShownEntry {
    #[serde(flatten)]
    pub listed: ListedEntry,
    pub document: serde_json::Value,
}

/// Every catalog entry of the vehicle at `path`, checked as the analyses would read it.
pub(crate) fn list_blocking(path: &Path) -> AexResult<CatalogListing> {
    let session = super::synthesis::open_blocking(path, &BTreeMap::new())?;
    let model = session.model();
    let entries = model
        .catalog
        .iter()
        .map(|(id, entry)| listed(model, id, entry))
        .collect();
    Ok(CatalogListing {
        vehicle_id: model.id.clone(),
        entries,
    })
}

/// One catalog entry and its document.
pub(crate) fn show_blocking(path: &Path, id: &str) -> AexResult<ShownEntry> {
    let session = super::synthesis::open_blocking(path, &BTreeMap::new())?;
    let model = session.model();
    let entry = model.catalog.get(id).ok_or_else(|| {
        AexError::validation(
            "UNKNOWN_CATALOG_ENTRY",
            id.to_owned(),
            format!(
                "the vehicle declares no catalog entry with this identifier; entries: {}",
                model.catalog.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        )
    })?;
    Ok(ShownEntry {
        listed: listed(model, id, entry),
        document: entry.document.clone(),
    })
}

fn listed(model: &ace_model::VehicleModel, id: &str, entry: &CatalogEntry) -> ListedEntry {
    ListedEntry {
        id: id.to_owned(),
        kind: entry.kind.clone(),
        version: entry.version,
        source: entry.source.clone(),
        check: check(entry),
        selected_by: model
            .analyses
            .iter()
            .filter(|(_, analysis)| analysis.resources.catalog.iter().any(|item| item == id))
            .map(|(name, _)| name.clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests;

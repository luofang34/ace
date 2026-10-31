use crate::{ModelError, ModelResult, Source};
use serde::{Deserialize, Serialize};

/// Versioned reusable data, such as an engine or propeller profile, that only the solvers
/// selecting it interpret; its whole content enters their calculation evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    /// Document type understood by consuming solvers, e.g. `piston_engine`.
    pub kind: String,
    /// Positive document schema version for `kind`.
    pub version: u32,
    /// Origin of the data.
    pub source: Source,
    /// Kind-specific document; solvers reject shapes they do not support.
    pub document: serde_json::Value,
}

impl CatalogEntry {
    /// Check the envelope; the document itself is validated by the solver that reads it.
    pub fn validate(&self, id: &str) -> ModelResult<()> {
        if id.trim().is_empty() || self.kind.trim().is_empty() {
            return Err(ModelError::invalid(
                "catalog",
                "entry identifiers and kinds must be nonempty",
            ));
        }
        if self.version == 0 || !self.document.is_object() {
            return Err(ModelError::invalid(
                id,
                "catalog entries need a positive version and an object document",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

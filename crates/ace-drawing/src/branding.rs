use crate::{DrawingError, DrawingResult};
use serde::{Deserialize, Serialize};

/// Customer-facing text, independent of the authoritative vehicle and solver evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Branding {
    /// Organization or product name; empty leaves the drawing unbranded.
    pub organization: String,
    /// Drawing title. Standard PDF fonts currently support printable ASCII only.
    pub title: String,
}

impl Default for Branding {
    fn default() -> Self {
        Self {
            organization: String::new(),
            title: "Aircraft Synthesis Drawing".into(),
        }
    }
}

impl Branding {
    pub(crate) fn heading(&self) -> String {
        if self.organization.is_empty() {
            self.title.clone()
        } else {
            format!("{} / {}", self.organization, self.title)
        }
    }

    pub(crate) fn validate(&self) -> DrawingResult<()> {
        if self.title.trim().is_empty()
            || self.heading().len() > 90
            || !self
                .heading()
                .bytes()
                .all(|c| c.is_ascii_graphic() || c == b' ')
        {
            return Err(DrawingError::Invalid("branding requires a nonempty title and at most 90 printable ASCII characters across the heading; embedded Unicode fonts and logos are unavailable".into()));
        }
        Ok(())
    }
}

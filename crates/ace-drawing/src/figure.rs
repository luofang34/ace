//! Caller-sized documentary figures derived from the same frozen drawing input.

use serde::{Deserialize, Serialize};

/// Documentary fact supplied by the author, without inferred engineering meaning.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentaryFact {
    /// Stable identifier selected by figure requests.
    pub id: String,
    /// English field name, including the applicable mass or power basis.
    pub label: String,
    /// Exact supplied text; absent renders N/C.
    pub value: Option<String>,
    /// Evidence register key carrying source, condition and maturity.
    pub evidence: String,
}

/// Supplied diagram linework in normalized page coordinates, without inferred relationships.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramPath {
    /// Points in the inclusive 0..1 content region.
    pub points: Vec<[f64; 2]>,
    /// Whether the last point connects to the first.
    pub closed: bool,
    /// Evidence register key.
    pub evidence: String,
}

/// Supplied diagram annotation, including interface or responsibility declarations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramLabel {
    /// Normalized content-region position.
    pub at: [f64; 2],
    /// English documentary text.
    pub text: String,
    /// Evidence register key.
    pub evidence: String,
}

/// Content selected for an independently composed figure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FigureContent {
    /// Three coordinated views of the supplied/canonical geometry.
    GeneralArrangement,
    /// Existing imported curves sharing compatible axes, in supplied order.
    Curves {
        /// Curve identifiers in the drawing data.
        ids: Vec<String>,
    },
    /// Documentary fields, with absent values explicitly shown as N/C.
    Facts {
        /// Documentary fact identifiers.
        ids: Vec<String>,
    },
    /// Author-supplied documentary diagram; no system design is generated.
    Diagram {
        /// Supplied connections or boundaries.
        paths: Vec<DiagramPath>,
        /// Supplied component/interface labels.
        labels: Vec<DiagramLabel>,
    },
}

/// Presentation slot supplied by the caller; identifiers are never platform-specific defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FigureSpec {
    /// Safe file stem and caller's figure identifier.
    pub id: String,
    /// English title.
    pub title: String,
    /// Target display width in pixels; PNG export is four times this width.
    pub width_px: u32,
    /// Target display height in pixels; PNG export is four times this height.
    pub height_px: u32,
    /// Document/slide/reference association supplied by the caller.
    pub reference: String,
    /// Supplied content to present.
    pub content: FigureContent,
}

/// Compose all requested figures without executing engineering solvers.
pub fn compose_figures(
    document: &crate::DrawingDocument,
) -> crate::DrawingResult<Vec<crate::DrawingDocument>> {
    document.verify()?;
    document
        .input
        .data
        .iter()
        .flat_map(|d| &d.figures)
        .map(|spec| crate::document::figure(document, spec))
        .collect()
}

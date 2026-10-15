use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Evidence attached to an annotation, plotted curve or geometry object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Model content identity; study samples carry their own candidate identity.
    pub model_digest: String,
    /// Source model revision.
    pub revision: u64,
    /// Mission, operating case, candidate or layout context.
    pub scenario: String,
    /// Exact producing run, when computed.
    pub run_id: Option<String>,
    /// Canonical parameter, geometry or requirement identifier.
    pub source: String,
    /// Explicit validity and fidelity label.
    pub quality: String,
    /// Physical units represented by this drawing entity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<ace_model::Unit>,
    /// Supplied evidence category, readiness and revision binding when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported: Option<crate::SourceEvidence>,
}

/// Engineering line semantics shared by all output formats.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stroke {
    /// Native conceptual outer surface, solid dark line.
    Outline,
    /// Declared installation volume, dashed muted line.
    Envelope,
    /// Centerline or station, thin dash-dot line.
    Datum,
    /// Calculated curve or selected state.
    Data,
    /// Violated hard constraint.
    Failure,
    /// Drawing framework, dimensions or unvalidated limits.
    Annotation,
    /// Supplied structural member, never inferred from an occupancy envelope.
    Structure,
    /// Hidden geometry explicitly declared in the source data.
    Hidden,
    /// Section material hatch line.
    Hatch,
}

/// Renderer-independent vector primitive in page millimetres, origin at top left.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Primitive {
    /// Polyline or closed polygon, with no solid fill.
    Path {
        /// Ordered page coordinates.
        points: Vec<[f64; 2]>,
        /// Connect last vertex to first.
        closed: bool,
        /// Semantic line appearance.
        stroke: Stroke,
    },
    /// Single-line, left-aligned text at a baseline.
    Text {
        /// Baseline origin in page millimetres.
        at: [f64; 2],
        /// Literal readable text.
        text: String,
        /// Font size in millimetres.
        size: f64,
        /// Bold heading or status.
        bold: bool,
        /// Text color semantics.
        stroke: Stroke,
    },
}

/// Inspectable object with provenance, not a separate authority for design values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    /// Stable layout or semantic identifier.
    pub id: String,
    /// Key in the scene's evidence index.
    pub evidence: String,
    /// Shared vector content.
    pub primitive: Primitive,
}

/// Single-page engineering scene consumed by SVG, PDF and screen previews.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    /// Physical sheet width.
    pub width_mm: f64,
    /// Physical sheet height.
    pub height_mm: f64,
    /// Main orthographic scale denominator, printed at 100 percent.
    pub scale_denominator: f64,
    /// Readable drawing elements in painting order.
    pub elements: Vec<Element>,
    /// Exact source identities of every element.
    pub evidence: BTreeMap<String, Evidence>,
}

impl Scene {
    pub(crate) fn path(
        &mut self,
        id: &str,
        points: Vec<[f64; 2]>,
        closed: bool,
        stroke: Stroke,
        evidence: &str,
    ) {
        self.elements.push(Element {
            id: format!("{id}-{}", self.elements.len()),
            evidence: evidence.into(),
            primitive: Primitive::Path {
                points,
                closed,
                stroke,
            },
        });
    }

    pub(crate) fn text(&mut self, at: [f64; 2], text: impl Into<String>, evidence: &str) {
        self.label(at, text, 2.5, false, Stroke::Annotation, evidence);
    }

    pub(crate) fn label(
        &mut self,
        at: [f64; 2],
        text: impl Into<String>,
        size: f64,
        bold: bool,
        stroke: Stroke,
        evidence: &str,
    ) {
        self.elements.push(Element {
            id: format!("label-{}", self.elements.len()),
            evidence: evidence.into(),
            primitive: Primitive::Text {
                at,
                text: text.into(),
                size,
                bold,
                stroke,
            },
        });
    }

    pub(crate) fn line(&mut self, a: [f64; 2], b: [f64; 2], stroke: Stroke, evidence: &str) {
        self.path("line", vec![a, b], false, stroke, evidence);
    }
}

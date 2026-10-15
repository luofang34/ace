//! Typed, revision-bound drawing imports; these declarations do not run engineering solvers.

use ace_model::{Binding, Priority, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod figures;
mod validation;
pub(crate) use validation::{resolve, validate};

/// Source evidence category, independent of readiness and calculation convergence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// Configuration or renderer definition, without measured performance claims.
    Definition,
    /// Numerical analysis supplied by an engineering tool.
    Simulation,
    /// Instrumented bench measurement.
    Bench,
    /// Ground test evidence.
    Ground,
    /// Flight test evidence.
    Flight,
    /// Independently supplied third-party evidence.
    ThirdParty,
    /// Desired future result, not an achieved capability.
    Target,
    /// Evidence category has not been supplied.
    Unknown,
}

/// Declared capability maturity; a converged simulation does not set this value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Maturity {
    /// Demonstration is claimed by the cited external evidence.
    Demonstrated,
    /// Integration is in progress.
    InIntegration,
    /// Future target only.
    FutureTarget,
    /// No readiness evidence supplied.
    Unknown,
}

/// Revision scope of one evidence source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvidenceScope {
    /// Reusable configuration definition; only definition/unknown evidence is permitted.
    CurrentModel,
    /// External results valid only for this exact configuration content and revision.
    Frozen {
        /// Canonical model digest associated with the result.
        model_digest: String,
        /// Canonical model revision associated with the result.
        revision: u64,
    },
}

/// External evidence register entry retained behind the sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidence {
    /// Document, dataset or analysis reference.
    pub reference: String,
    /// Immutable source result or evidence identifier.
    pub result_id: String,
    /// Type of evidence, separate from maturity.
    pub kind: EvidenceKind,
    /// Readiness declaration from the source, never inferred by the renderer.
    pub maturity: Maturity,
    /// Configuration binding.
    pub scope: EvidenceScope,
    /// Operating scenario associated with the evidence.
    pub scenario: String,
    /// Optional state identifier.
    pub state: Option<String>,
    /// Optional structural or operating load-case identifier.
    pub load_case: Option<String>,
}

/// Semantics of supplied geometry; an occupancy box is never inferred to be structure.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathRole {
    /// Supplied exterior linework.
    Outline,
    /// Supplied frame, bulkhead, spar or attachment linework.
    Structure,
    /// Declared occupied volume or installation boundary.
    Occupancy,
    /// Hidden geometry explicitly identified by its source.
    Hidden,
    /// Closed material polygon in a section plane, eligible for hatching.
    SectionMaterial,
}

/// Supplied polyline in vehicle coordinates: x aft, y right, z up.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryPath {
    /// Unique identifier, also usable as an annotation target.
    pub id: String,
    /// Human-readable member or equipment name.
    pub label: String,
    /// Explicit drawing semantics.
    pub role: PathRole,
    /// Ordered x/y/z positions; every coordinate must be a length.
    pub points: Vec<[Binding; 3]>,
    /// Join the last point to the first.
    pub closed: bool,
    /// Key in the external evidence register.
    pub evidence: String,
}

/// A genuine x-normal section cut through supplied/native geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionCut {
    /// Section identifier used by the cut marks and detail view.
    pub id: String,
    /// Longitudinal station in compatible length units.
    pub station: Binding,
    /// Canonical surface objects to cut; empty selects the available body surface.
    #[serde(default)]
    pub objects: Vec<String>,
    /// Evidence reference for the section selection.
    pub evidence: String,
}

/// Geometry additions to the canonical vehicle projection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryData {
    /// Only explicitly supplied linework is added to the native geometry.
    #[serde(default)]
    pub paths: Vec<GeometryPath>,
    /// Empty selects three labelled cuts through the available body geometry.
    #[serde(default)]
    pub sections: Vec<SectionCut>,
}

/// Physical axis label and unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Axis {
    /// English axis description.
    pub label: String,
    /// Unit required of every coordinate on this axis.
    pub unit: Unit,
}

/// Spatial meaning of a supplied result curve.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CurveRole {
    /// Cross-sectional area aligned to the longitudinal station axis.
    SectionArea,
    /// CG position against mass.
    CgTrajectory,
    /// Declared permissible CG/mass boundary, not a simulated trajectory.
    CgEnvelope,
    /// Fuel or energy against the declared state axis.
    Energy,
    /// Other supplied calculation plot with explicit physical axes.
    General,
}

/// One curve point with a source state identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    /// Horizontal physical quantity.
    pub x: Binding,
    /// Vertical physical quantity.
    pub y: Binding,
    /// State, station or load-case identifier in the source result.
    pub state: String,
}

/// A supplied engineering result curve; the renderer performs no analysis fit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultCurve {
    /// Unique curve identifier.
    pub id: String,
    /// Compact English curve title.
    pub title: String,
    /// Intended position and semantics on the drawing.
    pub role: CurveRole,
    /// Horizontal axis.
    pub x_axis: Axis,
    /// Vertical axis.
    pub y_axis: Axis,
    /// Ordered source points; all points are retained.
    pub points: Vec<CurvePoint>,
    /// Source result/evidence reference.
    pub evidence: String,
}

/// Supplied margin disposition, separate from provisional evidence quality.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    /// Declared pass from the source.
    Pass,
    /// Declared warning or low margin.
    Warn,
    /// Declared violation.
    Fail,
    /// No supported disposition.
    Unknown,
    /// Requirement or check does not apply.
    NotApplicable,
}

/// A source-provided check; desired attributes need not be hard constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultMargin {
    /// Disposition declared by the source.
    pub status: ResultStatus,
    /// Hard, soft or report-only semantics explicitly chosen by the author.
    pub priority: Priority,
    /// Provisional evidence flag, independent of the status.
    pub provisional: bool,
    /// Optional unit-bearing margin supplied by the source.
    pub margin: Option<Quantity>,
}

/// A compact supplied statement or engineering result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultValue {
    /// English result label.
    pub label: String,
    /// Missing means N/C, never zero.
    pub value: Option<Binding>,
    /// Unit expected for the value.
    pub unit: Unit,
    /// Optional source-provided check; the renderer does not infer compliance.
    pub check: Option<ResultMargin>,
    /// Optional exact equation text supplied by the engineering source.
    pub equation: Option<String>,
    /// Evidence register key.
    pub evidence: String,
}

/// A result attached to a particular member, section or geometric locus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultCallout {
    /// Stable callout identifier.
    pub id: String,
    /// Canonical geometry or supplied path identifier.
    pub target: String,
    /// Vehicle-coordinate anchor with length units.
    pub anchor: [Binding; 3],
    /// Result, unit and source declaration.
    pub result: ResultValue,
}

/// Imported numerical presentation data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultData {
    /// Supplied engineering curves.
    #[serde(default)]
    pub curves: Vec<ResultCurve>,
    /// Member/section-linked result callouts.
    #[serde(default)]
    pub callouts: Vec<ResultCallout>,
    /// Compact statements such as mass, volume, cost basis or readiness.
    #[serde(default)]
    pub statements: Vec<ResultValue>,
}

/// Typed drawing profile and imports, frozen with the same canonical input snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingData {
    /// Optional document/solicitation reference; this never creates requirements.
    #[serde(default)]
    pub document_reference: String,
    /// Supplied geometric entities and section stations.
    #[serde(default)]
    pub geometry: GeometryData,
    /// Supplied engineering results and statements.
    #[serde(default)]
    pub results: ResultData,
    /// External evidence register, retained behind the sheet.
    #[serde(default)]
    pub evidence: BTreeMap<String, SourceEvidence>,
    /// Canonical parameters selected for the compact configuration statement.
    #[serde(default)]
    pub configuration_parameters: Vec<String>,
    /// Supplied documentary facts, such as interface names or integration status.
    #[serde(default)]
    pub facts: Vec<crate::DocumentaryFact>,
    /// Independently composed presentation figures with caller-defined identifiers.
    #[serde(default)]
    pub figures: Vec<crate::FigureSpec>,
}

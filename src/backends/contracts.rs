use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::domain::diagnostic::AexResult;
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{MissionPowerScreen, RequirementEvaluation, StructuralScreen};
use crate::domain::schema::ResolvedScenario;

pub(crate) use crate::domain::result::ResultProvenance;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BackendDescriptor {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) capabilities: Vec<String>,
    pub(crate) unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GeometryMetrics {
    pub(crate) wing_area: QuantityOutput,
    pub(crate) wing_span: QuantityOutput,
    pub(crate) mean_aerodynamic_chord: QuantityOutput,
    pub(crate) horizontal_tail_area: QuantityOutput,
    pub(crate) vertical_tail_area: QuantityOutput,
    pub(crate) wetted_area: QuantityOutput,
    pub(crate) aspect_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GeometryOutput {
    pub(crate) metrics: GeometryMetrics,
    pub(crate) artifact_path: Option<PathBuf>,
    pub(crate) provenance: ResultProvenance,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PolarPoint {
    pub(crate) angle_of_attack_deg: Option<f64>,
    pub(crate) lift_coefficient: f64,
    pub(crate) drag_coefficient: f64,
    pub(crate) pitching_moment_coefficient: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StabilitySummary {
    pub(crate) pitching_moment_slope_per_deg: Option<f64>,
    pub(crate) statically_stable: Option<bool>,
    pub(crate) note: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AnalysisOutput {
    pub(crate) metrics: BTreeMap<String, QuantityOutput>,
    pub(crate) polar: Vec<PolarPoint>,
    pub(crate) stability: StabilitySummary,
    pub(crate) structural_screen: Option<StructuralScreen>,
    pub(crate) mission_power_screen: Option<MissionPowerScreen>,
    pub(crate) requirements: Vec<RequirementEvaluation>,
    pub(crate) feasible: Option<bool>,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) provenance: ResultProvenance,
}

pub(crate) struct GeometryRequest<'a> {
    pub(crate) scenario: &'a ResolvedScenario,
    pub(crate) artifact_path: Option<&'a Path>,
}

pub(crate) struct AnalysisRequest<'a> {
    pub(crate) scenario: &'a ResolvedScenario,
    pub(crate) geometry: &'a GeometryOutput,
}

pub(crate) trait GeometryBackend: Send + Sync {
    fn descriptor(&self) -> BackendDescriptor;

    fn generate_geometry_blocking(&self, request: GeometryRequest<'_>)
    -> AexResult<GeometryOutput>;
}

pub(crate) trait AnalysisBackend: Send + Sync {
    fn analyze_blocking(&self, request: AnalysisRequest<'_>) -> AexResult<AnalysisOutput>;
}

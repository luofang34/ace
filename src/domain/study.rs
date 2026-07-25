use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::content_identity::digest_serializable;
use crate::domain::diagnostic::AexResult;
use crate::domain::evidence::CandidateDescriptor;
use crate::domain::schema::{
    AircraftDocument, MissionDocument, ProfileDocument, RequirementsDocument, ScenarioDocument,
};

mod validation;

pub(crate) use validation::validate_study_document;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudyDocument {
    pub(crate) schema_version: u32,
    pub(crate) study: StudyDefinition,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudyDefinition {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) baseline: StudyBaseline,
    #[serde(default)]
    pub(crate) variables: Vec<StudyVariable>,
    #[serde(default)]
    pub(crate) derived_parameters: Vec<StudyDerivedParameter>,
    pub(crate) objectives: Vec<StudyObjective>,
    #[serde(default)]
    pub(crate) constraints: Vec<StudyConstraint>,
    #[serde(default)]
    pub(crate) selected_candidates: Vec<SelectedCandidateSnapshot>,
    #[serde(default)]
    pub(crate) analysis: StudyAnalysisPolicy,
    #[serde(default)]
    pub(crate) search: StudySearchPolicy,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudyBaseline {
    #[serde(default)]
    pub(crate) scenario_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) embedded: Option<EmbeddedStudyBaseline>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct EmbeddedStudyBaseline {
    pub(crate) scenario: ScenarioDocument,
    pub(crate) aircraft: AircraftDocument,
    pub(crate) mission: MissionDocument,
    pub(crate) requirements: RequirementsDocument,
    #[serde(default)]
    pub(crate) profiles: Vec<ProfileDocument>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudyVariable {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) kind: StudyVariableKind,
    pub(crate) values: Vec<String>,
    #[serde(default)]
    pub(crate) active_when: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StudyVariableKind {
    Continuous,
    Integer,
    Categorical,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudyDerivedParameter {
    pub(crate) target: String,
    pub(crate) method: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudyObjective {
    pub(crate) id: String,
    pub(crate) metric: String,
    pub(crate) direction: ObjectiveDirection,
    #[serde(default = "one")]
    pub(crate) weight: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ObjectiveDirection {
    Minimize,
    Maximize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudyConstraint {
    pub(crate) id: String,
    pub(crate) metric: String,
    pub(crate) operator: String,
    pub(crate) value: String,
    #[serde(default = "hard")]
    pub(crate) severity: String,
    #[serde(default = "one")]
    pub(crate) weight: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SelectedCandidateSnapshot {
    pub(crate) candidate: CandidateDescriptor,
    #[serde(default)]
    pub(crate) evaluation_ids: Vec<String>,
    #[serde(default)]
    pub(crate) note: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudyAnalysisPolicy {
    #[serde(default = "native")]
    pub(crate) screening_backend: String,
    #[serde(default)]
    pub(crate) refinement_backend: Option<String>,
    #[serde(default = "default_refinement_limit")]
    pub(crate) refinement_candidate_limit: u32,
}

impl Default for StudyAnalysisPolicy {
    fn default() -> Self {
        Self {
            screening_backend: native(),
            refinement_backend: None,
            refinement_candidate_limit: default_refinement_limit(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StudySearchPolicy {
    #[serde(default = "grid")]
    pub(crate) strategy: String,
    #[serde(default = "default_max_evaluations")]
    pub(crate) max_evaluations: u32,
    #[serde(default = "default_population")]
    pub(crate) population: u32,
    #[serde(default = "default_generations")]
    pub(crate) generations: u32,
    #[serde(default = "default_mutation_rate")]
    pub(crate) mutation_rate: f64,
    #[serde(default)]
    pub(crate) seed: u64,
}

impl Default for StudySearchPolicy {
    fn default() -> Self {
        Self {
            strategy: grid(),
            max_evaluations: default_max_evaluations(),
            population: default_population(),
            generations: default_generations(),
            mutation_rate: default_mutation_rate(),
            seed: 0,
        }
    }
}

pub(crate) fn study_digest(document: &StudyDocument) -> AexResult<String> {
    validate_study_document(document)?;
    digest_serializable(document)
}

fn one() -> f64 {
    1.0
}

fn hard() -> String {
    "hard".to_owned()
}

fn native() -> String {
    "native".to_owned()
}

fn grid() -> String {
    "grid".to_owned()
}

fn default_refinement_limit() -> u32 {
    3
}

fn default_max_evaluations() -> u32 {
    256
}

fn default_population() -> u32 {
    24
}

fn default_generations() -> u32 {
    12
}

fn default_mutation_rate() -> f64 {
    0.15
}

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::domain::content_identity::{
    content_id, validate_content_id, validate_digest, validate_safe_id,
};
use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;

mod archive;

pub(crate) use archive::StudyArchive;
#[cfg(test)]
pub(crate) use archive::StudyArchiveDraft;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvaluationStatus {
    Succeeded,
    Failed,
    Unsupported,
    NotPerformed,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConstraintStatus {
    Pass,
    Fail,
    Indeterminate,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct CandidateDescriptor {
    pub(crate) candidate_id: String,
    pub(crate) baseline_digest: String,
    pub(crate) parameters: BTreeMap<String, String>,
}

impl CandidateDescriptor {
    pub(crate) fn new(
        baseline_digest: String,
        parameters: BTreeMap<String, String>,
    ) -> AexResult<Self> {
        validate_digest(&baseline_digest, "candidate.baseline_digest")?;
        validate_parameters(&parameters)?;
        let candidate_id = candidate_content_id(&baseline_digest, &parameters)?;
        Ok(Self {
            candidate_id,
            baseline_digest,
            parameters,
        })
    }

    pub(crate) fn validate(&self) -> AexResult<()> {
        validate_digest(&self.baseline_digest, "candidate.baseline_digest")?;
        validate_content_id(&self.candidate_id, "candidate_", "candidate.candidate_id")?;
        validate_parameters(&self.parameters)?;
        let expected = candidate_content_id(&self.baseline_digest, &self.parameters)?;
        require_matching_id(&self.candidate_id, &expected, "candidate.candidate_id")
    }
}

#[derive(Serialize)]
struct CandidateIdentity<'a> {
    schema_version: u32,
    baseline_digest: &'a str,
    parameters: &'a BTreeMap<String, String>,
}

fn candidate_content_id(
    baseline_digest: &str,
    parameters: &BTreeMap<String, String>,
) -> AexResult<String> {
    content_id(
        "candidate_",
        &CandidateIdentity {
            schema_version: 1,
            baseline_digest,
            parameters,
        },
    )
}

fn validate_parameters(parameters: &BTreeMap<String, String>) -> AexResult<()> {
    if parameters
        .iter()
        .any(|(path, value)| path.trim().is_empty() || value.trim().is_empty())
    {
        return Err(AexError::validation(
            "INVALID_CANDIDATE_PARAMETER",
            "candidate.parameters",
            "parameter paths and values must be non-empty",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct EvidenceConstraint {
    pub(crate) id: String,
    pub(crate) metric: String,
    pub(crate) status: ConstraintStatus,
    pub(crate) severity: String,
    pub(crate) actual: Option<QuantityOutput>,
    pub(crate) required: Option<QuantityOutput>,
    pub(crate) operator: String,
    pub(crate) normalized_violation: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct EvidenceArtifact {
    pub(crate) kind: String,
    pub(crate) digest: String,
    pub(crate) location: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct EvidenceAnalysis {
    pub(crate) discipline: String,
    pub(crate) operating_condition: String,
    pub(crate) method: String,
    pub(crate) backend: String,
    pub(crate) software_version: String,
    pub(crate) model_versions: BTreeMap<String, String>,
    pub(crate) fidelity_level: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct EvidenceResults {
    pub(crate) metrics: BTreeMap<String, QuantityOutput>,
    pub(crate) constraints: Vec<EvidenceConstraint>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct EvidenceProvenance {
    pub(crate) assumptions: Vec<String>,
    pub(crate) validity_range: Vec<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) dependencies: Vec<String>,
    pub(crate) artifacts: Vec<EvidenceArtifact>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvidenceDraft {
    pub(crate) study_id: String,
    pub(crate) candidate_id: String,
    pub(crate) input_digest: String,
    pub(crate) status: EvaluationStatus,
    pub(crate) analysis: EvidenceAnalysis,
    pub(crate) results: EvidenceResults,
    pub(crate) provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct EvidenceEnvelope {
    pub(crate) schema_version: u32,
    pub(crate) evaluation_id: String,
    pub(crate) study_id: String,
    pub(crate) candidate_id: String,
    pub(crate) input_digest: String,
    pub(crate) status: EvaluationStatus,
    pub(crate) analysis: EvidenceAnalysis,
    pub(crate) results: EvidenceResults,
    pub(crate) provenance: EvidenceProvenance,
}

impl EvidenceEnvelope {
    pub(crate) fn from_draft(draft: EvidenceDraft) -> AexResult<Self> {
        validate_evidence_draft(&draft)?;
        let evaluation_id = content_id("eval_", &draft)?;
        Ok(Self {
            schema_version: 1,
            evaluation_id,
            study_id: draft.study_id,
            candidate_id: draft.candidate_id,
            input_digest: draft.input_digest,
            status: draft.status,
            analysis: draft.analysis,
            results: draft.results,
            provenance: draft.provenance,
        })
    }

    pub(crate) fn validate(&self) -> AexResult<()> {
        if self.schema_version != 1 {
            return Err(AexError::validation(
                "UNSUPPORTED_SCHEMA_VERSION",
                "evidence.schema_version",
                format!("expected 1, got {}", self.schema_version),
            ));
        }
        validate_content_id(&self.evaluation_id, "eval_", "evidence.evaluation_id")?;
        let draft = self.as_draft();
        validate_evidence_draft(&draft)?;
        let expected = content_id("eval_", &draft)?;
        require_matching_id(&self.evaluation_id, &expected, "evidence.evaluation_id")
    }

    fn as_draft(&self) -> EvidenceDraft {
        EvidenceDraft {
            study_id: self.study_id.clone(),
            candidate_id: self.candidate_id.clone(),
            input_digest: self.input_digest.clone(),
            status: self.status.clone(),
            analysis: self.analysis.clone(),
            results: self.results.clone(),
            provenance: self.provenance.clone(),
        }
    }
}

fn validate_evidence_draft(draft: &EvidenceDraft) -> AexResult<()> {
    validate_safe_id(&draft.study_id, "evidence.study_id")?;
    validate_content_id(&draft.candidate_id, "candidate_", "evidence.candidate_id")?;
    validate_digest(&draft.input_digest, "evidence.input_digest")?;
    validate_analysis(&draft.analysis)?;
    validate_results(&draft.results)?;
    validate_provenance(&draft.provenance)
}

fn validate_analysis(analysis: &EvidenceAnalysis) -> AexResult<()> {
    let required = [
        analysis.discipline.as_str(),
        analysis.operating_condition.as_str(),
        analysis.method.as_str(),
        analysis.backend.as_str(),
        analysis.software_version.as_str(),
    ];
    if required.iter().any(|value| value.trim().is_empty())
        || analysis
            .model_versions
            .iter()
            .any(|(model, version)| model.trim().is_empty() || version.trim().is_empty())
    {
        return Err(AexError::validation(
            "INVALID_EVIDENCE_ANALYSIS",
            "evidence.analysis",
            "analysis identity, backend, software, and model versions must be non-empty",
        ));
    }
    Ok(())
}

fn validate_results(results: &EvidenceResults) -> AexResult<()> {
    for (metric, quantity) in &results.metrics {
        validate_quantity(quantity, &format!("evidence.results.metrics.{metric}"))?;
    }
    let mut ids = BTreeSet::new();
    for constraint in &results.constraints {
        validate_safe_id(&constraint.id, "evidence.results.constraints[].id")?;
        if constraint.metric.trim().is_empty()
            || constraint.severity.trim().is_empty()
            || constraint.operator.trim().is_empty()
            || !constraint.normalized_violation.is_finite()
            || constraint.normalized_violation < 0.0
            || !ids.insert(constraint.id.as_str())
        {
            return Err(AexError::validation(
                "INVALID_EVIDENCE_CONSTRAINT",
                "evidence.results.constraints",
                "constraints require unique identities and nonnegative finite violation",
            ));
        }
        if let Some(actual) = &constraint.actual {
            validate_quantity(actual, "evidence.results.constraints[].actual")?;
        }
        if let Some(required) = &constraint.required {
            validate_quantity(required, "evidence.results.constraints[].required")?;
        }
    }
    Ok(())
}

fn validate_quantity(quantity: &QuantityOutput, path: &str) -> AexResult<()> {
    if quantity.value.is_finite()
        && quantity.display_value.is_finite()
        && !quantity.unit.trim().is_empty()
        && !quantity.display_unit.trim().is_empty()
    {
        Ok(())
    } else {
        Err(AexError::validation(
            "INVALID_EVIDENCE_QUANTITY",
            path,
            "quantity values must be finite and units must be non-empty",
        ))
    }
}

fn validate_provenance(provenance: &EvidenceProvenance) -> AexResult<()> {
    if provenance
        .confidence
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(AexError::validation(
            "INVALID_EVIDENCE_CONFIDENCE",
            "evidence.provenance.confidence",
            "confidence must be finite and between zero and one",
        ));
    }
    let mut dependencies = BTreeSet::new();
    for dependency in &provenance.dependencies {
        validate_content_id(dependency, "eval_", "evidence.provenance.dependencies")?;
        if !dependencies.insert(dependency.as_str()) {
            return Err(AexError::validation(
                "DUPLICATE_EVIDENCE_DEPENDENCY",
                "evidence.provenance.dependencies",
                "dependency identifiers must be unique",
            ));
        }
    }
    for artifact in &provenance.artifacts {
        if artifact.kind.trim().is_empty() {
            return Err(AexError::validation(
                "INVALID_EVIDENCE_ARTIFACT",
                "evidence.provenance.artifacts",
                "artifact kind cannot be empty",
            ));
        }
        validate_digest(&artifact.digest, "evidence.provenance.artifacts[].digest")?;
    }
    Ok(())
}

pub(super) fn require_matching_id(actual: &str, expected: &str, path: &str) -> AexResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(AexError::validation(
            "CONTENT_ID_MISMATCH",
            path,
            format!("stored identifier {actual} does not match computed identifier {expected}"),
        ))
    }
}

#[cfg(test)]
mod tests;

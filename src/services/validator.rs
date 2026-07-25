use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_yaml::Value;

use crate::domain::capabilities::{
    DocumentKind, ProfileRole, document_type as capability_document_type, document_types,
    profile_type,
};
use crate::domain::diagnostic::{AexError, AexResult, Diagnostic, Severity};
use crate::domain::schema::{
    AircraftDocument, MissionDocument, ProfileDocument, RequirementsDocument, ScenarioDocument,
};
use crate::domain::study::{StudyDocument, validate_study_document};
use crate::domain::warning::WarningCode;
use crate::services::analysis::ApplicationService;
use crate::services::model_preflight::preflight_model_domains;
use crate::services::profile_resolution::{parse_engine_profile, parse_propeller_profile};
use crate::services::profile_sanity::{engine_profile_warnings, propeller_profile_warnings};
use crate::services::requirement_resolution::resolve_requirements;
use crate::services::resolver::{resolve_aircraft, resolve_embedded_study, resolve_mission};
use crate::storage::project_store::read_yaml_value_blocking;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidationResult {
    pub(crate) valid: bool,
    pub(crate) document_type: String,
    pub(crate) errors: Vec<Diagnostic>,
    pub(crate) warnings: Vec<Diagnostic>,
}

impl ApplicationService {
    pub(crate) fn validate_path_blocking(&self, path: &Path) -> AexResult<ValidationResult> {
        let document = read_yaml_value_blocking(path)?;
        if document.get("scenario").is_some() {
            let resolved = self.resolve_blocking(path, &BTreeMap::new())?;
            return Ok(result_with_warnings("scenario", resolved.warnings));
        }
        if document.get("study").is_some() {
            let typed: StudyDocument = from_value(document, "study")?;
            validate_study_document(&typed)?;
            let warnings = if let Some(relative) = &typed.study.baseline.scenario_path {
                let directory = path.parent().unwrap_or_else(|| Path::new("."));
                self.resolve_blocking(&directory.join(relative), &BTreeMap::new())?
                    .warnings
            } else if let Some(embedded) = &typed.study.baseline.embedded {
                embedded_study_warnings(embedded)?
            } else {
                Vec::new()
            };
            return Ok(result_with_warnings("study", warnings));
        }
        validate_document_value(&document, None)
    }
}

pub(crate) fn validate_document_value(
    document: &Value,
    expected_type: Option<&str>,
) -> AexResult<ValidationResult> {
    let document_type = expected_type
        .map(str::to_owned)
        .or_else(|| detect_type(document))
        .ok_or_else(|| {
            AexError::validation(
                "UNKNOWN_DOCUMENT_TYPE",
                "document",
                "document has no supported envelope key",
            )
        })?;
    let kind = capability_document_type(&document_type).ok_or_else(|| {
        AexError::validation("UNSUPPORTED_DOCUMENT_TYPE", "document_type", &document_type)
    })?;
    match kind.kind {
        DocumentKind::Aircraft => {
            let typed: AircraftDocument = from_value(document.clone(), "aircraft")?;
            let resolved = resolve_aircraft(typed)?;
            if resolved.metadata.certification_use != "prohibited" {
                return Ok(ValidationResult {
                    valid: true,
                    document_type,
                    errors: Vec::new(),
                    warnings: vec![Diagnostic::warning(
                        WarningCode::CertificationUseNotProhibited,
                        "Example concepts should explicitly prohibit certification use.",
                        "aircraft.metadata.certification_use",
                    )],
                });
            }
        }
        DocumentKind::Mission => {
            let typed: MissionDocument = from_value(document.clone(), "mission")?;
            resolve_mission(typed)?;
        }
        DocumentKind::Requirements => {
            let typed: RequirementsDocument = from_value(document.clone(), "requirements")?;
            resolve_requirements(typed)?;
        }
        DocumentKind::Profile => {
            let typed: ProfileDocument = from_value(document.clone(), "profile")?;
            let warnings = match profile_type(&typed.profile.kind).map(|item| item.role) {
                Some(ProfileRole::Propeller) => {
                    propeller_profile_warnings(&parse_propeller_profile(typed.profile)?)
                }
                _ => engine_profile_warnings(&parse_engine_profile(typed.profile)?),
            };
            return Ok(result_with_warnings(&document_type, warnings));
        }
        DocumentKind::Scenario => {
            let _: ScenarioDocument = from_value(document.clone(), "scenario")?;
        }
        DocumentKind::Study => {
            let typed: StudyDocument = from_value(document.clone(), "study")?;
            validate_study_document(&typed)?;
            if let Some(embedded) = &typed.study.baseline.embedded {
                let warnings = embedded_study_warnings(embedded)?;
                return Ok(result_with_warnings(&document_type, warnings));
            }
        }
    }
    Ok(valid_result(&document_type))
}

fn embedded_study_warnings(
    embedded: &crate::domain::study::EmbeddedStudyBaseline,
) -> AexResult<Vec<Diagnostic>> {
    let resolved = resolve_embedded_study(embedded)?;
    preflight_model_domains(&resolved)?;
    Ok(resolved.warnings)
}

pub(crate) fn error_validation(document_type: &str, error: &AexError) -> ValidationResult {
    ValidationResult {
        valid: false,
        document_type: document_type.to_owned(),
        errors: vec![Diagnostic {
            code: "VALIDATION_ERROR".to_owned(),
            severity: Severity::Error,
            message: error.to_string(),
            path: Some("document".to_owned()),
            context: serde_json::Value::Object(serde_json::Map::new()),
        }],
        warnings: Vec::new(),
    }
}

fn from_value<T: serde::de::DeserializeOwned>(value: Value, path: &str) -> AexResult<T> {
    serde_yaml::from_value(value).map_err(|source| AexError::Yaml {
        path: path.into(),
        source,
    })
}

fn detect_type(document: &Value) -> Option<String> {
    document_types()
        .iter()
        .find(|item| document.get(item.id).is_some())
        .map(|item| item.id.to_owned())
}

fn valid_result(document_type: &str) -> ValidationResult {
    result_with_warnings(document_type, Vec::new())
}

fn result_with_warnings(document_type: &str, warnings: Vec<Diagnostic>) -> ValidationResult {
    ValidationResult {
        valid: true,
        document_type: document_type.to_owned(),
        errors: Vec::new(),
        warnings,
    }
}

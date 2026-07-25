use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_yaml::Value;

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic, Severity};
use crate::domain::schema::{
    AircraftDocument, MissionDocument, ProfileDocument, RequirementsDocument,
};
use crate::domain::study::{StudyDocument, validate_study_document};
use crate::services::analysis::ApplicationService;
use crate::services::profile_resolution::parse_engine_profile;
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
            self.resolve_blocking(path, &BTreeMap::new())?;
            return Ok(valid_result("scenario"));
        }
        if document.get("study").is_some() {
            let typed: StudyDocument = from_value(document, "study")?;
            validate_study_document(&typed)?;
            if let Some(relative) = &typed.study.baseline.scenario_path {
                let directory = path.parent().unwrap_or_else(|| Path::new("."));
                self.resolve_blocking(&directory.join(relative), &BTreeMap::new())?;
            } else if let Some(embedded) = &typed.study.baseline.embedded {
                resolve_embedded_study(embedded)?;
            }
            return Ok(valid_result("study"));
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
    match document_type.as_str() {
        "aircraft" => {
            let typed: AircraftDocument = from_value(document.clone(), "aircraft")?;
            let resolved = resolve_aircraft(typed)?;
            if resolved.metadata.certification_use != "prohibited" {
                return Ok(ValidationResult {
                    valid: true,
                    document_type,
                    errors: Vec::new(),
                    warnings: vec![Diagnostic::warning(
                        "CERTIFICATION_USE_NOT_PROHIBITED",
                        "Example concepts should explicitly prohibit certification use.",
                        "aircraft.metadata.certification_use",
                    )],
                });
            }
        }
        "mission" => {
            let typed: MissionDocument = from_value(document.clone(), "mission")?;
            resolve_mission(typed)?;
        }
        "requirements" => {
            let typed: RequirementsDocument = from_value(document.clone(), "requirements")?;
            resolve_requirements(typed)?;
        }
        "profile" => {
            let typed: ProfileDocument = from_value(document.clone(), "profile")?;
            if typed.profile.kind != "propeller" {
                parse_engine_profile(typed.profile)?;
            }
        }
        "study" => {
            let typed: StudyDocument = from_value(document.clone(), "study")?;
            validate_study_document(&typed)?;
            if let Some(embedded) = &typed.study.baseline.embedded {
                resolve_embedded_study(embedded)?;
            }
        }
        _ => {
            return Err(AexError::validation(
                "UNSUPPORTED_DOCUMENT_TYPE",
                "document_type",
                document_type,
            ));
        }
    }
    Ok(valid_result(&document_type))
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
    ["aircraft", "mission", "requirements", "profile", "study"]
        .into_iter()
        .find(|key| document.get(*key).is_some())
        .map(str::to_owned)
}

fn valid_result(document_type: &str) -> ValidationResult {
    ValidationResult {
        valid: true,
        document_type: document_type.to_owned(),
        errors: Vec::new(),
        warnings: Vec::new(),
    }
}

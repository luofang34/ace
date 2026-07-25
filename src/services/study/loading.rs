use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tempfile::TempDir;

use crate::domain::content_identity::digest_serializable;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::{ResolvedScenario, ScenarioDocument};
use crate::domain::study::{EmbeddedStudyBaseline, StudyDocument, validate_study_document};
use crate::services::analysis::ApplicationService;
use crate::storage::project_store::read_yaml_blocking;

pub(super) struct PreparedStudy {
    pub(super) document: StudyDocument,
    pub(super) scenario: ResolvedScenario,
    pub(super) baseline_digest: String,
    pub(super) study_digest: String,
    baseline: BaselineWorkspace,
}

enum BaselineWorkspace {
    Referenced(PathBuf),
    Embedded {
        _directory: TempDir,
        scenario_path: PathBuf,
    },
}

impl PreparedStudy {
    pub(super) fn scenario_path(&self) -> &Path {
        match &self.baseline {
            BaselineWorkspace::Referenced(path) => path,
            BaselineWorkspace::Embedded { scenario_path, .. } => scenario_path,
        }
    }

    pub(super) fn embedded_baseline(&self) -> bool {
        matches!(self.baseline, BaselineWorkspace::Embedded { .. })
    }
}

pub(super) fn prepare_study_blocking(
    service: &ApplicationService,
    path: &Path,
) -> AexResult<PreparedStudy> {
    let document: StudyDocument = read_yaml_blocking(path)?;
    validate_study_document(&document)?;
    validate_screening_policy(&document)?;
    let baseline = prepare_baseline_blocking(path, &document)?;
    let scenario = service.resolve_blocking(baseline.path(), &Default::default())?;
    let baseline_digest = baseline_digest(&scenario)?;
    let study_digest = digest_serializable(&serde_json::json!({
        "document": document,
        "baseline_digest": baseline_digest,
    }))?;
    Ok(PreparedStudy {
        document,
        scenario,
        baseline_digest,
        study_digest,
        baseline: baseline.into_workspace(),
    })
}

fn validate_screening_policy(document: &StudyDocument) -> AexResult<()> {
    if document.study.analysis.screening_backend == "native" {
        Ok(())
    } else {
        Err(AexError::validation(
            "UNSUPPORTED_STUDY_SCREENING_BACKEND",
            "study.analysis.screening_backend",
            "study execution requires native screening",
        ))
    }
}

enum PreparedBaseline {
    Referenced(PathBuf),
    Embedded {
        directory: TempDir,
        scenario_path: PathBuf,
    },
}

impl PreparedBaseline {
    fn path(&self) -> &Path {
        match self {
            Self::Referenced(path) => path,
            Self::Embedded { scenario_path, .. } => scenario_path,
        }
    }

    fn into_workspace(self) -> BaselineWorkspace {
        match self {
            Self::Referenced(path) => BaselineWorkspace::Referenced(path),
            Self::Embedded {
                directory,
                scenario_path,
            } => BaselineWorkspace::Embedded {
                _directory: directory,
                scenario_path,
            },
        }
    }
}

fn prepare_baseline_blocking(
    study_path: &Path,
    document: &StudyDocument,
) -> AexResult<PreparedBaseline> {
    if let Some(relative) = &document.study.baseline.scenario_path {
        let directory = study_path.parent().unwrap_or_else(|| Path::new("."));
        return Ok(PreparedBaseline::Referenced(directory.join(relative)));
    }
    document
        .study
        .baseline
        .embedded
        .as_ref()
        .ok_or_else(|| {
            AexError::validation(
                "INVALID_STUDY_BASELINE",
                "study.baseline",
                "validated study has no baseline",
            )
        })
        .and_then(materialize_embedded_blocking)
}

fn materialize_embedded_blocking(embedded: &EmbeddedStudyBaseline) -> AexResult<PreparedBaseline> {
    let directory = tempfile::Builder::new()
        .prefix("ace-study-")
        .tempdir()
        .map_err(|source| AexError::Write {
            path: std::env::temp_dir(),
            source,
        })?;
    let mut scenario: ScenarioDocument = embedded.scenario.clone();
    scenario.scenario.aircraft = PathBuf::from("aircraft.yaml");
    scenario.scenario.mission = PathBuf::from("mission.yaml");
    scenario.scenario.requirements = PathBuf::from("requirements.yaml");
    write_yaml_blocking(&directory.path().join("scenario.yaml"), &scenario)?;
    write_yaml_blocking(&directory.path().join("aircraft.yaml"), &embedded.aircraft)?;
    write_yaml_blocking(&directory.path().join("mission.yaml"), &embedded.mission)?;
    write_yaml_blocking(
        &directory.path().join("requirements.yaml"),
        &embedded.requirements,
    )?;
    write_profiles_blocking(directory.path(), embedded)?;
    let scenario_path = directory.path().join("scenario.yaml");
    Ok(PreparedBaseline::Embedded {
        directory,
        scenario_path,
    })
}

fn write_profiles_blocking(root: &Path, embedded: &EmbeddedStudyBaseline) -> AexResult<()> {
    let profile_directory = root.join("profiles");
    fs::create_dir(&profile_directory).map_err(|source| AexError::Write {
        path: profile_directory.clone(),
        source,
    })?;
    for (index, profile) in embedded.profiles.iter().enumerate() {
        write_yaml_blocking(
            &profile_directory.join(format!("profile-{index}.yaml")),
            profile,
        )?;
    }
    Ok(())
}

fn write_yaml_blocking<T: Serialize>(path: &Path, value: &T) -> AexResult<()> {
    let data = serde_yaml::to_string(value).map_err(|source| AexError::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, data).map_err(|source| AexError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn baseline_digest(scenario: &ResolvedScenario) -> AexResult<String> {
    digest_serializable(&serde_json::json!({
        "aircraft": scenario.aircraft,
        "mission": scenario.mission,
        "requirements": scenario.requirements,
        "engine": scenario.engine,
        "propeller": scenario.propeller,
    }))
}

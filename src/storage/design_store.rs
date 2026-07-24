use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tempfile::{Builder, NamedTempFile};

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::ScenarioDocument;
use crate::storage::project_store::read_yaml_blocking;

#[derive(Debug, Clone)]
pub(crate) struct CreateDesignSpec<'a> {
    pub(crate) design_id: &'a str,
    pub(crate) display_name: &'a str,
    pub(crate) source_scenario: &'a Path,
    pub(crate) design_root: &'a Path,
    pub(crate) parameters: &'a BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DesignRecord {
    pub(crate) design_id: String,
    pub(crate) display_name: String,
    pub(crate) scenario_path: PathBuf,
    pub(crate) parameters: BTreeMap<String, String>,
}

pub(crate) fn create_design_blocking(spec: CreateDesignSpec<'_>) -> AexResult<DesignRecord> {
    validate_design_id(spec.design_id)?;
    fs::create_dir_all(spec.design_root).map_err(|source| AexError::Write {
        path: spec.design_root.to_path_buf(),
        source,
    })?;
    let target = spec.design_root.join(spec.design_id);
    if target.exists() {
        return Err(AexError::validation(
            "DESIGN_ALREADY_EXISTS",
            target.display().to_string(),
            "choose a new design_id or update the existing design",
        ));
    }
    let temporary = Builder::new()
        .prefix(".ace-design-")
        .tempdir_in(spec.design_root)
        .map_err(|source| AexError::Write {
            path: spec.design_root.to_path_buf(),
            source,
        })?;
    populate_design_blocking(temporary.path(), &spec)?;
    fs::rename(temporary.path(), &target).map_err(|source| AexError::Write {
        path: target.clone(),
        source,
    })?;
    Ok(DesignRecord {
        design_id: spec.design_id.to_owned(),
        display_name: spec.display_name.to_owned(),
        scenario_path: target.join("scenario.yaml"),
        parameters: spec.parameters.clone(),
    })
}

pub(crate) fn update_parameters_blocking(
    scenario_path: &Path,
    updates: &BTreeMap<String, String>,
) -> AexResult<DesignRecord> {
    let mut document: ScenarioDocument = read_yaml_blocking(scenario_path)?;
    document.scenario.overrides.extend(updates.clone());
    write_yaml_atomic_blocking(scenario_path, &document)?;
    Ok(DesignRecord {
        design_id: document.scenario.id,
        display_name: document.scenario.name,
        scenario_path: scenario_path.to_path_buf(),
        parameters: document.scenario.overrides,
    })
}

fn populate_design_blocking(directory: &Path, spec: &CreateDesignSpec<'_>) -> AexResult<()> {
    let mut scenario: ScenarioDocument = read_yaml_blocking(spec.source_scenario)?;
    let source_directory = spec
        .source_scenario
        .parent()
        .unwrap_or_else(|| Path::new("."));
    copy_document_blocking(
        &source_directory.join(&scenario.scenario.aircraft),
        &directory.join("aircraft.yaml"),
    )?;
    copy_document_blocking(
        &source_directory.join(&scenario.scenario.mission),
        &directory.join("mission.yaml"),
    )?;
    copy_document_blocking(
        &source_directory.join(&scenario.scenario.requirements),
        &directory.join("requirements.yaml"),
    )?;
    copy_tree_blocking(
        &source_directory.join("profiles"),
        &directory.join("profiles"),
    )?;
    scenario.scenario.id = spec.design_id.to_owned();
    scenario.scenario.name = spec.display_name.to_owned();
    scenario.scenario.aircraft = PathBuf::from("aircraft.yaml");
    scenario.scenario.mission = PathBuf::from("mission.yaml");
    scenario.scenario.requirements = PathBuf::from("requirements.yaml");
    scenario.scenario.overrides = spec.parameters.clone();
    write_yaml_blocking(&directory.join("scenario.yaml"), &scenario)
}

fn copy_document_blocking(source: &Path, destination: &Path) -> AexResult<()> {
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|source_error| AexError::Write {
            path: destination.to_path_buf(),
            source: source_error,
        })
}

fn copy_tree_blocking(source: &Path, destination: &Path) -> AexResult<()> {
    fs::create_dir_all(destination).map_err(|source_error| AexError::Write {
        path: destination.to_path_buf(),
        source: source_error,
    })?;
    let entries = fs::read_dir(source).map_err(|source_error| AexError::Read {
        path: source.to_path_buf(),
        source: source_error,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source_error| AexError::Read {
            path: source.to_path_buf(),
            source: source_error,
        })?;
        let file_type = entry.file_type().map_err(|source_error| AexError::Read {
            path: entry.path(),
            source: source_error,
        })?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree_blocking(&entry.path(), &target)?;
        } else if file_type.is_file() {
            copy_document_blocking(&entry.path(), &target)?;
        } else {
            return Err(AexError::validation(
                "UNSUPPORTED_PROFILE_ENTRY",
                entry.path().display().to_string(),
                "profile directories cannot contain symbolic links or special files",
            ));
        }
    }
    Ok(())
}

fn write_yaml_blocking<T: Serialize>(path: &Path, value: &T) -> AexResult<()> {
    let serialized = serde_yaml::to_string(value).map_err(|source| AexError::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, serialized).map_err(|source| AexError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn write_yaml_atomic_blocking<T: Serialize>(path: &Path, value: &T) -> AexResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let serialized = serde_yaml::to_string(value).map_err(|source| AexError::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    let mut temporary = NamedTempFile::new_in(parent).map_err(|source| AexError::Write {
        path: parent.to_path_buf(),
        source,
    })?;
    temporary
        .write_all(serialized.as_bytes())
        .and_then(|()| temporary.flush())
        .map_err(|source| AexError::Write {
            path: temporary.path().to_path_buf(),
            source,
        })?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| AexError::Write {
            path: path.to_path_buf(),
            source: error.error,
        })
}

fn validate_design_id(design_id: &str) -> AexResult<()> {
    let valid = !design_id.is_empty()
        && design_id.len() <= 80
        && design_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(AexError::validation(
            "INVALID_DESIGN_ID",
            "design_id",
            "use 1-80 ASCII letters, digits, hyphens, or underscores",
        ))
    }
}

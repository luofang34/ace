use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::presentation::DisplayUnitSystem;

#[derive(Debug, Deserialize)]
struct ProjectDocument {
    project: ProjectSettings,
}

#[derive(Debug, Deserialize)]
struct ProjectSettings {
    default_unit_system: String,
}

pub(crate) fn display_unit_system_blocking(
    scenario_path: &Path,
    explicit: Option<&str>,
) -> AexResult<DisplayUnitSystem> {
    if let Some(value) = explicit {
        return DisplayUnitSystem::parse(value, "units");
    }
    let project_path = scenario_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("aircraft-explorer.yaml");
    match fs::read_to_string(&project_path) {
        Ok(content) => {
            let document: ProjectDocument =
                serde_yaml::from_str(&content).map_err(|source| AexError::Yaml {
                    path: project_path,
                    source,
                })?;
            DisplayUnitSystem::parse(
                &document.project.default_unit_system,
                "project.default_unit_system",
            )
        }
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(DisplayUnitSystem::Si),
        Err(source) => Err(AexError::Read {
            path: project_path,
            source,
        }),
    }
}

pub(crate) fn read_yaml_blocking<T: DeserializeOwned>(path: &Path) -> AexResult<T> {
    let content = fs::read_to_string(path).map_err(|source| AexError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&content).map_err(|source| AexError::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn read_yaml_value_blocking(path: &Path) -> AexResult<serde_yaml::Value> {
    read_yaml_blocking(path)
}

#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::{ProfileDocument, RawProfile};
use crate::storage::project_store::read_yaml_blocking;

pub(crate) trait ProfileRepository: Send + Sync {
    fn load_profile_blocking(&self, directory: &Path, profile_id: &str) -> AexResult<RawProfile>;

    fn list_profiles_blocking(&self, directory: &Path) -> AexResult<Vec<RawProfile>>;
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FileProfileStore;

impl ProfileRepository for FileProfileStore {
    fn load_profile_blocking(&self, directory: &Path, profile_id: &str) -> AexResult<RawProfile> {
        let profiles = self.list_profiles_blocking(directory)?;
        profiles
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| AexError::ProfileNotFound {
                profile_id: profile_id.to_owned(),
                directory: directory.to_path_buf(),
            })
    }

    fn list_profiles_blocking(&self, directory: &Path) -> AexResult<Vec<RawProfile>> {
        let paths = yaml_paths_blocking(directory)?;
        paths
            .iter()
            .map(|path| {
                let document: ProfileDocument = read_yaml_blocking(path)?;
                if document.schema_version != 1 {
                    return Err(AexError::validation(
                        "UNSUPPORTED_SCHEMA_VERSION",
                        path.display().to_string(),
                        format!("expected schema version 1, got {}", document.schema_version),
                    ));
                }
                Ok(document.profile)
            })
            .collect()
    }
}

fn yaml_paths_blocking(directory: &Path) -> AexResult<Vec<PathBuf>> {
    let entries = fs::read_dir(directory).map_err(|source| AexError::Read {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut paths = Vec::new();
    for entry_result in entries {
        let entry = entry_result.map_err(|source| AexError::Read {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            paths.extend(yaml_paths_blocking(&path)?);
        } else if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("yaml")
        ) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

use crate::domain::diagnostic::{AexError, AexResult};
use ace_model::VehicleModel;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// A vehicle file opened for optimistic, atomic replacement.
#[derive(Clone)]
pub(crate) struct DocumentFile {
    path: PathBuf,
    bytes_hash: String,
    model: VehicleModel,
}

impl DocumentFile {
    pub(crate) fn model(&self) -> &VehicleModel {
        &self.model
    }
    pub(crate) fn open_blocking(path: PathBuf) -> AexResult<Self> {
        let bytes = fs::read(&path).map_err(|source| AexError::Read {
            path: path.clone(),
            source,
        })?;
        let model = serde_yaml::from_slice(&bytes).map_err(|source| AexError::Yaml {
            path: path.clone(),
            source,
        })?;
        Ok(Self {
            path,
            bytes_hash: hash(&bytes),
            model,
        })
    }

    pub(crate) fn model_digest(&self) -> AexResult<String> {
        let bytes = serde_json::to_vec(&self.model).map_err(|source| AexError::Json { source })?;
        Ok(hash(&bytes))
    }

    pub(crate) fn save_blocking(&mut self, model: &VehicleModel) -> AexResult<()> {
        let current = fs::read(&self.path).map_err(|source| AexError::Read {
            path: self.path.clone(),
            source,
        })?;
        if hash(&current) != self.bytes_hash {
            return Err(AexError::validation(
                "EXTERNAL_EDIT",
                "model",
                "model file changed outside this session; export your design before reopening it",
            ));
        }
        let bytes = encode(&self.path, model)?;
        persist(&self.path, &bytes, true)?;
        self.bytes_hash = hash(&bytes);
        self.model = model.clone();
        Ok(())
    }
}

/// Write `model` to a new file, refusing to replace an existing one.
pub(crate) fn create_blocking(path: &Path, model: &VehicleModel) -> AexResult<()> {
    persist(path, &encode(path, model)?, false)
}

/// JSON for `.json` paths, YAML otherwise, matching how the file is read back.
fn encode(path: &Path, model: &VehicleModel) -> AexResult<Vec<u8>> {
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        serde_json::to_vec_pretty(model).map_err(|source| AexError::Json { source })
    } else {
        serde_yaml::to_string(model)
            .map(String::into_bytes)
            .map_err(|source| AexError::Yaml {
                path: path.into(),
                source,
            })
    }
}

fn persist(path: &Path, bytes: &[u8], replace: bool) -> AexResult<()> {
    let write_error = |source| AexError::Write {
        path: path.into(),
        source,
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(write_error)?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(write_error)?;
    if replace {
        temporary
            .persist(path)
            .map_err(|error| write_error(error.error))?;
    } else {
        temporary
            .persist_noclobber(path)
            .map_err(|error| write_error(error.error))?;
    }
    Ok(())
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

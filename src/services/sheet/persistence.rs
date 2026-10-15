use crate::domain::diagnostic::{AexError, AexResult};
use ace_model::VehicleModel;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone)]
pub(super) struct DocumentFile {
    path: PathBuf,
    bytes_hash: String,
    model: VehicleModel,
}

impl DocumentFile {
    pub(super) fn model(&self) -> &VehicleModel {
        &self.model
    }
    pub(super) fn open_blocking(path: PathBuf) -> AexResult<Self> {
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

    pub(super) fn model_digest(&self) -> AexResult<String> {
        let bytes = serde_json::to_vec(&self.model).map_err(|source| AexError::Json { source })?;
        Ok(hash(&bytes))
    }

    pub(super) fn save_blocking(&mut self, model: &VehicleModel) -> AexResult<()> {
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
        let bytes = if self
            .path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            serde_json::to_vec_pretty(model).map_err(|source| AexError::Json { source })?
        } else {
            serde_yaml::to_string(model)
                .map_err(|source| AexError::Yaml {
                    path: self.path.clone(),
                    source,
                })?
                .into_bytes()
        };
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|source| AexError::Write {
                path: self.path.clone(),
                source,
            })?;
        temporary
            .write_all(&bytes)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|source| AexError::Write {
                path: self.path.clone(),
                source,
            })?;
        temporary
            .persist(&self.path)
            .map_err(|error| AexError::Write {
                path: self.path.clone(),
                source: error.error,
            })?;
        self.bytes_hash = hash(&bytes);
        self.model = model.clone();
        Ok(())
    }
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

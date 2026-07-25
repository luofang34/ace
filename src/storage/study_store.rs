use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use crate::domain::content_identity::validate_content_id;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::evidence::{EvidenceEnvelope, StudyArchive};

pub(crate) trait StudyRepository: Send + Sync {
    fn save_evaluation_blocking(&self, evidence: &EvidenceEnvelope) -> AexResult<PathBuf>;

    fn load_evaluation_blocking(&self, evaluation_id: &str) -> AexResult<EvidenceEnvelope>;

    fn save_archive_blocking(&self, archive: &StudyArchive) -> AexResult<PathBuf>;

    fn load_archive_blocking(&self, archive_id: &str) -> AexResult<StudyArchive>;

    fn list_archives_blocking(&self) -> AexResult<Vec<StudyArchive>>;

    fn archive_path(&self, archive_id: &str) -> AexResult<PathBuf>;
}

#[derive(Debug, Clone)]
pub(crate) struct FileStudyStore {
    root: PathBuf,
}

impl FileStudyStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn evaluation_path(&self, evaluation_id: &str) -> AexResult<PathBuf> {
        content_path(&self.root, "evaluations", "eval_", evaluation_id)
    }

    pub(crate) fn archive_path(&self, archive_id: &str) -> AexResult<PathBuf> {
        content_path(&self.root, "archives", "archive_", archive_id)
    }
}

impl StudyRepository for FileStudyStore {
    fn save_evaluation_blocking(&self, evidence: &EvidenceEnvelope) -> AexResult<PathBuf> {
        evidence.validate()?;
        let path = self.evaluation_path(&evidence.evaluation_id)?;
        let bytes =
            serde_json::to_vec_pretty(evidence).map_err(|source| AexError::Json { source })?;
        if path.exists() || !write_new_blocking(&path, &bytes)? {
            let stored = self.load_evaluation_blocking(&evidence.evaluation_id)?;
            require_immutable_match(&path, stored == *evidence)?;
        }
        Ok(path)
    }

    fn load_evaluation_blocking(&self, evaluation_id: &str) -> AexResult<EvidenceEnvelope> {
        let path = self.evaluation_path(evaluation_id)?;
        let evidence: EvidenceEnvelope = read_stored_json_blocking(&path)?;
        evidence
            .validate()
            .map_err(|source| AexError::StoredRecord {
                path,
                source: Box::new(source),
            })?;
        Ok(evidence)
    }

    fn save_archive_blocking(&self, archive: &StudyArchive) -> AexResult<PathBuf> {
        archive.validate()?;
        let path = self.archive_path(&archive.archive_id)?;
        let bytes =
            serde_json::to_vec_pretty(archive).map_err(|source| AexError::Json { source })?;
        if path.exists() || !write_new_blocking(&path, &bytes)? {
            let stored = self.load_archive_blocking(&archive.archive_id)?;
            require_immutable_match(&path, stored == *archive)?;
        }
        Ok(path)
    }

    fn load_archive_blocking(&self, archive_id: &str) -> AexResult<StudyArchive> {
        let path = self.archive_path(archive_id)?;
        let archive: StudyArchive = read_stored_json_blocking(&path)?;
        archive
            .validate()
            .map_err(|source| AexError::StoredRecord {
                path,
                source: Box::new(source),
            })?;
        Ok(archive)
    }

    fn list_archives_blocking(&self) -> AexResult<Vec<StudyArchive>> {
        let root = self.root.join("archives");
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut paths = json_paths_blocking(&root)?;
        paths.sort();
        paths
            .iter()
            .map(|path| {
                let archive: StudyArchive = read_stored_json_blocking(path)?;
                archive
                    .validate()
                    .map_err(|source| AexError::StoredRecord {
                        path: path.clone(),
                        source: Box::new(source),
                    })?;
                Ok(archive)
            })
            .collect()
    }

    fn archive_path(&self, archive_id: &str) -> AexResult<PathBuf> {
        FileStudyStore::archive_path(self, archive_id)
    }
}

fn json_paths_blocking(root: &Path) -> AexResult<Vec<PathBuf>> {
    let entries = fs::read_dir(root).map_err(|source| AexError::Read {
        path: root.to_path_buf(),
        source,
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| AexError::Read {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| AexError::Read {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            paths.extend(json_paths_blocking(&path)?);
        } else if file_type.is_file()
            && path.extension().and_then(|value| value.to_str()) == Some("json")
        {
            paths.push(path);
        } else if file_type.is_symlink() {
            return Err(AexError::validation(
                "UNSUPPORTED_STUDY_STORE_ENTRY",
                path.display().to_string(),
                "study stores cannot contain symbolic links",
            ));
        }
    }
    Ok(paths)
}

fn content_path(root: &Path, collection: &str, prefix: &str, id: &str) -> AexResult<PathBuf> {
    validate_content_id(id, prefix, &format!("{collection}.id"))?;
    let digest = id.strip_prefix(prefix).ok_or_else(|| {
        AexError::validation("INVALID_CONTENT_ID", format!("{collection}.id"), id)
    })?;
    Ok(root
        .join(collection)
        .join(&digest[..2])
        .join(format!("{id}.json")))
}

fn write_new_blocking(path: &Path, bytes: &[u8]) -> AexResult<bool> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(directory).map_err(|source| AexError::Write {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut temporary = NamedTempFile::new_in(directory).map_err(|source| AexError::Write {
        path: directory.to_path_buf(),
        source,
    })?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| AexError::Write {
            path: temporary.path().to_path_buf(),
            source,
        })?;
    match temporary.persist_noclobber(path) {
        Ok(_) => Ok(true),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(AexError::Write {
            path: path.to_path_buf(),
            source: error.error,
        }),
    }
}

fn read_stored_json_blocking<T: serde::de::DeserializeOwned>(path: &Path) -> AexResult<T> {
    let bytes = fs::read(path).map_err(|source| AexError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| AexError::StoredJson {
        path: path.to_path_buf(),
        source,
    })
}

fn require_immutable_match(path: &Path, matches: bool) -> AexResult<()> {
    if matches {
        Ok(())
    } else {
        Err(AexError::validation(
            "IMMUTABLE_RECORD_CONFLICT",
            path.display().to_string(),
            "content-addressed records cannot be overwritten",
        ))
    }
}

#[cfg(test)]
mod tests;

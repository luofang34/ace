use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::schema::{AssumptionEntry, ResolvedScenario};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunManifest {
    pub(crate) run_id: String,
    pub(crate) scenario_id: String,
    pub(crate) analysis: String,
    pub(crate) software_version: String,
    pub(crate) timestamp: String,
    pub(crate) deterministic_seed: u64,
    pub(crate) content_hash: String,
    pub(crate) models: Vec<ModelManifestEntry>,
    pub(crate) artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelManifestEntry {
    pub(crate) model_id: String,
    pub(crate) model_version: String,
    pub(crate) fidelity_level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunRecord {
    pub(crate) run_id: String,
    pub(crate) directory: PathBuf,
    pub(crate) manifest: RunManifest,
}

pub(crate) trait RunRepository: Send + Sync {
    fn persist_blocking(&self, request: PersistRunRequest<'_>) -> AexResult<RunRecord>;

    fn load_result_blocking(&self, run_id: &str) -> AexResult<(RunManifest, Value)>;
}

pub(crate) struct PersistRunRequest<'a> {
    pub(crate) scenario: &'a ResolvedScenario,
    pub(crate) analysis: &'a str,
    pub(crate) original_input: &'a Value,
    pub(crate) result: &'a Value,
    pub(crate) warnings: &'a [Diagnostic],
    pub(crate) seed: u64,
    pub(crate) artifacts: &'a [String],
}

#[derive(Debug, Clone)]
pub(crate) struct FileRunStore {
    root: PathBuf,
}

impl FileRunStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl RunRepository for FileRunStore {
    fn persist_blocking(&self, request: PersistRunRequest<'_>) -> AexResult<RunRecord> {
        fs::create_dir_all(&self.root).map_err(|source| AexError::Write {
            path: self.root.clone(),
            source,
        })?;
        let content_hash = content_hash(&request)?;
        let timestamp = Utc::now();
        let run_id = format!(
            "{}-{}",
            timestamp.format("%Y-%m-%dT%H%M%SZ"),
            &content_hash[..8]
        );
        let target = self.root.join(&run_id);
        if target.exists() {
            return Err(AexError::analysis(
                "RUN_ID_COLLISION",
                format!(
                    "immutable run directory {} already exists",
                    target.display()
                ),
            ));
        }
        let temporary = self.root.join(format!(".{run_id}.tmp"));
        fs::create_dir(&temporary).map_err(|source| AexError::Write {
            path: temporary.clone(),
            source,
        })?;
        let manifest = manifest(&request, &run_id, &content_hash, timestamp.to_rfc3339());
        write_run_files(&temporary, &request, &manifest)?;
        fs::rename(&temporary, &target).map_err(|source| AexError::Write {
            path: target.clone(),
            source,
        })?;
        Ok(RunRecord {
            run_id,
            directory: target,
            manifest,
        })
    }

    fn load_result_blocking(&self, run_id: &str) -> AexResult<(RunManifest, Value)> {
        let directory = self.root.join(run_id);
        let manifest = read_json_blocking(&directory.join("manifest.json"))?;
        let result = read_json_blocking(&directory.join("results.json"))?;
        Ok((manifest, result))
    }
}

fn content_hash(request: &PersistRunRequest<'_>) -> AexResult<String> {
    let hash_value = serde_json::json!({
        "scenario": request.scenario,
        "analysis": request.analysis,
        "input": request.original_input,
        "result": request.result,
        "seed": request.seed,
    });
    let bytes = serde_json::to_vec(&hash_value).map_err(|source| AexError::Json { source })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn manifest(
    request: &PersistRunRequest<'_>,
    run_id: &str,
    content_hash: &str,
    timestamp: String,
) -> RunManifest {
    RunManifest {
        run_id: run_id.to_owned(),
        scenario_id: request.scenario.id.clone(),
        analysis: request.analysis.to_owned(),
        software_version: env!("CARGO_PKG_VERSION").to_owned(),
        timestamp,
        deterministic_seed: request.seed,
        content_hash: content_hash.to_owned(),
        models: vec![
            ModelManifestEntry {
                model_id: "atmosphere.isa1976".to_owned(),
                model_version: "1.0.0".to_owned(),
                fidelity_level: 1,
            },
            ModelManifestEntry {
                model_id: request.scenario.aircraft.aerodynamics.model.clone(),
                model_version: "1.0.0".to_owned(),
                fidelity_level: 1,
            },
            ModelManifestEntry {
                model_id: request.scenario.engine.model_id().to_owned(),
                model_version: "1".to_owned(),
                fidelity_level: 1,
            },
        ],
        artifacts: request.artifacts.to_vec(),
    }
}

fn write_run_files(
    directory: &Path,
    request: &PersistRunRequest<'_>,
    manifest: &RunManifest,
) -> AexResult<()> {
    write_yaml_blocking(&directory.join("input.yaml"), request.original_input)?;
    write_yaml_blocking(&directory.join("resolved.yaml"), request.scenario)?;
    write_json_blocking(&directory.join("manifest.json"), manifest)?;
    write_json_blocking(&directory.join("results.json"), request.result)?;
    write_json_blocking(&directory.join("warnings.json"), request.warnings)?;
    write_assumptions_blocking(
        &directory.join("assumptions.csv"),
        &request.scenario.assumptions,
    )?;
    fs::create_dir(directory.join("artifacts")).map_err(|source| AexError::Write {
        path: directory.join("artifacts"),
        source,
    })?;
    Ok(())
}

fn write_json_blocking<T: Serialize + ?Sized>(path: &Path, value: &T) -> AexResult<()> {
    let data = serde_json::to_vec_pretty(value).map_err(|source| AexError::Json { source })?;
    fs::write(path, data).map_err(|source| AexError::Write {
        path: path.to_path_buf(),
        source,
    })
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

fn write_assumptions_blocking(path: &Path, entries: &[AssumptionEntry]) -> AexResult<()> {
    let mut writer = csv::Writer::from_path(path).map_err(|source| AexError::Write {
        path: path.to_path_buf(),
        source: std::io::Error::other(source),
    })?;
    for entry in entries {
        writer.serialize(entry).map_err(|source| AexError::Write {
            path: path.to_path_buf(),
            source: std::io::Error::other(source),
        })?;
    }
    writer.flush().map_err(|source| AexError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json_blocking<T: serde::de::DeserializeOwned>(path: &Path) -> AexResult<T> {
    let data = fs::read(path).map_err(|source| AexError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&data).map_err(|source| AexError::Json { source })
}

#[cfg(test)]
mod tests;

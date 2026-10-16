use ace_cfd::{Conserved, ConvergenceSample};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub(super) enum CheckpointError {
    #[error("checkpoint I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("checkpoint serialization at {path}: {source}")]
    Serialization {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Serialize)]
struct SavedState<'a> {
    solution: SavedSolution<'a>,
    checkpoint: &'a ConvergenceSample,
}

#[derive(Serialize)]
struct SavedSolution<'a> {
    state: &'a [Conserved],
    converged: bool,
}

pub(super) fn write_blocking(
    path: &Path,
    sample: &ConvergenceSample,
    state: &[Conserved],
) -> Result<(), CheckpointError> {
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(format!(".partial-{}-{}", std::process::id(), sample.step));
    let temporary = PathBuf::from(temporary);
    let io = |source| CheckpointError::Io {
        path: path.to_path_buf(),
        source,
    };
    let mut file = std::fs::File::create_new(&temporary).map_err(io)?;
    let result = (|| {
        let saved = SavedState {
            solution: SavedSolution {
                state,
                converged: false,
            },
            checkpoint: sample,
        };
        serde_json::to_writer(&mut file, &saved).map_err(|source| {
            CheckpointError::Serialization {
                path: path.to_path_buf(),
                source,
            }
        })?;
        file.sync_all().map_err(io)?;
        // Readers see either complete checkpoint; a failed write leaves the prior one intact.
        std::fs::rename(&temporary, path).map_err(io)
    })();
    if result.is_err() {
        std::fs::remove_file(&temporary).ok();
    }
    result
}

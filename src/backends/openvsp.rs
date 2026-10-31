use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::NamedTempFile;

use crate::backends::contracts::{BackendDescriptor, BackendTopologyCapabilities, GeometryBackend};
use crate::domain::diagnostic::{AexError, AexResult};

pub(crate) mod canonical;
pub(crate) mod kernel;
mod parsing;
pub(crate) mod polar;
mod synthesis;

use parsing::positive_marker_number;

#[derive(Debug, Clone)]
pub(crate) struct OpenVspBackend {
    executable: PathBuf,
    version: Option<String>,
    search_path: OsString,
}

impl OpenVspBackend {
    pub(crate) fn new(executable: PathBuf, version: Option<String>, search_path: OsString) -> Self {
        Self {
            executable,
            version,
            search_path,
        }
    }
}

impl GeometryBackend for OpenVspBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor_value()
    }
}

impl OpenVspBackend {
    fn descriptor_value(&self) -> BackendDescriptor {
        openvsp_descriptor(true, self.version.clone(), None)
    }

    fn run_script_controlled_blocking(
        &self,
        directory: &Path,
        script: &str,
        operation: &str,
        success_marker: &str,
        control: &ace_engine::RunControl,
    ) -> AexResult<String> {
        let mut script_file =
            NamedTempFile::new_in(directory).map_err(|source| AexError::Write {
                path: directory.to_path_buf(),
                source,
            })?;
        script_file
            .write_all(script.as_bytes())
            .and_then(|()| script_file.flush())
            .map_err(|source| AexError::Write {
                path: script_file.path().to_path_buf(),
                source,
            })?;
        let mut command = Command::new(&self.executable);
        command
            .args(["-script"])
            .arg(script_file.path())
            .current_dir(directory)
            .env("PATH", &self.search_path);
        let result = super::process::output_blocking(command, control).map_err(|source| {
            AexError::Service {
                operation: "OpenVSP process",
                source: Box::new(source),
            }
        })?;
        let mut output = String::from_utf8_lossy(&result.stdout).into_owned();
        output.push_str(&String::from_utf8_lossy(&result.stderr));
        if let Some(error) = output
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix("ACE_ERROR="))
        {
            return Err(backend_failure(operation, error, result.status.code()));
        }
        let lines = output.lines().map(str::trim).collect::<Vec<_>>();
        let protocol_complete = lines.iter().any(|line| line.starts_with(success_marker))
            && lines.contains(&"ACE_COMPLETE=1");
        if !result.status.success() || !protocol_complete {
            return Err(backend_failure(operation, &output, result.status.code()));
        }
        Ok(output)
    }
}

pub(super) fn openvsp_descriptor(
    available: bool,
    version: Option<String>,
    unavailable_reason: Option<String>,
) -> BackendDescriptor {
    BackendDescriptor {
        id: "openvsp".to_owned(),
        display_name: "OpenVSP subprocess refinement".to_owned(),
        available,
        version,
        capabilities: vec![
            "vsp3_geometry".to_owned(),
            "tailless_bwb_geometry".to_owned(),
            "wetted_area".to_owned(),
            "vspaero_polar".to_owned(),
            "static_pitching_moment".to_owned(),
        ],
        disciplines: vec!["geometry".to_owned(), "aerodynamics".to_owned()],
        fidelity_levels: vec![1, 2],
        topology: openvsp_topology_capabilities(),
        unavailable_reason,
    }
}

pub(super) fn script_string(path: &Path) -> AexResult<String> {
    let raw = path.to_string_lossy();
    if raw.contains(['\n', '\r', '\0']) {
        return Err(AexError::validation(
            "INVALID_ARTIFACT_PATH",
            path.display().to_string(),
            "OpenVSP artifact paths cannot contain control characters",
        ));
    }
    Ok(raw.replace('\\', "\\\\").replace('"', "\\\""))
}

fn backend_failure(operation: &str, output: &str, code: Option<i32>) -> AexError {
    let tail = output
        .lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    AexError::BackendExecution {
        backend: "openvsp".to_owned(),
        operation: operation.to_owned(),
        message: format!("exit code {code:?}: {tail}"),
    }
}

pub(super) fn openvsp_topology_capabilities() -> BackendTopologyCapabilities {
    BackendTopologyCapabilities {
        component_kinds: vec![
            "fuselage".to_owned(),
            "wing".to_owned(),
            "horizontal_tail".to_owned(),
            "vertical_tail".to_owned(),
            "lifting_body".to_owned(),
            "engine".to_owned(),
            "propeller".to_owned(),
        ],
        relationship_kinds: vec!["attached_to".to_owned(), "symmetric_about".to_owned()],
        delegated_relationship_kinds: vec!["carries_load_to".to_owned()],
    }
}

#[cfg(test)]
mod tests;

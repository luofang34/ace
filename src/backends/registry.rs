use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::backends::contracts::{BackendDescriptor, GeometryBackend};
use crate::backends::native::NativeBackend;
use crate::backends::openvsp::OpenVspBackend;
use crate::domain::diagnostic::{AexError, AexResult};

const OPENVSP_ENV: &str = "ACE_OPENVSP_EXECUTABLE";
const MACOS_VSPSCRIPT: &str = "/Applications/OpenVSP.app/Contents/Resources/vspscript";

#[derive(Debug, Clone)]
pub(crate) struct BackendRegistry {
    native: NativeBackend,
    openvsp: Option<OpenVspBackend>,
    openvsp_unavailable_reason: Option<String>,
}

impl BackendRegistry {
    pub(crate) fn detect_blocking() -> Self {
        let discovery = discover_openvsp();
        match discovery {
            Ok(path) => {
                let search_path = backend_search_path(&path);
                let version = probe_version_blocking(&path, &search_path);
                Self {
                    native: NativeBackend,
                    openvsp: Some(OpenVspBackend::new(path, version, search_path)),
                    openvsp_unavailable_reason: None,
                }
            }
            Err(reason) => Self {
                native: NativeBackend,
                openvsp: None,
                openvsp_unavailable_reason: Some(reason),
            },
        }
    }

    pub(crate) fn native(&self) -> &NativeBackend {
        &self.native
    }

    pub(crate) fn openvsp(&self) -> AexResult<&OpenVspBackend> {
        self.openvsp
            .as_ref()
            .ok_or_else(|| AexError::BackendUnavailable {
                backend: "openvsp".to_owned(),
                reason: self
                    .openvsp_unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "executable was not discovered".to_owned()),
            })
    }

    pub(crate) fn descriptors(&self) -> Vec<BackendDescriptor> {
        let native = GeometryBackend::descriptor(&self.native);
        let openvsp = self.openvsp.as_ref().map_or_else(
            || BackendDescriptor {
                id: "openvsp".to_owned(),
                display_name: "OpenVSP subprocess refinement".to_owned(),
                available: false,
                version: None,
                capabilities: vec![
                    "vsp3_geometry".to_owned(),
                    "wetted_area".to_owned(),
                    "vspaero_polar".to_owned(),
                    "static_pitching_moment".to_owned(),
                ],
                unavailable_reason: self.openvsp_unavailable_reason.clone(),
            },
            GeometryBackend::descriptor,
        );
        vec![native, openvsp]
    }
}

fn discover_openvsp() -> Result<PathBuf, String> {
    if let Some(configured) = env::var_os(OPENVSP_ENV) {
        let path = PathBuf::from(configured);
        return executable_path(&path).ok_or_else(|| {
            format!(
                "{OPENVSP_ENV} points to {}, which is not an executable file",
                path.display()
            )
        });
    }
    let macos = PathBuf::from(MACOS_VSPSCRIPT);
    if let Some(path) = executable_path(&macos) {
        return Ok(path);
    }
    env::var_os("PATH")
        .and_then(|path| {
            env::split_paths(&path)
                .map(|directory| directory.join("vspscript"))
                .find_map(|candidate| executable_path(&candidate))
        })
        .ok_or_else(|| {
            format!("set {OPENVSP_ENV} or install OpenVSP with the headless vspscript executable")
        })
}

fn executable_path(path: &Path) -> Option<PathBuf> {
    path.is_file().then(|| path.to_path_buf())
}

fn backend_search_path(executable: &Path) -> OsString {
    let mut paths = executable
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    env::join_paths(paths).unwrap_or_else(|_| OsString::from("/usr/bin:/bin"))
}

fn probe_version_blocking(executable: &Path, search_path: &OsString) -> Option<String> {
    let output = Command::new(executable)
        .arg("-help")
        .env("PATH", search_path)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Vehicle Sketch Pad"))
        .and_then(|line| line.split_whitespace().last())
        .map(str::to_owned)
}

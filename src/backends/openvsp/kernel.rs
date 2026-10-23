//! Whole-vehicle OpenVSP pipeline: build canonical shapes, canonicalize the model file, then
//! export interchange products and surface measurements from that canonical file.

use super::{
    OpenVspBackend, canonical, parsing::positive_marker_number, synthesis::validate_readback,
};
use crate::domain::diagnostic::{AexError, AexResult};
use std::{collections::BTreeMap, fs, path::Path};

mod script;
pub(crate) use script::{KernelObject, KernelShape};

const BUILD_MARKER: &str = "ACE_BUILD=";
const AREA_MARKER: &str = "ACE_WETTED_AREA_M2=";
const VOLUME_MARKER: &str = "ACE_VOLUME_M3=";
const COMPONENT_MARKER: &str = "ACE_COMPONENT=";

/// Intersected surface measurements of one canonical object.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct KernelComponent {
    /// Canonical geometry identifier.
    pub id: String,
    /// CompGeom wetted area after intersection with the other components.
    pub wetted_area_m2: f64,
    /// CompGeom enclosed volume after intersection.
    pub volume_m3: f64,
}

/// Interchange artifacts and measurements of one canonical kernel evaluation.
pub(crate) struct KernelProducts {
    /// Canonicalized native model consumed by the export pass.
    pub vsp3: String,
    /// STEP AP203 surfaces with a fixed header time stamp.
    pub step: String,
    /// Wavefront OBJ tessellation of the untrimmed component surfaces.
    pub obj: String,
    /// Kernel parameter values read back after generation.
    pub readback: BTreeMap<String, f64>,
    /// Per-object intersected measurements.
    pub components: Vec<KernelComponent>,
    /// Total intersected wetted area.
    pub wetted_area_m2: f64,
    /// Total enclosed volume.
    pub volume_m3: f64,
}

impl OpenVspBackend {
    pub(crate) fn kernel_geometry_blocking(
        &self,
        objects: &[KernelObject],
        control: &ace_engine::RunControl,
    ) -> AexResult<KernelProducts> {
        let directory = tempfile::tempdir().map_err(|source| AexError::Write {
            path: std::env::temp_dir(),
            source,
        })?;
        let build = self.run_script_controlled_blocking(
            directory.path(),
            &script::build(objects)?,
            "canonical kernel geometry",
            BUILD_MARKER,
            control,
        )?;
        let readback = validate_readback(&build, &script::expected_readback(objects))?;
        let vsp3 = canonical::vsp3(&read(directory.path(), script::MODEL_FILE)?);
        let canonical_path = directory.path().join(script::CANONICAL_FILE);
        fs::write(&canonical_path, &vsp3).map_err(|source| AexError::Write {
            path: canonical_path,
            source,
        })?;
        let export = self.run_script_controlled_blocking(
            directory.path(),
            script::EXPORT,
            "kernel interchange export",
            AREA_MARKER,
            control,
        )?;
        Ok(KernelProducts {
            step: canonical::step(&read(directory.path(), script::STEP_FILE)?),
            obj: read(directory.path(), script::OBJ_FILE)?,
            vsp3,
            readback,
            components: components(&export)?,
            wetted_area_m2: positive_marker_number(&export, AREA_MARKER)?,
            volume_m3: positive_marker_number(&export, VOLUME_MARKER)?,
        })
    }
}

fn read(directory: &Path, name: &str) -> AexResult<String> {
    let path = directory.join(name);
    fs::read_to_string(&path).map_err(|source| AexError::Read { path, source })
}

/// CompGeom reports every component, including its own result mesh; only canonical objects
/// carry the generated name prefix.
fn components(output: &str) -> AexResult<Vec<KernelComponent>> {
    output
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix(COMPONENT_MARKER))
        .filter_map(|line| {
            let (name, measurements) = line.split_once(',')?;
            Some((name.strip_prefix(script::NAME_PREFIX)?, measurements))
        })
        .map(|(id, measurements)| {
            let (area, volume) = measurements.split_once(',').ok_or_else(|| {
                parse_failure(format!("component {id}: missing volume measurement"))
            })?;
            Ok(KernelComponent {
                id: id.to_owned(),
                wetted_area_m2: measurement(id, area)?,
                volume_m3: measurement(id, volume)?,
            })
        })
        .collect()
}

fn measurement(id: &str, raw: &str) -> AexResult<f64> {
    let value = raw
        .trim()
        .parse::<f64>()
        .map_err(|source| parse_failure(format!("component {id}: {source}")))?;
    if !value.is_finite() || value < 0.0 {
        return Err(parse_failure(format!(
            "component {id}: measurement must be finite and nonnegative, got {value}"
        )));
    }
    Ok(value)
}

fn parse_failure(message: String) -> AexError {
    AexError::BackendExecution {
        backend: "openvsp".to_owned(),
        operation: "kernel result parsing".to_owned(),
        message,
    }
}

#[cfg(test)]
mod tests;

use super::OpenVspBackend;
use crate::domain::diagnostic::{AexError, AexResult};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

mod fitting;
pub(crate) use fitting::{Fit, Point};

const SCRIPT: &str = include_str!("scripts/synthesis_polar.vspscript");
pub(crate) const MACH: [f64; 3] = [0.2, 0.5, 0.8];
pub(crate) const PORTS: [&str; 4] = ["k_020", "k_050", "k_080", "cl_limit"];

#[derive(Serialize)]
pub(crate) struct Polar {
    pub geometry: String,
    pub readback: BTreeMap<String, f64>,
    pub points: Vec<Point>,
    pub fits: Vec<Fit>,
    pub script: String,
    pub raw: String,
}

impl OpenVspBackend {
    pub(crate) fn polar_identity_blocking(&self) -> AexResult<String> {
        let solver = self.executable.with_file_name("vspaero");
        let bytes = fs::read(&solver).map_err(|source| AexError::Read {
            path: solver,
            source,
        })?;
        Ok(format!(
            "{};vspaero:sha256:{:x}",
            self.binary_identity_blocking()?,
            Sha256::digest(bytes)
        ))
    }

    pub(crate) fn synthesis_polar_blocking(
        &self,
        inputs: &BTreeMap<String, f64>,
        control: &ace_engine::RunControl,
    ) -> AexResult<Polar> {
        let wing = self.generate_synthesis_wing_blocking(inputs, control)?;
        let directory = tempfile::tempdir().map_err(|source| AexError::Write {
            path: std::env::temp_dir(),
            source,
        })?;
        let path = directory.path().join("wing.vsp3");
        fs::write(&path, &wing.content).map_err(|source| AexError::Write { path, source })?;
        let area = required(inputs, "area")?;
        let span = required(inputs, "span")?;
        let script = SCRIPT
            .replace("__AREA__", &format!("{area:.15}"))
            .replace("__SPAN__", &format!("{span:.15}"))
            .replace("__CHORD__", &format!("{:.15}", area / span));
        let raw = self.run_script_controlled_blocking(
            directory.path(),
            &script,
            "canonical VSPAERO polar",
            "ACE_POLAR=",
            control,
        )?;
        let points = fitting::parse(&raw)?;
        let fits = fitting::fit(&points)?;
        Ok(Polar {
            geometry: wing.content,
            readback: wing.measured,
            points,
            fits,
            script,
            raw,
        })
    }
}

fn required(inputs: &BTreeMap<String, f64>, name: &str) -> AexResult<f64> {
    inputs
        .get(name)
        .copied()
        .ok_or_else(|| invalid(format!("missing {name}")))
}

fn invalid(message: impl Into<String>) -> AexError {
    AexError::validation("INVALID_VSPAERO_POLAR", "aerodynamics", message)
}

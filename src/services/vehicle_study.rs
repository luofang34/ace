use crate::domain::diagnostic::{AexError, AexResult};
use ace_study::{StudyArchive, StudyPlan};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn run_blocking(
    vehicle: &Path,
    plan: &Path,
    overrides: &BTreeMap<String, String>,
) -> AexResult<StudyArchive> {
    let content = std::fs::read_to_string(plan).map_err(|source| AexError::Read {
        path: plan.into(),
        source,
    })?;
    let plan: StudyPlan = serde_yaml::from_str(&content).map_err(|source| AexError::Yaml {
        path: plan.into(),
        source,
    })?;
    let session = super::synthesis::open_blocking(vehicle, overrides)?;
    ace_study::run(&session, &plan).map_err(AexError::from)
}

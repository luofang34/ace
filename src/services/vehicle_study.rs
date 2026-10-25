use crate::charts::generators::study;
use crate::charts::renderer::render_svg_blocking;
use crate::charts::spec::ChartSpec;
use crate::domain::diagnostic::{AexError, AexResult};
use ace_study::{StudyArchive, StudyPlan};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Study archive, optionally with a chart of one output; archive fields stay at the top level.
#[derive(Debug, Serialize)]
pub(crate) struct StudyOutput {
    #[serde(flatten)]
    pub archive: StudyArchive,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<StudyChart>,
}

/// Chart data for one study output and the SVG written from it, when requested.
#[derive(Debug, Serialize)]
pub(crate) struct StudyChart {
    pub metric: String,
    pub spec: ChartSpec,
    pub artifact_path: Option<PathBuf>,
}

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

/// Attach a chart of `metric`, rendering it to `artifact` when a path is given.
pub(crate) fn chart_blocking(
    archive: StudyArchive,
    metric: Option<String>,
    artifact: Option<PathBuf>,
) -> AexResult<StudyOutput> {
    let chart = match metric {
        Some(metric) => {
            let spec = study(&archive, &metric)?;
            if let Some(path) = &artifact {
                render_svg_blocking(&spec, path)?;
            }
            Some(StudyChart {
                metric,
                spec,
                artifact_path: artifact,
            })
        }
        None if artifact.is_some() => {
            return Err(AexError::validation(
                "MISSING_CHART_METRIC",
                "artifact",
                "an artifact requires a chart metric",
            ));
        }
        None => None,
    };
    Ok(StudyOutput { archive, chart })
}

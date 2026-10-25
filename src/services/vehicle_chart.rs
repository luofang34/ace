//! Charts drawn from a synthesized vehicle's own calculation evidence.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ace_engine::ValueStatus;
use serde::Serialize;

use crate::charts::generators::{constraints, vehicle_payload_range};
use crate::charts::renderer::render_svg_blocking;
use crate::charts::spec::ChartSpec;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::result::ConstraintResult;
use crate::services::vehicle_payload_range::PayloadRange;

/// Plugins whose run details carry enough evidence to draw a chart.
const CHARTED_PLUGINS: [&str; 1] = ["aircraft.constraint_diagram"];

/// Chart data for one analysis run and the SVG written from it, when requested.
#[derive(Debug, Serialize)]
pub(crate) struct AnalysisChart {
    pub analysis: String,
    pub plugin: String,
    pub run_id: String,
    pub spec: ChartSpec,
    pub artifact_path: Option<PathBuf>,
}

/// Payload-range corners, optionally with their chart; corner fields stay at the top level.
#[derive(Debug, Serialize)]
pub(crate) struct PayloadRangeOutput {
    #[serde(flatten)]
    pub result: PayloadRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<PayloadRangeChart>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PayloadRangeChart {
    pub spec: ChartSpec,
    pub artifact_path: PathBuf,
}

/// Synthesize the vehicle and chart the latest run of `analysis` from its recorded details.
pub(crate) fn analysis_blocking(
    path: &Path,
    overrides: &BTreeMap<String, String>,
    analysis: &str,
    artifact: Option<&Path>,
) -> AexResult<AnalysisChart> {
    let mut session = super::synthesis::open_blocking(path, overrides)?;
    if !session.model().analyses.contains_key(analysis) {
        return Err(AexError::validation(
            "UNKNOWN_ANALYSIS",
            analysis.to_owned(),
            "the vehicle declares no analysis with this identifier",
        ));
    }
    let snapshot = session.run()?;
    let node = format!("analysis:{analysis}");
    let run = snapshot
        .runs
        .iter()
        .rev()
        .find(|run| run.node == node)
        .ok_or_else(|| {
            AexError::analysis(
                "ANALYSIS_NOT_RUN",
                format!("{analysis} produced no run record"),
            )
        })?;
    if !matches!(run.status, ValueStatus::Computed | ValueStatus::Approximate) {
        return Err(AexError::analysis(
            "ANALYSIS_FAILED",
            format!(
                "{analysis} did not compute: {}",
                run.message.as_deref().unwrap_or("no diagnostic")
            ),
        ));
    }
    let plugin = run
        .plugin
        .as_ref()
        .map(|identity| identity.id.clone())
        .unwrap_or_default();
    let spec = match plugin.as_str() {
        "aircraft.constraint_diagram" => {
            let diagram: ConstraintResult = serde_json::from_value(run.details["diagram"].clone())
                .map_err(|source| {
                    AexError::analysis(
                        "INVALID_ANALYSIS_EVIDENCE",
                        format!("{analysis} details are not a constraint diagram: {source}"),
                    )
                })?;
            constraints(&session.model().name, &diagram)
        }
        _ => {
            return Err(AexError::validation(
                "UNSUPPORTED_ANALYSIS_CHART",
                analysis.to_owned(),
                format!(
                    "{plugin} has no chart; charted plugins: {}",
                    CHARTED_PLUGINS.join(", ")
                ),
            ));
        }
    };
    if let Some(artifact) = artifact {
        render_svg_blocking(&spec, artifact)?;
    }
    Ok(AnalysisChart {
        analysis: analysis.to_owned(),
        plugin,
        run_id: run.id.clone(),
        spec,
        artifact_path: artifact.map(Path::to_path_buf),
    })
}

/// Attach and render the payload-range chart when an artifact path is given.
pub(crate) fn payload_range_blocking(
    result: PayloadRange,
    artifact: Option<&Path>,
) -> AexResult<PayloadRangeOutput> {
    let chart = match artifact {
        Some(artifact) => {
            let spec = vehicle_payload_range(&result);
            render_svg_blocking(&spec, artifact)?;
            Some(PayloadRangeChart {
                spec,
                artifact_path: artifact.to_path_buf(),
            })
        }
        None => None,
    };
    Ok(PayloadRangeOutput { result, chart })
}

#[cfg(test)]
mod tests;

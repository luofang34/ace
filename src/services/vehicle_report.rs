//! Design report of one synthesized vehicle, drawn only from its own calculation evidence.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

use ace_engine::{Assessment, Feasibility, Margin, MarginStatus, ValueStatus};
use ace_model::Priority;
use serde::Serialize;

use crate::charts::generators::{vehicle_mission_mass, vehicle_requirement_margins};
use crate::charts::spec::ChartSpec;
use crate::domain::diagnostic::{AexError, AexResult};

pub(crate) const LIMITATION: &str = "Results are conceptual estimates from the vehicle's \
selected low-fidelity analyses and are not suitable for certification, operational flight \
planning, or safety-critical decisions.";

#[derive(Debug, Serialize)]
pub(crate) struct VehicleReport {
    pub vehicle_id: String,
    pub name: String,
    pub revision: u64,
    pub model_digest: String,
    pub completed: bool,
    pub decision: Decision,
    pub assessment: Assessment,
    pub margins: BTreeMap<String, Margin>,
    pub charts: Vec<ReportChart>,
    pub limitation: &'static str,
}

/// Hard requirements by disposition; unknown ones are counted, never assumed met.
#[derive(Debug, Serialize)]
pub(crate) struct Decision {
    pub feasibility: Feasibility,
    pub hard_satisfied: usize,
    pub hard_failed: usize,
    pub hard_unknown: usize,
    pub hard_total: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReportChart {
    pub order: usize,
    pub section: &'static str,
    pub purpose: &'static str,
    /// Analysis whose run details the chart is drawn from, when it is not the margins.
    pub analysis: Option<String>,
    pub specification: ChartSpec,
}

/// Synthesize the vehicle and assemble its verdict, margins and evidence charts.
pub(crate) fn report_blocking(
    path: &Path,
    overrides: &BTreeMap<String, String>,
) -> AexResult<VehicleReport> {
    let mut session = super::synthesis::open_blocking(path, overrides)?;
    let snapshot = session.run()?;
    let model = session.model();
    let margins = ace_engine::evaluate_requirements(model, &snapshot)?;
    let assessment = ace_engine::assess(model, &snapshot)?;
    let mut decision = Decision {
        feasibility: assessment.feasibility,
        hard_satisfied: 0,
        hard_failed: 0,
        hard_unknown: 0,
        hard_total: 0,
    };
    for (id, requirement) in &model.requirements {
        if requirement.priority != Priority::Hard {
            continue;
        }
        decision.hard_total += 1;
        match margins.get(id).map(|margin| margin.status) {
            Some(MarginStatus::Pass | MarginStatus::Warn) => decision.hard_satisfied += 1,
            Some(MarginStatus::Fail) => decision.hard_failed += 1,
            _ => decision.hard_unknown += 1,
        }
    }
    let mut charts = vec![ReportChart {
        order: 1,
        section: "decision",
        purpose: "Shows each requirement's percentage margin; unknown margins are labelled.",
        analysis: None,
        specification: vehicle_requirement_margins(&model.name, &margins),
    }];
    for (analysis, declaration) in &model.analyses {
        let node = format!("analysis:{analysis}");
        let Some(run) = snapshot.runs.iter().rev().find(|run| run.node == node) else {
            continue;
        };
        if !matches!(run.status, ValueStatus::Computed | ValueStatus::Approximate) {
            continue;
        }
        let chart = if declaration.plugin == "vehicle.mission_sequence" {
            vehicle_mission_mass(&model.name, &run.details).map(|spec| {
                (
                    "mission",
                    "Shows fuel burn and payload releases by stage.",
                    spec,
                )
            })
        } else {
            super::vehicle_chart::run_chart(&model.name, analysis, run)?.map(|spec| {
                (
                    "sizing",
                    "Locates the design against its wing-loading constraints.",
                    spec,
                )
            })
        };
        if let Some((section, purpose, specification)) = chart {
            charts.push(ReportChart {
                order: charts.len() + 1,
                section,
                purpose,
                analysis: Some(analysis.clone()),
                specification,
            });
        }
    }
    Ok(VehicleReport {
        vehicle_id: model.id.clone(),
        name: model.name.clone(),
        revision: model.revision,
        model_digest: snapshot.model_digest.clone(),
        completed: snapshot.complete(),
        decision,
        assessment,
        margins,
        charts,
        limitation: LIMITATION,
    })
}

/// Verdict, hard-requirement counts, margin table and chart index.
pub(crate) fn markdown(report: &VehicleReport) -> AexResult<String> {
    let mut output = String::new();
    let decision = &report.decision;
    let feasibility = serde_json::to_value(decision.feasibility)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    writeln!(output, "# {}", report.name).map_err(format_error)?;
    writeln!(output, "\n- Feasibility: {feasibility}").map_err(format_error)?;
    writeln!(
        output,
        "- Hard requirements: {} satisfied, {} failed, {} unknown of {}",
        decision.hard_satisfied, decision.hard_failed, decision.hard_unknown, decision.hard_total
    )
    .map_err(format_error)?;
    writeln!(
        output,
        "- Revision {} ({})",
        report.revision, report.model_digest
    )
    .map_err(format_error)?;
    writeln!(output, "\n> {}", report.limitation).map_err(format_error)?;
    writeln!(
        output,
        "\n## Requirements\n\n| Requirement | Status | Actual | Margin |\n|---|---|---|---|"
    )
    .map_err(format_error)?;
    for (id, margin) in &report.margins {
        let status = serde_json::to_value(margin.status)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let actual = margin.actual.map_or_else(
            || {
                margin
                    .message
                    .clone()
                    .unwrap_or_else(|| "unavailable".to_owned())
            },
            |quantity| format!("{:.4} {}", quantity.value, unit(quantity.unit)),
        );
        let fraction = margin.fraction.map_or_else(
            || "—".to_owned(),
            |fraction| format!("{:+.1}%", fraction * 100.0),
        );
        writeln!(output, "| {id} | {status} | {actual} | {fraction} |").map_err(format_error)?;
    }
    writeln!(output, "\n## Chart order").map_err(format_error)?;
    for chart in &report.charts {
        writeln!(
            output,
            "\n{}. **{}** — {}",
            chart.order, chart.specification.title, chart.purpose
        )
        .map_err(format_error)?;
    }
    Ok(output)
}

fn unit(unit: ace_model::Unit) -> String {
    serde_json::to_value(unit)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn format_error(source: std::fmt::Error) -> AexError {
    AexError::analysis("REPORT_FORMAT_ERROR", source.to_string())
}

#[cfg(test)]
mod tests;

use std::path::Path;

use serde_json::{Value, json};

use crate::domain::diagnostic::{AexError, AexResult};
use crate::services::analysis::ApplicationService;
use crate::services::report::{LIMITATION, concept_report_markdown};

use super::schema::ReportRequest;

pub(super) fn generate_report_blocking(
    service: &ApplicationService,
    request: ReportRequest,
) -> AexResult<Value> {
    validate_format(&request.format)?;
    match (&request.scenario_path, &request.run_id) {
        (Some(_), Some(_)) => Err(AexError::validation(
            "AMBIGUOUS_REPORT_SOURCE",
            "report",
            "provide scenario_path or run_id, not both",
        )),
        (Some(scenario_path), None) => concept_report(service, scenario_path, &request),
        (None, Some(run_id)) => run_report(service, run_id, &request),
        (None, None) => Err(AexError::validation(
            "MISSING_REPORT_SOURCE",
            "report",
            "provide scenario_path for a concept report or run_id for a run report",
        )),
    }
}

fn concept_report(
    service: &ApplicationService,
    scenario_path: &str,
    request: &ReportRequest,
) -> AexResult<Value> {
    let backend = request.backend.as_deref().unwrap_or("native");
    let report = service.concept_report_blocking(Path::new(scenario_path), backend)?;
    let content = (request.format == "markdown")
        .then(|| concept_report_markdown(&report))
        .transpose()?;
    Ok(json!({
        "format": request.format,
        "sections": request.sections,
        "content": content,
        "report": report,
    }))
}

fn run_report(
    service: &ApplicationService,
    run_id: &str,
    request: &ReportRequest,
) -> AexResult<Value> {
    let (manifest, result) = service.load_run_blocking(run_id)?;
    let content = if request.format == "markdown" {
        Some(format!(
            "# Analysis run {run_id}\n\n{LIMITATION}\n\n## Summary\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&result).map_err(|source| AexError::Json { source })?
        ))
    } else {
        None
    };
    Ok(json!({
        "format": request.format,
        "sections": request.sections,
        "content": content,
        "manifest": manifest,
        "result": result,
        "limitation": LIMITATION,
    }))
}

fn validate_format(format: &str) -> AexResult<()> {
    if matches!(format, "json" | "markdown") {
        Ok(())
    } else {
        Err(AexError::validation(
            "INVALID_REPORT_FORMAT",
            "format",
            "format must be markdown or json",
        ))
    }
}

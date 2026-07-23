use std::fmt::Write;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::result::{MissionResult, PerformanceSummary, RequirementEvaluation};
use crate::domain::schema::ResolvedScenario;

pub(crate) const LIMITATION: &str = "Results are conceptual estimates based on the selected \
low-fidelity models and are not suitable for certification, operational flight planning, or \
safety-critical decisions.";

pub(crate) fn markdown_report(
    scenario: &ResolvedScenario,
    run_id: &str,
    mission: &MissionResult,
    performance: &PerformanceSummary,
    requirements: &[RequirementEvaluation],
) -> AexResult<String> {
    let hard_passed = requirements
        .iter()
        .filter(|item| item.severity == "hard")
        .all(|item| item.passed);
    let mut output = String::new();
    writeln!(output, "# {}", scenario.name).map_err(format_error)?;
    writeln!(output, "\n- Run ID: `{run_id}`").map_err(format_error)?;
    writeln!(output, "- Completion: {}", mission.completed).map_err(format_error)?;
    writeln!(output, "- Hard requirements passed: {hard_passed}").map_err(format_error)?;
    writeln!(output, "- Warnings: {}", mission.warnings.len()).map_err(format_error)?;
    writeln!(output, "\n> {LIMITATION}").map_err(format_error)?;
    writeln!(output, "\n## Primary results").map_err(format_error)?;
    writeln!(
        output,
        "\n- Mission distance: {:.1} nmi",
        mission.total_distance.display_value
    )
    .map_err(format_error)?;
    writeln!(
        output,
        "- Mission fuel: {:.1} kg",
        mission.total_fuel_burn_kg
    )
    .map_err(format_error)?;
    writeln!(
        output,
        "- Service ceiling: {:.0} m",
        performance.service_ceiling_m
    )
    .map_err(format_error)?;
    writeln!(output, "\n## Requirements").map_err(format_error)?;
    for item in requirements {
        writeln!(
            output,
            "\n- {}: {} (margin {:.3})",
            item.id,
            if item.passed { "PASS" } else { "FAIL" },
            item.absolute_margin
        )
        .map_err(format_error)?;
    }
    writeln!(output, "\n## Assumptions").map_err(format_error)?;
    writeln!(
        output,
        "\n{} resolved input and profile entries are recorded in the ledger.",
        scenario.assumptions.len()
    )
    .map_err(format_error)?;
    Ok(output)
}

fn format_error(source: std::fmt::Error) -> AexError {
    AexError::analysis("REPORT_FORMAT_ERROR", source.to_string())
}

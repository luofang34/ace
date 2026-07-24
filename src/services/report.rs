use std::fmt::Write;
use std::path::Path;

use serde::Serialize;

use crate::charts::generators::{
    climb_envelope, constraints, mission_mass, payload_range, performance_curves,
    requirement_margins,
};
use crate::charts::spec::{AxisSpec, ChartSpec, SeriesSpec};
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::result::{
    ConstraintResult, MissionResult, PayloadRangeResult, PerformanceSummary, RequirementEvaluation,
};
use crate::domain::schema::{ResolvedScenario, SegmentKind};
use crate::services::analysis::ApplicationService;
use crate::services::design_experiments::{BackendEvaluation, FeasibilityResult};

pub(crate) const LIMITATION: &str = "Results are conceptual estimates based on the selected \
low-fidelity models and are not suitable for certification, operational flight planning, or \
safety-critical decisions.";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConceptReportDecision {
    pub(crate) feasible: bool,
    pub(crate) hard_requirements_passed: usize,
    pub(crate) hard_requirements_total: usize,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) refinement_changes_native_verdict: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReportChart {
    pub(crate) order: u32,
    pub(crate) section: String,
    pub(crate) purpose: String,
    pub(crate) specification: ChartSpec,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConceptDesignReport {
    pub(crate) scenario: ResolvedScenario,
    pub(crate) selected_backend: String,
    pub(crate) decision: ConceptReportDecision,
    pub(crate) feasibility: FeasibilityResult,
    pub(crate) mission: MissionResult,
    pub(crate) performance: PerformanceSummary,
    pub(crate) payload_range: PayloadRangeResult,
    pub(crate) constraints: ConstraintResult,
    pub(crate) charts: Vec<ReportChart>,
    pub(crate) limitation: String,
}

impl ApplicationService {
    pub(crate) fn concept_report_blocking(
        &self,
        scenario_path: &Path,
        backend: &str,
    ) -> AexResult<ConceptDesignReport> {
        let scenario = self.resolve_blocking(scenario_path, &Default::default())?;
        let feasibility = self.evaluate_feasibility_blocking(scenario_path, backend, None)?;
        let (_, mission) = self.mission_blocking(scenario_path, &Default::default())?;
        let (_, performance) = self.performance_blocking(scenario_path, &Default::default())?;
        let (_, payload_range_result) =
            self.payload_range_blocking(scenario_path, &Default::default())?;
        let (_, constraint_result) =
            self.constraints_blocking(scenario_path, &Default::default(), 300.0, 9_000.0, 80)?;
        let decision = concept_decision(&feasibility);
        let charts = concept_charts(
            &scenario,
            &feasibility,
            &mission,
            &payload_range_result,
            &constraint_result,
        )?;
        Ok(ConceptDesignReport {
            scenario,
            selected_backend: backend.to_owned(),
            decision,
            feasibility,
            mission,
            performance,
            payload_range: payload_range_result,
            constraints: constraint_result,
            charts,
            limitation: LIMITATION.to_owned(),
        })
    }
}

fn concept_decision(feasibility: &FeasibilityResult) -> ConceptReportDecision {
    let requirements = &feasibility.baseline.analysis.requirements;
    let native_feasible = feasibility.baseline.analysis.feasible.unwrap_or(false);
    let hard = requirements
        .iter()
        .filter(|requirement| requirement.severity == "hard")
        .collect::<Vec<_>>();
    ConceptReportDecision {
        feasible: feasibility.feasible,
        hard_requirements_passed: hard.iter().filter(|item| item.passed).count(),
        hard_requirements_total: hard.len(),
        failed_constraints: feasibility.failed_constraints.clone(),
        refinement_changes_native_verdict: feasibility
            .refinement
            .as_ref()
            .and_then(|evaluation| evaluation.analysis.feasible)
            .is_some_and(|refined| refined != native_feasible),
    }
}

fn concept_charts(
    scenario: &ResolvedScenario,
    feasibility: &FeasibilityResult,
    mission: &MissionResult,
    payload_range_result: &PayloadRangeResult,
    constraint_result: &ConstraintResult,
) -> AexResult<Vec<ReportChart>> {
    let requirements = &feasibility.baseline.analysis.requirements;
    let cruise_altitude = scenario
        .mission
        .segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Cruise)
        .and_then(|segment| segment.altitude_m)
        .unwrap_or(0.0);
    let polar_evaluation = feasibility
        .refinement
        .as_ref()
        .unwrap_or(&feasibility.baseline);
    Ok(vec![
        report_chart(
            1,
            "decision",
            "Shows normalized requirement margins.",
            requirement_margins(scenario, requirements),
        ),
        report_chart(
            2,
            "mission",
            "Separates full-payload and ferry capability.",
            payload_range(scenario, payload_range_result),
        ),
        report_chart(
            3,
            "mission",
            "Shows fuel burn and explicit payload-offload mass steps.",
            mission_mass(scenario, mission),
        ),
        report_chart(
            4,
            "sizing",
            "Locates the selected design against conceptual constraints.",
            constraints(scenario, constraint_result),
        ),
        report_chart(
            5,
            "performance",
            "Shows climb capability and ceiling trend.",
            climb_envelope(scenario)?,
        ),
        report_chart(
            6,
            "performance",
            "Compares thrust or power required with available.",
            performance_curves(scenario, cruise_altitude)?,
        ),
        report_chart(
            7,
            "aerodynamics",
            "Shows the polar from the selected refinement when available.",
            evaluation_polar(scenario, polar_evaluation),
        ),
    ])
}

fn report_chart(order: u32, section: &str, purpose: &str, specification: ChartSpec) -> ReportChart {
    ReportChart {
        order,
        section: section.to_owned(),
        purpose: purpose.to_owned(),
        specification,
    }
}

fn evaluation_polar(scenario: &ResolvedScenario, evaluation: &BackendEvaluation) -> ChartSpec {
    ChartSpec {
        chart_type: "line".to_owned(),
        title: format!(
            "{}: {} drag polar",
            scenario.name, evaluation.analysis.provenance.backend
        ),
        x: AxisSpec {
            label: "Lift coefficient".to_owned(),
            unit: "1".to_owned(),
            values: evaluation
                .analysis
                .polar
                .iter()
                .map(|point| point.lift_coefficient)
                .collect(),
        },
        y: AxisSpec {
            label: "Drag coefficient".to_owned(),
            unit: "1".to_owned(),
            values: Vec::new(),
        },
        series: vec![SeriesSpec {
            id: "drag_coefficient".to_owned(),
            label: "CD".to_owned(),
            unit: "1".to_owned(),
            values: evaluation
                .analysis
                .polar
                .iter()
                .map(|point| point.drag_coefficient)
                .collect(),
        }],
        annotations: Vec::new(),
        warnings: evaluation.analysis.provenance.warnings.clone(),
    }
}

pub(crate) fn concept_report_markdown(report: &ConceptDesignReport) -> AexResult<String> {
    let mut output = String::new();
    writeln!(output, "# {}", report.scenario.name).map_err(format_error)?;
    writeln!(
        output,
        "\n- Conceptual feasibility: {}",
        if report.decision.feasible {
            "PASS"
        } else {
            "FAIL"
        }
    )
    .map_err(format_error)?;
    writeln!(
        output,
        "- Hard requirements: {}/{} passed",
        report.decision.hard_requirements_passed, report.decision.hard_requirements_total
    )
    .map_err(format_error)?;
    writeln!(output, "- Backend: {}", report.selected_backend).map_err(format_error)?;
    writeln!(output, "\n> {}", report.limitation).map_err(format_error)?;
    write_geometry_summary(&mut output, report)?;
    write_failed_constraints(&mut output, report)?;
    write_chart_index(&mut output, &report.charts)?;
    Ok(output)
}

fn write_geometry_summary(output: &mut String, report: &ConceptDesignReport) -> AexResult<()> {
    let evaluation = report
        .feasibility
        .refinement
        .as_ref()
        .unwrap_or(&report.feasibility.baseline);
    let geometry = &evaluation.geometry.metrics;
    writeln!(output, "\n## Geometry and packaging").map_err(format_error)?;
    writeln!(
        output,
        "\n- Wetted area: {:.1} m²",
        geometry.wetted_area.value
    )
    .map_err(format_error)?;
    writeln!(
        output,
        "- Wing area / span: {:.1} m² / {:.1} m",
        geometry.wing_area.value, geometry.wing_span.value
    )
    .map_err(format_error)?;
    if let Some(volume) = &geometry.estimated_usable_internal_volume {
        writeln!(
            output,
            "- Estimated usable internal volume: {:.1} m³",
            volume.value
        )
        .map_err(format_error)?;
    }
    Ok(())
}

fn write_failed_constraints(output: &mut String, report: &ConceptDesignReport) -> AexResult<()> {
    writeln!(output, "\n## Failed constraints").map_err(format_error)?;
    if report.decision.failed_constraints.is_empty() {
        writeln!(output, "\nNone in the registered conceptual screens.").map_err(format_error)?;
    }
    for constraint in &report.decision.failed_constraints {
        writeln!(output, "\n- {constraint}").map_err(format_error)?;
    }
    Ok(())
}

fn write_chart_index(output: &mut String, charts: &[ReportChart]) -> AexResult<()> {
    writeln!(output, "\n## Chart order").map_err(format_error)?;
    for chart in charts {
        writeln!(
            output,
            "\n{}. **{}** — {}",
            chart.order, chart.specification.title, chart.purpose
        )
        .map_err(format_error)?;
    }
    Ok(())
}

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

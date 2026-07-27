#![allow(clippy::expect_used)]

use std::collections::BTreeMap;

use crate::domain::evidence::{
    CandidateDescriptor, CandidateSummary, StudyRunResult, StudyTradeSurface,
};

use super::trade_space;

fn candidate(id: &str) -> CandidateSummary {
    CandidateSummary {
        candidate: CandidateDescriptor {
            candidate_id: id.to_owned(),
            baseline_digest: "a".repeat(64),
            parameters: BTreeMap::new(),
        },
        generation: 0,
        feasible: true,
        normalized_constraint_violation: 0.0,
        objective_values: BTreeMap::from([("fuel".to_owned(), 10.0), ("mass".to_owned(), 20.0)]),
        rank_score: 0.0,
        evidence_id: format!("eval_{id}"),
        backend: "native".to_owned(),
        fidelity_level: 1,
        failed_constraints: Vec::new(),
    }
}

#[test]
fn duplicate_trade_points_are_coalesced() {
    let result = StudyRunResult {
        study_id: "trade".to_owned(),
        study_digest: "a".repeat(64),
        archive_id: "archive_test".to_owned(),
        evaluated_candidates: 2,
        feasible_candidates: 2,
        reused_evaluations: 0,
        pareto_candidates: vec![candidate("candidate_one"), candidate("candidate_two")],
        selected_candidates: Vec::new(),
        archive_path: "archive.json".to_owned(),
        complete: true,
        trade_surface: None,
        irregular_trade_space: true,
    };

    let chart = trade_space(&result).expect("two objectives produce a chart");

    assert_eq!(chart.x.values, [10.0]);
    assert_eq!(chart.series[0].values, [20.0]);
    assert!(chart.annotations[0].label.ends_with("×2"));
    assert!(
        chart
            .warnings
            .iter()
            .any(|warning| warning.code == "IRREGULAR_TRADE_SPACE")
    );
}

#[test]
fn rectangular_grid_uses_variable_surface_instead_of_pareto_projection() {
    let result = StudyRunResult {
        study_id: "rectangular".to_owned(),
        study_digest: "a".repeat(64),
        archive_id: "archive_test".to_owned(),
        evaluated_candidates: 4,
        feasible_candidates: 3,
        reused_evaluations: 0,
        pareto_candidates: Vec::new(),
        selected_candidates: Vec::new(),
        archive_path: "archive.json".to_owned(),
        complete: true,
        trade_surface: Some(StudyTradeSurface {
            x_path: "wing_loading".to_owned(),
            x_unit: "N/m^2".to_owned(),
            x_values: vec![1.0, 2.0],
            y_path: "thrust_loading".to_owned(),
            y_unit: "1".to_owned(),
            y_values: vec![0.2, 0.3],
            objective_id: "fuel".to_owned(),
            values: vec![4.0, 3.0, 2.0, 1.0],
            feasible_mask: vec![true, true, false, true],
        }),
        irregular_trade_space: false,
    };

    let chart = trade_space(&result).expect("rectangular grid produces a surface");
    let surface = chart.surface.expect("surface data");

    assert_eq!(chart.chart_type, "carpet");
    assert_eq!(surface.values, [4.0, 3.0, 2.0, 1.0]);
    assert_eq!(surface.feasible_mask, [true, true, false, true]);
    assert!(chart.warnings.is_empty());
}

#[test]
fn infeasible_selection_chart_does_not_claim_a_pareto_set() {
    let result = StudyRunResult {
        study_id: "trade".to_owned(),
        study_digest: "a".repeat(64),
        archive_id: "archive_test".to_owned(),
        evaluated_candidates: 1,
        feasible_candidates: 0,
        reused_evaluations: 0,
        pareto_candidates: Vec::new(),
        selected_candidates: vec![candidate("candidate_one")],
        archive_path: "archive.json".to_owned(),
        complete: true,
        trade_surface: None,
        irregular_trade_space: true,
    };

    let chart = trade_space(&result).expect("two objectives produce a chart");

    assert!(chart.title.contains("selected trade space"));
    assert_eq!(chart.series[0].id, "selected_candidates");
}

#[test]
fn higher_dimensional_chart_declares_its_projection() {
    let mut projected = candidate("candidate_one");
    projected.objective_values.insert("range".to_owned(), 30.0);
    let result = StudyRunResult {
        study_id: "trade".to_owned(),
        study_digest: "a".repeat(64),
        archive_id: "archive_test".to_owned(),
        evaluated_candidates: 1,
        feasible_candidates: 1,
        reused_evaluations: 0,
        pareto_candidates: vec![projected],
        selected_candidates: Vec::new(),
        archive_path: "archive.json".to_owned(),
        complete: true,
        trade_surface: None,
        irregular_trade_space: true,
    };

    let chart = trade_space(&result).expect("three objectives produce a projected chart");

    assert_eq!(chart.warnings[0].code, "STUDY_CHART_PROJECTED");
    assert_eq!(
        chart.warnings[0].context["shown"],
        serde_json::json!(["fuel", "mass"])
    );
    assert_eq!(
        chart.warnings[0].context["omitted"],
        serde_json::json!(["range"])
    );
}

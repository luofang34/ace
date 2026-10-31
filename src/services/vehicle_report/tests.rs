use super::*;
use std::path::PathBuf;

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("vehicle.json")
}

#[test]
fn c172_report_counts_every_hard_requirement_and_charts_its_evidence() {
    let report = report_blocking(&example("c172"), &BTreeMap::new()).unwrap();
    let decision = &report.decision;
    assert_eq!(decision.feasibility, Feasibility::Provisional);
    assert_eq!(decision.hard_failed, 0);
    assert_eq!(decision.hard_unknown, 0);
    assert_eq!(decision.hard_satisfied, decision.hard_total);
    let sections: Vec<_> = report.charts.iter().map(|chart| chart.section).collect();
    assert_eq!(sections, ["decision", "sizing", "mission"]);
    assert_eq!(report.charts[2].analysis.as_deref(), Some("mission"));
    let masses = &report.charts[2].specification.series[0].values;
    assert!(
        masses.windows(2).all(|pair| pair[1] <= pair[0]),
        "{masses:?}"
    );
    let text = markdown(&report).unwrap();
    assert!(text.contains("| service_ceiling | FAIL |"), "{text}");
    assert!(text.contains("## Chart order"));
}

#[test]
fn out_of_domain_report_counts_unknown_hard_requirements_without_charts_of_failed_runs() {
    let report = report_blocking(&example("sr71"), &BTreeMap::new()).unwrap();
    assert_eq!(report.decision.feasibility, Feasibility::Unknown);
    assert!(report.decision.hard_unknown > 0);
    assert_eq!(report.decision.hard_failed, 0);
    assert!(!report.completed);
    let sections: Vec<_> = report.charts.iter().map(|chart| chart.section).collect();
    assert_eq!(sections, ["decision"]);
}

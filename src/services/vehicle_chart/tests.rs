use super::*;

fn b777() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/b777/vehicle.json")
}

fn code(result: AexResult<AnalysisChart>) -> &'static str {
    match result {
        Err(AexError::Validation { code, .. }) => code,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[test]
fn charts_name_declared_analyses_with_chartable_evidence() {
    let overrides = BTreeMap::new();
    assert_eq!(
        code(analysis_blocking(&b777(), &overrides, "wing", None)),
        "UNKNOWN_ANALYSIS"
    );
    assert_eq!(
        code(analysis_blocking(&b777(), &overrides, "performance", None)),
        "UNSUPPORTED_ANALYSIS_CHART"
    );
    let chart = analysis_blocking(&b777(), &overrides, "constraints", None).unwrap();
    assert_eq!(chart.plugin, "aircraft.constraint_diagram");
    assert_eq!(chart.spec.chart_type, "carpet");
    assert!(chart.artifact_path.is_none());
}

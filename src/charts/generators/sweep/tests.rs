#![allow(clippy::expect_used)]

use std::collections::BTreeMap;

use serde_json::json;

use crate::domain::result::{ResultProvenance, SweepResult, SweepRow};

use super::sweep;

fn row(x: f64, y: f64, value: f64, feasible: bool) -> SweepRow {
    SweepRow {
        variables: BTreeMap::from([
            ("design.x".to_owned(), json!(format!("{x} m"))),
            ("design.y".to_owned(), json!(format!("{y} 1"))),
        ]),
        metrics: BTreeMap::from([("objective".to_owned(), value)]),
        feasible,
        metric_validity: BTreeMap::new(),
        warnings: Vec::new(),
    }
}

fn result(rows: Vec<SweepRow>) -> SweepResult {
    SweepResult {
        scenario_id: "surface-test".to_owned(),
        rows,
        deterministic_ordering: true,
        warnings: Vec::new(),
        provenance: ResultProvenance {
            method: "test".to_owned(),
            backend: "native".to_owned(),
            assumptions: Vec::new(),
            validity_range: Vec::new(),
            validity_domains: Vec::new(),
            units: BTreeMap::new(),
            warnings: Vec::new(),
        },
    }
}

#[test]
fn rectangular_sweep_is_deterministic_row_major_surface() -> Result<(), Box<dyn std::error::Error>>
{
    let chart = sweep(
        &result(vec![
            row(1.0, 10.0, 110.0, true),
            row(1.0, 20.0, 120.0, true),
            row(2.0, 10.0, 210.0, false),
            row(2.0, 20.0, 220.0, false),
        ]),
        "objective",
    )?;
    let surface = chart.surface.ok_or("missing surface")?;

    assert_eq!(chart.x.values, [1.0, 2.0]);
    assert_eq!(chart.y.values, [10.0, 20.0]);
    assert_eq!(surface.values, [110.0, 210.0, 120.0, 220.0]);
    assert_eq!(surface.feasible_mask, [true, false, true, false]);
    assert_eq!(surface.contour_levels, [137.5, 165.0, 192.5]);
    Ok(())
}

#[test]
fn incomplete_two_variable_sweep_has_a_stable_error() {
    let error = sweep(
        &result(vec![
            row(1.0, 10.0, 110.0, true),
            row(2.0, 10.0, 210.0, false),
            row(1.0, 20.0, 120.0, true),
        ]),
        "objective",
    )
    .expect_err("missing Cartesian cell must fail");

    assert_eq!(error.detail().code, "IRREGULAR_SWEEP_GRID");
}

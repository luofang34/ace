#![allow(clippy::expect_used)]

use std::f64::consts::PI;

use super::PolarTable;

fn oswald_table() -> PolarTable {
    PolarTable {
        mach: vec![0.0, 1.0, 2.0],
        cd0: vec![0.02, 0.04, 0.03],
        cl_max: vec![1.4, 1.0, 0.8],
        oswald_efficiency: Some(vec![0.8, 0.6, 0.4]),
        induced_drag_factor: None,
    }
}

#[test]
fn table_interpolates_exact_and_interior_values() {
    let table = oswald_table();
    let exact = table.interpolate(1.0, 8.0).expect("valid table");
    let interior = table.interpolate(0.5, 8.0).expect("valid table");

    assert!((exact.cd0 - 0.04).abs() < 1.0e-12);
    assert!((exact.cl_max - 1.0).abs() < 1.0e-12);
    assert!((exact.induced_drag_factor - 1.0 / (PI * 0.6 * 8.0)).abs() < 1.0e-12);
    assert!((interior.cd0 - 0.03).abs() < 1.0e-12);
    assert!((interior.induced_drag_factor - 1.0 / (PI * 0.7 * 8.0)).abs() < 1.0e-12);
    assert!(!exact.extrapolated);
    assert!(!interior.extrapolated);
}

#[test]
fn table_extrapolates_from_the_nearest_interval() {
    let table = PolarTable {
        mach: vec![1.0, 2.0],
        cd0: vec![0.02, 0.04],
        cl_max: vec![1.0, 0.8],
        oswald_efficiency: None,
        induced_drag_factor: Some(vec![0.1, 0.2]),
    };
    let below = table.interpolate(0.5, 8.0).expect("valid table");
    let above = table.interpolate(2.5, 8.0).expect("valid table");

    assert!((below.cd0 - 0.01).abs() < 1.0e-12);
    assert!((below.induced_drag_factor - 0.05).abs() < 1.0e-12);
    assert!((above.cd0 - 0.05).abs() < 1.0e-12);
    assert!((above.induced_drag_factor - 0.25).abs() < 1.0e-12);
    assert!(below.extrapolated);
    assert!(above.extrapolated);
}

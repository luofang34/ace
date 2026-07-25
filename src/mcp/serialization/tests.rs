use serde_json::json;

use super::attach_units;

#[test]
fn ranges_default_to_nautical_miles() {
    let result = attach_units(json!({ "range_m": 1852.0 }));
    assert_eq!(result["range_m"]["unit"], "m");
    assert_eq!(result["range_m"]["display_unit"], "nmi");
    assert_eq!(result["range_m"]["display_value"], 1.0);
}

#[test]
fn achieved_cruise_metrics_carry_canonical_units() {
    let result = attach_units(json!({
        "performance.achieved_cruise_true_airspeed": 59.0,
        "performance.minimum_cruise_excess_power": 1200.0,
        "performance.achieved_cruise_mach": 0.18,
    }));

    assert_eq!(
        result["performance.achieved_cruise_true_airspeed"]["unit"],
        "m/s"
    );
    assert_eq!(
        result["performance.minimum_cruise_excess_power"]["unit"],
        "W"
    );
    assert_eq!(result["performance.achieved_cruise_mach"], 0.18);
}

use serde_json::json;

use crate::domain::presentation::DisplayUnitSystem;
use crate::domain::quantity::KNOT_M_S;

use super::attach_units_for;

#[test]
fn si_display_keeps_canonical_distance() {
    let result = attach_units_for(json!({ "range_m": 1852.0 }), DisplayUnitSystem::Si);
    assert_eq!(result["range_m"]["unit"], "m");
    assert_eq!(result["range_m"]["display_unit"], "m");
    assert_eq!(result["range_m"]["display_value"], 1852.0);
}

#[test]
fn aviation_display_converts_semantic_quantities() {
    let result = attach_units_for(
        json!({
            "range_m": 1852.0,
            "altitude_m": 304.8,
            "true_airspeed_m_s": 100.0 * KNOT_M_S,
            "mass_kg": 45.359_237,
        }),
        DisplayUnitSystem::AviationUs,
    );

    assert_eq!(result["range_m"]["display_unit"], "nmi");
    assert_eq!(result["range_m"]["display_value"], 1.0);
    assert_eq!(result["altitude_m"]["display_unit"], "ft");
    assert_eq!(result["altitude_m"]["display_value"], 1000.0);
    assert_eq!(result["true_airspeed_m_s"]["display_unit"], "kt");
    assert!(
        (result["true_airspeed_m_s"]["display_value"]
            .as_f64()
            .unwrap_or_default()
            - 100.0)
            .abs()
            < 1.0e-6
    );
    assert_eq!(result["mass_kg"]["display_unit"], "lb");
    assert_eq!(result["mass_kg"]["display_value"], 100.0);
}

#[test]
fn existing_quantity_objects_retain_canonical_values() {
    let result = attach_units_for(
        json!({
            "total_distance": {
                "value": 1852.0,
                "unit": "m",
                "display_value": 1852.0,
                "display_unit": "m"
            },
            "altitude": {
                "value": 304.8,
                "unit": "m",
                "display_value": 304.8,
                "display_unit": "m"
            }
        }),
        DisplayUnitSystem::AviationUs,
    );

    assert_eq!(result["total_distance"]["value"], 1852.0);
    assert_eq!(result["total_distance"]["unit"], "m");
    assert_eq!(result["total_distance"]["display_value"], 1.0);
    assert_eq!(result["total_distance"]["display_unit"], "nmi");
    assert_eq!(result["altitude"]["value"], 304.8);
    assert_eq!(result["altitude"]["display_value"], 1000.0);
    assert_eq!(result["altitude"]["display_unit"], "ft");
}

#[test]
fn requirement_quantities_use_metric_semantics() {
    let result = attach_units_for(
        json!({
            "requirements": [{
                "metric": "mission.completed_distance",
                "actual": {
                    "value": 1852.0,
                    "unit": "m",
                    "display_value": 1852.0,
                    "display_unit": "m"
                },
                "required": {
                    "value": 3704.0,
                    "unit": "m",
                    "display_value": 3704.0,
                    "display_unit": "m"
                }
            }, {
                "metric": "performance.service_ceiling",
                "actual": {
                    "value": 304.8,
                    "unit": "m",
                    "display_value": 304.8,
                    "display_unit": "m"
                },
                "required": {
                    "value": 609.6,
                    "unit": "m",
                    "display_value": 609.6,
                    "display_unit": "m"
                }
            }]
        }),
        DisplayUnitSystem::AviationUs,
    );

    assert_eq!(result["requirements"][0]["actual"]["display_unit"], "nmi");
    assert_eq!(result["requirements"][0]["actual"]["display_value"], 1.0);
    assert_eq!(result["requirements"][0]["required"]["display_value"], 2.0);
    assert_eq!(result["requirements"][1]["actual"]["display_unit"], "ft");
    assert_eq!(result["requirements"][1]["actual"]["display_value"], 1000.0);
}

#[test]
fn achieved_cruise_metrics_carry_canonical_units() {
    let result = attach_units_for(
        json!({
            "performance.achieved_cruise_true_airspeed": 59.0,
            "performance.minimum_cruise_excess_power": 1200.0,
            "performance.achieved_cruise_mach": 0.18,
        }),
        DisplayUnitSystem::Si,
    );

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

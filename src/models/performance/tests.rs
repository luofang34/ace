use crate::domain::validity::ValidityStatus;
use crate::test_support::example_scenario;

use super::PointAnalyzer;

#[test]
fn both_reference_aircraft_have_finite_service_ceilings() {
    for name in ["c172", "b777"] {
        let scenario = example_scenario(name);
        assert!(scenario.is_ok());
        if let Ok(resolved) = scenario {
            let summary = PointAnalyzer::new(resolved).summary();
            assert!(summary.is_ok());
            if let Ok(result) = summary {
                assert!(result.service_ceiling_m.is_finite());
                assert!(result.service_ceiling_m > 0.0);
                assert!(result.absolute_ceiling_m >= result.service_ceiling_m);
                assert_eq!(
                    result.validity_for("performance.service_ceiling").status,
                    ValidityStatus::Valid
                );
            }
        }
    }
}

#[test]
fn sr71_distinguishes_boundary_ceiling_from_extrapolated_speed()
-> Result<(), Box<dyn std::error::Error>> {
    let result = PointAnalyzer::new(example_scenario("sr71")?).summary()?;

    assert_eq!(result.service_ceiling_m, 19_900.0);
    assert_eq!(
        result.validity_for("performance.service_ceiling").status,
        ValidityStatus::BoundaryLimited
    );
    assert_eq!(
        result
            .validity_for("performance.maximum_level_speed")
            .status,
        ValidityStatus::Extrapolated
    );
    assert_eq!(result.model.validity_status, "boundary_limited");
    Ok(())
}

#[test]
fn maximum_speed_at_default_search_cap_is_boundary_limited()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.limits.maximum_operating_speed_m_s = None;
    scenario.aircraft.limits.maximum_operating_mach = None;
    scenario.aircraft.propulsion.sizing_factor = 1_000.0;
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    let result = PointAnalyzer::new(scenario).maximum_level_speed_solution(0.0, mass)?;

    assert_eq!(result.validity.status, ValidityStatus::BoundaryLimited);
    assert_eq!(
        result.validity.boundary.as_deref(),
        Some("performance.maximum_speed_search.default_mach")
    );
    Ok(())
}

#[test]
fn legacy_performance_json_defaults_metric_validity() -> Result<(), Box<dyn std::error::Error>> {
    let result = PointAnalyzer::new(example_scenario("c172")?).summary()?;
    let mut stored = serde_json::to_value(result)?;
    stored
        .as_object_mut()
        .ok_or("performance result must be an object")?
        .remove("metric_validity");
    let restored: crate::domain::result::PerformanceSummary = serde_json::from_value(stored)?;

    assert!(restored.metric_validity.is_empty());
    assert_eq!(
        restored.validity_for("performance.service_ceiling").status,
        ValidityStatus::Valid
    );
    Ok(())
}

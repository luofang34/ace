use crate::domain::diagnostic::AexResult;
use crate::domain::quantity::KNOT_M_S;
use crate::domain::result::PerformanceSummary;
use crate::domain::schema::ResolvedScenario;
use crate::domain::validity::ValidityStatus;
use crate::models::mission::MissionSimulator;
use crate::test_support::example_scenario;

use super::PointAnalyzer;

fn summary(scenario: ResolvedScenario) -> AexResult<PerformanceSummary> {
    let mission = match MissionSimulator::new(scenario.clone()).simulate() {
        Ok(mission) => Some(mission),
        Err(crate::domain::diagnostic::AexError::Analysis {
            code: "ATMOSPHERE_OUTSIDE_VALIDITY",
            ..
        }) => None,
        Err(error) => return Err(error),
    };
    PointAnalyzer::new(scenario).summary(mission.as_ref())
}

#[test]
fn both_reference_aircraft_have_finite_service_ceilings() {
    for name in ["c172", "b777"] {
        let scenario = example_scenario(name);
        assert!(scenario.is_ok());
        if let Ok(resolved) = scenario {
            let summary = summary(resolved);
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
    let result = summary(example_scenario("sr71")?)?;

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
    assert_eq!(result.cruise_mach, Some(3.2));
    assert_eq!(result.declared_cruise_mach, Some(3.2));
    assert_eq!(result.achieved_cruise_mach, None);
    assert_eq!(result.minimum_cruise_excess_power_w, None);
    assert_eq!(result.cruise_feasible, Some(false));
    assert!(result.warnings.iter().any(|warning| {
        warning.code == "CRUISE_CONDITION_UNSUPPORTED"
            && warning.path.as_deref() == Some("mission.segments.supersonic_cruise")
    }));
    Ok(())
}

#[test]
fn multiple_cruise_segments_aggregate_the_weakest_power_condition()
-> Result<(), Box<dyn std::error::Error>> {
    let result = summary(example_scenario("b777")?)?;
    let weakest = result
        .cruise_conditions
        .iter()
        .min_by(|left, right| left.excess_power_w.total_cmp(&right.excess_power_w))
        .ok_or("missing B777 cruise conditions")?;

    assert_eq!(result.cruise_conditions.len(), 4);
    assert_eq!(weakest.segment_id, "cruise_1");
    assert_eq!(result.achieved_cruise_mach, weakest.achieved_mach);
    assert_eq!(
        result.minimum_cruise_excess_power_w,
        Some(weakest.excess_power_w)
    );
    assert_eq!(result.cruise_feasible, Some(true));
    assert!(result.cruise_conditions.iter().any(|condition| {
        condition.segment_id == "alternate"
            && condition.declared_mach < weakest.declared_mach
            && condition.excess_power_w > weakest.excess_power_w
    }));
    Ok(())
}

#[test]
fn cruise_above_declared_operating_altitude_is_infeasible() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("b777")?;
    scenario.aircraft.limits.maximum_operating_altitude_m = Some(10_000.0 * 0.3048);
    let result = summary(scenario)?;

    assert_eq!(result.cruise_feasible, Some(false));
    assert!(
        result
            .cruise_conditions
            .iter()
            .all(|condition| !condition.feasible)
    );
    assert_eq!(
        result
            .warnings
            .iter()
            .filter(|warning| warning.code == "CRUISE_ALTITUDE_LIMIT_EXCEEDED")
            .count(),
        result.cruise_conditions.len()
    );
    assert!(result.warnings.iter().all(|warning| {
        warning.code != "CRUISE_ALTITUDE_LIMIT_EXCEEDED"
            || warning
                .path
                .as_deref()
                .is_some_and(|path| path.contains("aircraft.limits.maximum_operating_altitude"))
    }));
    Ok(())
}

#[test]
fn altitude_limit_does_not_mask_a_cruise_power_shortfall() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("b777")?;
    scenario.aircraft.limits.maximum_operating_altitude_m = Some(10_000.0 * 0.3048);
    scenario.aircraft.propulsion.sizing_factor = 0.5;
    scenario
        .mission
        .segments
        .retain(|segment| segment.kind != crate::domain::schema::SegmentKind::EnergyClimb);
    let result = summary(scenario)?;

    assert_eq!(result.cruise_feasible, Some(false));
    assert_eq!(result.cruise_conditions.len(), 4);
    assert!(
        result
            .minimum_cruise_excess_power_w
            .is_some_and(|power| power < -6_000_000.0)
    );
    assert_eq!(result.achieved_cruise_mach, None);
    assert_eq!(
        result
            .warnings
            .iter()
            .filter(|warning| warning.code == "CRUISE_CAPABILITY_UNAVAILABLE")
            .count(),
        2
    );
    Ok(())
}

fn assert_cruise_unavailable_below_stall(
    scenario: ResolvedScenario,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = summary(scenario)?;

    assert_eq!(result.achieved_cruise_true_airspeed_m_s, None);
    assert_eq!(result.achieved_cruise_mach, None);
    assert_eq!(result.cruise_feasible, Some(false));
    assert!(
        result
            .cruise_conditions
            .iter()
            .all(|condition| condition.achieved_true_airspeed_m_s.is_none())
    );
    assert!(result.warnings.iter().any(|warning| {
        warning.code == "CRUISE_CAPABILITY_UNAVAILABLE"
            && warning
                .path
                .as_deref()
                .is_some_and(|path| path.starts_with("mission.segments."))
    }));
    Ok(())
}

#[test]
fn operating_speed_limit_below_stall_has_no_achieved_cruise()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.limits.maximum_operating_speed_m_s = Some(40.0 * KNOT_M_S);
    assert_cruise_unavailable_below_stall(scenario)
}

#[test]
fn operating_mach_limit_below_stall_has_no_achieved_cruise()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.limits.maximum_operating_speed_m_s = None;
    scenario.aircraft.limits.maximum_operating_mach = Some(0.05);
    assert_cruise_unavailable_below_stall(scenario)
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
    let result = summary(example_scenario("c172")?)?;
    let mut stored = serde_json::to_value(result)?;
    let object = stored
        .as_object_mut()
        .ok_or("performance result must be an object")?;
    for field in [
        "declared_cruise_mach",
        "declared_cruise_true_airspeed_m_s",
        "achieved_cruise_mach",
        "achieved_cruise_true_airspeed_m_s",
        "minimum_cruise_excess_power_w",
        "cruise_feasible",
        "cruise_conditions",
        "metric_validity",
    ] {
        object.remove(field);
    }
    let restored: crate::domain::result::PerformanceSummary = serde_json::from_value(stored)?;

    assert!(restored.metric_validity.is_empty());
    assert_eq!(restored.declared_cruise_mach, None);
    assert_eq!(restored.achieved_cruise_mach, None);
    assert_eq!(restored.cruise_feasible, None);
    assert!(restored.cruise_conditions.is_empty());
    assert_eq!(
        restored.validity_for("performance.service_ceiling").status,
        ValidityStatus::Valid
    );
    Ok(())
}

use crate::backends::contracts::{
    AnalysisBackend, AnalysisRequest, GeometryBackend, GeometryRequest,
};
use crate::models::mission::MissionSimulator;
use crate::test_support::example_scenario;

use super::{NativeBackend, failed_constraints};

#[test]
fn native_analysis_publishes_validated_scenario_domains() -> Result<(), Box<dyn std::error::Error>>
{
    let scenario = example_scenario("b777")?;
    let backend = NativeBackend;
    let geometry = backend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let analysis = backend.analyze_blocking(AnalysisRequest {
        scenario: &scenario,
        geometry: &geometry,
    })?;
    let domains = &analysis.provenance.validity_domains;

    for expected in [
        "atmosphere.isa1976",
        "aero.parabolic_polar",
        "aero.wave_drag_power_law",
        "propulsion.turbofan_simple_deck",
        "performance.field_length_simple",
        "structures.conventional_conceptual_screen",
    ] {
        assert!(domains.iter().any(|domain| domain.model_id == expected));
    }
    for domain in domains {
        domain.validate()?;
    }
    for metric in [
        "performance.maximum_level_speed",
        "performance.service_ceiling",
        "performance.absolute_ceiling",
        "performance.achieved_cruise_mach",
        "performance.achieved_cruise_true_airspeed",
        "performance.minimum_cruise_excess_power",
        "performance.cruise_feasible",
    ] {
        assert!(analysis.metrics.contains_key(metric));
        assert!(analysis.metric_validity.contains_key(metric));
    }
    Ok(())
}

#[test]
fn cruise_capability_shortfall_is_a_native_feasibility_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.propulsion.sizing_factor = 0.5;
    let backend = NativeBackend;
    let geometry = backend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let analysis = backend.analyze_blocking(AnalysisRequest {
        scenario: &scenario,
        geometry: &geometry,
    })?;

    assert_eq!(analysis.feasible, Some(false));
    assert!(
        analysis
            .failed_constraints
            .iter()
            .any(|constraint| constraint == "cruise_capability")
    );
    assert_eq!(
        analysis
            .metrics
            .get("performance.cruise_feasible")
            .map(|value| value.value),
        Some(0.0)
    );
    Ok(())
}

#[test]
fn cruise_altitude_limit_breach_is_a_native_feasibility_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    scenario.aircraft.limits.maximum_operating_altitude_m = Some(10_000.0 * 0.3048);
    let backend = NativeBackend;
    let geometry = backend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let analysis = backend.analyze_blocking(AnalysisRequest {
        scenario: &scenario,
        geometry: &geometry,
    })?;

    assert_eq!(analysis.feasible, Some(false));
    assert!(
        analysis
            .failed_constraints
            .iter()
            .any(|constraint| constraint == "cruise_capability")
    );
    assert_eq!(analysis.metrics["performance.cruise_feasible"].value, 0.0);
    Ok(())
}

#[test]
fn fuel_failure_constraint_labels_are_independent() -> Result<(), Box<dyn std::error::Error>> {
    let mut capacity = MissionSimulator::new(example_scenario("c172")?).simulate()?;
    capacity.fuel_capacity_violation = true;
    let capacity_failures = failed_constraints(&[], &[], &capacity);
    assert!(
        capacity_failures
            .iter()
            .any(|constraint| constraint == "fuel_capacity")
    );
    assert!(
        !capacity_failures
            .iter()
            .any(|constraint| constraint == "fuel_exhausted")
    );

    let exhaustion = MissionSimulator::new(example_scenario("x15")?).simulate()?;
    let exhaustion_failures = failed_constraints(&[], &[], &exhaustion);
    assert!(
        exhaustion_failures
            .iter()
            .any(|constraint| constraint == "fuel_exhausted")
    );
    assert!(
        !exhaustion_failures
            .iter()
            .any(|constraint| constraint == "fuel_capacity")
    );
    Ok(())
}

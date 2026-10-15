use crate::domain::aerodynamics::PolarTable;
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
                let service = result.service_ceiling_m.unwrap_or(f64::NAN);
                assert!(service.is_finite());
                assert!(service > 0.0);
                assert!(result.absolute_ceiling_m.unwrap_or(f64::NAN) >= service);
                assert_eq!(
                    result.validity_for("performance.service_ceiling").status,
                    ValidityStatus::Valid
                );
                // The B777 out-climbs its declared maximum operating altitude, which
                // cuts off the published ceiling; the C172 is performance-limited.
                let physical = result.physical_service_ceiling_m.unwrap_or(f64::NAN);
                if name == "b777" {
                    assert!(service < physical);
                    assert_eq!(
                        result.ceiling_limited_by.as_deref(),
                        Some("aircraft.limits.maximum_operating_altitude")
                    );
                } else {
                    assert_eq!(physical, service);
                    assert_eq!(result.ceiling_limited_by, None);
                }
            }
        }
    }
}

#[test]
fn zero_mach_reference_metrics_retain_configuration_validity()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    for configuration in [
        &mut scenario.aircraft.aerodynamics.clean,
        &mut scenario.aircraft.aerodynamics.landing,
    ] {
        configuration.polar_table = Some(PolarTable {
            mach: vec![0.2, 0.8],
            cd0: vec![configuration.cd0; 2],
            cl_max: vec![configuration.cl_max; 2],
            oswald_efficiency: Some(vec![configuration.oswald_efficiency; 2]),
            induced_drag_factor: None,
        });
    }

    let result = summary(scenario)?;
    for metric in [
        "performance.stall_speed",
        "performance.stall_speed_clean",
        "performance.best_glide_speed",
        "performance.minimum_power_speed",
        "aerodynamics.maximum_lift_to_drag_ratio",
        "performance.stall_speed_landing",
    ] {
        assert_eq!(
            result.validity_for(metric).status,
            ValidityStatus::Extrapolated,
            "{metric}"
        );
    }
    for configuration in ["clean", "landing"] {
        let path = format!("aircraft.aerodynamics.{configuration}.polar_table.mach");
        assert!(result.warnings.iter().any(|warning| {
            warning.code == "MODEL_EXTRAPOLATION" && warning.path.as_deref() == Some(path.as_str())
        }));
    }
    Ok(())
}

#[test]
fn point_result_uses_configuration_reference_validity_and_legacy_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let clean = &mut scenario.aircraft.aerodynamics.clean;
    clean.polar_table = Some(PolarTable {
        mach: vec![0.0, 0.8],
        cd0: vec![clean.cd0; 2],
        cl_max: vec![clean.cl_max; 2],
        oswald_efficiency: Some(vec![clean.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });
    let landing = &mut scenario.aircraft.aerodynamics.landing;
    landing.polar_table = Some(PolarTable {
        mach: vec![0.2, 0.8],
        cd0: vec![landing.cd0; 2],
        cl_max: vec![landing.cl_max; 2],
        oswald_efficiency: Some(vec![landing.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });
    let result = PointAnalyzer::new(scenario).point(0.0, 170.0, 1_000.0, "landing")?;

    assert_eq!(
        result.metric_validity["performance.stall_speed"].status,
        ValidityStatus::Extrapolated
    );
    for metric in [
        "performance.best_glide_speed",
        "aerodynamics.maximum_lift_to_drag_ratio",
    ] {
        assert_eq!(result.metric_validity[metric].status, ValidityStatus::Valid);
    }
    assert!(result.warnings.iter().any(|warning| {
        warning.code == "MODEL_EXTRAPOLATION"
            && warning.path.as_deref() == Some("aircraft.aerodynamics.landing.polar_table.mach")
    }));
    let mut stored = serde_json::to_value(result)?;
    stored
        .as_object_mut()
        .ok_or("point result must be an object")?
        .remove("metric_validity");
    let legacy: crate::domain::result::PointPerformanceResult = serde_json::from_value(stored)?;
    assert!(legacy.metric_validity.is_empty());
    Ok(())
}

#[test]
fn sr71_solves_its_stratospheric_ceiling_and_publishes_it_cut_off_at_the_limit()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("sr71")?;
    scenario.aircraft.propulsion.sizing_factor = 2.0;
    let limit = scenario
        .aircraft
        .limits
        .maximum_operating_altitude_m
        .ok_or("SR-71 declares a maximum operating altitude")?;
    let result = summary(scenario)?;

    let physical = result
        .physical_service_ceiling_m
        .ok_or("ceiling is solved above 20 km")?;
    assert!(physical > limit, "{physical} m");
    assert!(result.physical_absolute_ceiling_m.unwrap_or(0.0) >= physical);
    assert_eq!(result.service_ceiling_m, Some(limit));
    assert_eq!(result.absolute_ceiling_m, Some(limit));
    assert_eq!(
        result.ceiling_limited_by.as_deref(),
        Some("aircraft.limits.maximum_operating_altitude")
    );
    assert_eq!(
        result
            .validity_for("performance.maximum_level_speed")
            .status,
        ValidityStatus::Valid
    );
    assert_eq!(result.cruise_mach, Some(3.2));
    assert_eq!(result.declared_cruise_mach, Some(3.2));
    // The stratospheric Mach 3.2 cruise is evaluated inside the 1976 atmosphere.
    let supersonic = result
        .cruise_conditions
        .iter()
        .find(|condition| condition.segment_id == "supersonic_cruise")
        .ok_or("supersonic cruise is evaluated")?;
    assert!(supersonic.altitude_m > 20_000.0);
    assert_eq!(supersonic.achieved_mach, Some(3.2));
    assert!(supersonic.feasible);
    assert_eq!(result.cruise_feasible, Some(true));
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

#[test]
fn point_reports_required_power_fuel_flow_separately_from_full_throttle()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("c172")?;
    let crate::domain::schema::EngineProfile::Piston(engine) = &scenario.engine else {
        return Err("C172 uses a shaft engine".into());
    };
    let bsfc = engine.bsfc.cruise_kg_kwh;
    let efficiency = scenario
        .propeller
        .as_ref()
        .ok_or("C172 declares a propeller")?
        .cruise_efficiency;
    let point = PointAnalyzer::new(scenario).point(1_500.0, 45.0, 1_000.0, "clean")?;
    // BSFC * P_required / eta, independent of the shared implementation.
    let expected = bsfc * point.power_required_w / efficiency / 3.6e6;
    assert!((point.fuel_flow_required_kg_s - expected).abs() < 1e-12);
    assert_eq!(point.propeller_efficiency, Some(efficiency));
    assert_eq!(
        point.fuel_flow_max_power_kg_s,
        point.propulsion.fuel_flow_kg_s
    );
    assert!(point.fuel_flow_required_kg_s < point.fuel_flow_max_power_kg_s);
    Ok(())
}

#[test]
fn declared_isa_offset_changes_point_and_mission_results() -> Result<(), Box<dyn std::error::Error>>
{
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resolver = crate::services::resolver::ScenarioResolver::new(std::sync::Arc::new(
        crate::storage::profile_store::FileProfileStore,
    ));
    let hot = resolver.resolve_blocking(
        &root.join("examples/c172/scenario.yaml"),
        &std::collections::BTreeMap::from([(
            "mission.atmosphere.isa_offset".to_owned(),
            "20 degC".to_owned(),
        )]),
    )?;
    assert_eq!(hot.mission.isa_offset_k, 20.0);
    let standard = example_scenario("c172")?;
    let point = |scenario: ResolvedScenario| {
        PointAnalyzer::new(scenario).point(1_500.0, 50.0, 1_000.0, "clean")
    };
    let (hot_point, standard_point) = (point(hot.clone())?, point(standard.clone())?);
    assert!(hot_point.rate_of_climb_m_s < standard_point.rate_of_climb_m_s);
    assert!(hot_point.mach < standard_point.mach);
    let fuel = |scenario: ResolvedScenario| {
        MissionSimulator::new(scenario)
            .simulate()
            .map(|mission| mission.total_fuel_burn_kg)
    };
    assert_ne!(fuel(hot)?, fuel(standard)?);
    Ok(())
}

#[test]
fn forward_cg_raises_trimmed_stall_speed_and_power_required()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("c172")?;
    let stations = crate::models::mass_properties::longitudinal_stations(&scenario)
        .ok_or("C172 has a horizontal tail")?;
    let analyzer = PointAnalyzer::new(scenario);
    let at = |cg| analyzer.point_at_cg(1_500.0, 50.0, 1_000.0, "clean", cg);
    let untrimmed = at(None)?;
    let neutral = at(Some(stations.wing_aerodynamic_center_m))?;
    // A CG at the wing aerodynamic center needs no tail load.
    assert!((neutral.stall_speed_m_s - untrimmed.stall_speed_m_s).abs() < 1e-9);
    assert_eq!(neutral.trim_wing_lift_fraction, Some(1.0));
    let forward = at(Some(stations.wing_aerodynamic_center_m - 0.3))?;
    let aft = at(Some(stations.wing_aerodynamic_center_m + 0.2))?;
    assert!(forward.stall_speed_m_s > untrimmed.stall_speed_m_s);
    assert!(forward.power_required_w > aft.power_required_w);
    assert!(forward.trim_wing_lift_fraction.unwrap_or(0.0) > 1.0);
    assert!(aft.trim_wing_lift_fraction.unwrap_or(2.0) < 1.0);
    // Weight, not wing lift, sets the climb rate denominator.
    assert_eq!(forward.mass_kg, 1_000.0);
    assert!(at(Some(stations.tail_aerodynamic_center_m + 0.1)).is_err());
    Ok(())
}

#[test]
fn cg_trim_uses_a_declared_wing_station_and_flags_the_correlated_one()
-> Result<(), Box<dyn std::error::Error>> {
    let correlated = example_scenario("c172")?;
    let estimated = crate::models::mass_properties::longitudinal_stations(&correlated)
        .ok_or("C172 has a horizontal tail")?;
    assert!(!estimated.wing_station_declared);
    let point = PointAnalyzer::new(correlated.clone()).point_at_cg(
        1_500.0,
        50.0,
        1_000.0,
        "clean",
        Some(estimated.wing_aerodynamic_center_m),
    )?;
    assert!(
        point
            .warnings
            .iter()
            .any(|warning| warning.code == "CORRELATED_GEOMETRY_STATION")
    );

    let mut declared = correlated;
    declared.aircraft.wing.aerodynamic_center_m = Some(estimated.wing_aerodynamic_center_m + 0.5);
    let stations = crate::models::mass_properties::longitudinal_stations(&declared)
        .ok_or("C172 has a horizontal tail")?;
    assert!(stations.wing_station_declared);
    assert_eq!(
        stations.wing_aerodynamic_center_m,
        estimated.wing_aerodynamic_center_m + 0.5
    );
    let point = PointAnalyzer::new(declared).point_at_cg(
        1_500.0,
        50.0,
        1_000.0,
        "clean",
        Some(stations.wing_aerodynamic_center_m),
    )?;
    assert_eq!(point.trim_wing_lift_fraction, Some(1.0));
    assert!(
        !point
            .warnings
            .iter()
            .any(|warning| warning.code == "CORRELATED_GEOMETRY_STATION")
    );
    Ok(())
}

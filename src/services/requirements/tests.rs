use crate::domain::aerodynamics::PolarTable;
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{RequirementEvaluation, RequirementStatus};
use crate::domain::schema::{MissionInitialState, Requirement, RequirementProvenance, SegmentKind};
use crate::domain::validity::{MetricValidity, ValidityStatus};
use crate::models::mission::MissionSimulator;
use crate::models::payload_range::PayloadRangeAnalyzer;
use crate::models::performance::PointAnalyzer;
use crate::test_support::{
    example_scenario, low_landing_fuel_sr71_scenario, set_inferred_configuration,
};

use super::{
    MetricInput, evaluate_one, evaluate_requirements, failed_hard_requirement_ids,
    hard_requirement_counts, hard_requirements_passed,
};

fn declared(id: &str, severity: &str) -> Requirement {
    Requirement {
        id: id.to_owned(),
        metric: "mission.completed_distance".to_owned(),
        operator: "ge".to_owned(),
        required: 1.0,
        unit: "m".to_owned(),
        severity: severity.to_owned(),
        weight: None,
        provenance: test_provenance(),
    }
}

fn evaluated(id: &str, severity: &str, passed: bool) -> RequirementEvaluation {
    RequirementEvaluation {
        id: id.to_owned(),
        metric: "mission.completed_distance".to_owned(),
        actual: QuantityOutput::si(1.0, "m"),
        required: QuantityOutput::si(1.0, "m"),
        operator: "ge".to_owned(),
        status: Some(if passed {
            RequirementStatus::Pass
        } else {
            RequirementStatus::Fail
        }),
        passed: Some(passed),
        validity: MetricValidity::default(),
        absolute_margin: 0.0,
        percentage_margin: Some(0.0),
        severity: severity.to_owned(),
        warning_state: false,
        provenance: None,
    }
}

#[test]
fn headline_verdict_requires_completion_and_every_hard_requirement() {
    let declared = [declared("range", "hard"), declared("ceiling", "soft")];
    let passing = [
        evaluated("range", "hard", true),
        evaluated("ceiling", "soft", false),
    ];
    assert!(hard_requirements_passed(true, &declared, &passing));
    assert!(!hard_requirements_passed(false, &declared, &passing));

    let failing = [evaluated("range", "hard", false)];
    assert!(!hard_requirements_passed(true, &declared, &failing));
    assert!(!hard_requirements_passed(true, &declared, &[]));
    assert_eq!(hard_requirement_counts(&declared, &passing), (1, 1));
    assert_eq!(
        failed_hard_requirement_ids(&declared, &[]),
        ["range".to_owned()]
    );
}

#[test]
fn sr71_boundary_ceiling_requirement_is_indeterminate_and_nullable()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("sr71")?;
    scenario.aircraft.propulsion.sizing_factor = 2.0;
    let performance = PointAnalyzer::new(scenario.clone()).summary(None)?;
    let requirement = scenario
        .requirements
        .items
        .iter()
        .find(|item| item.metric == "performance.service_ceiling")
        .ok_or("missing SR-71 service-ceiling requirement")?;
    let evaluation = evaluate_one(
        requirement,
        MetricInput {
            actual: performance.service_ceiling_m,
            validity: performance.validity_for("performance.service_ceiling"),
        },
    );

    assert_eq!(
        evaluation.resolved_status(),
        RequirementStatus::Indeterminate
    );
    assert_eq!(evaluation.passed, None);
    assert_eq!(evaluation.validity.status, ValidityStatus::BoundaryLimited);
    let stored = serde_json::to_value(&evaluation)?;
    assert_eq!(stored["status"], "indeterminate");
    assert!(stored["passed"].is_null());
    assert!(!hard_requirements_passed(
        true,
        &scenario.requirements.items,
        &[evaluation]
    ));
    Ok(())
}

#[test]
fn landing_stall_requirement_retains_reference_extrapolation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let landing = &mut scenario.aircraft.aerodynamics.landing;
    landing.polar_table = Some(PolarTable {
        mach: vec![0.2, 0.8],
        cd0: vec![landing.cd0; 2],
        cl_max: vec![landing.cl_max; 2],
        oswald_efficiency: Some(vec![landing.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
    let evaluations = evaluate_requirements(&scenario, &mission, &performance, None)?;
    let evaluation = evaluations
        .iter()
        .find(|item| item.metric == "performance.stall_speed_landing")
        .ok_or("missing landing-stall evaluation")?;

    assert_eq!(evaluation.validity.status, ValidityStatus::Extrapolated);
    Ok(())
}

#[test]
fn legacy_boolean_requirement_result_remains_readable() -> Result<(), Box<dyn std::error::Error>> {
    let mut stored = serde_json::to_value(evaluated("range", "hard", true))?;
    let object = stored
        .as_object_mut()
        .ok_or("requirement result must be an object")?;
    object.remove("status");
    object.remove("validity");
    let restored: RequirementEvaluation = serde_json::from_value(stored)?;

    assert_eq!(restored.passed, Some(true));
    assert_eq!(restored.resolved_status(), RequirementStatus::Pass);
    assert_eq!(restored.validity, MetricValidity::default());
    Ok(())
}

#[test]
fn achieved_cruise_requirement_fails_when_installed_power_cannot_close()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.propulsion.sizing_factor = 0.5;
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
    let evaluations = evaluate_requirements(&scenario, &mission, &performance, None)?;
    let cruise = evaluations
        .iter()
        .find(|evaluation| evaluation.id == "cruise_speed")
        .ok_or("missing achieved cruise-speed requirement")?;

    assert_eq!(cruise.metric, "performance.achieved_cruise_true_airspeed");
    assert_eq!(cruise.resolved_status(), RequirementStatus::Fail);
    assert_eq!(performance.cruise_feasible, Some(false));
    assert!(
        performance
            .achieved_cruise_true_airspeed_m_s
            .zip(performance.declared_cruise_true_airspeed_m_s)
            .is_some_and(|(achieved, declared)| achieved < declared)
    );
    assert!(!hard_requirements_passed(
        mission.completed,
        &scenario.requirements.items,
        &evaluations
    ));
    Ok(())
}

#[test]
fn altitude_limit_does_not_turn_a_power_shortfall_into_a_requirement_pass()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    scenario.aircraft.limits.maximum_operating_altitude_m = Some(10_000.0 * 0.3048);
    scenario.aircraft.propulsion.sizing_factor = 0.5;
    scenario
        .mission
        .segments
        .retain(|segment| segment.kind != SegmentKind::EnergyClimb);
    let requirement = scenario
        .requirements
        .items
        .iter_mut()
        .find(|item| item.id == "cruise_mach")
        .ok_or("missing declared cruise-Mach requirement")?;
    requirement.required = 0.84;
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
    let evaluations = evaluate_requirements(&scenario, &mission, &performance, None)?;
    assert!(
        evaluations
            .iter()
            .all(|evaluation| evaluation.id != "cruise_mach")
    );
    assert!(!hard_requirements_passed(
        mission.completed,
        &scenario.requirements.items,
        &evaluations
    ));
    assert!(
        performance
            .minimum_cruise_excess_power_w
            .is_some_and(|power| power < -6_000_000.0)
    );
    assert_eq!(performance.achieved_cruise_mach, None);
    assert_eq!(performance.cruise_conditions.len(), 4);
    assert_eq!(performance.cruise_feasible, Some(false));
    Ok(())
}

#[test]
fn achieved_cruise_requirement_uses_the_simulated_inherited_speed()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let mut cruise = scenario
        .mission
        .segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Cruise)
        .cloned()
        .ok_or("missing C172 cruise segment")?;
    cruise.distance_m = Some(1_000.0);
    cruise.indicated_airspeed_m_s = None;
    cruise.true_airspeed_m_s = None;
    cruise.mach = None;
    scenario.mission.segments = vec![cruise];
    scenario.mission.initial_state = Some(MissionInitialState {
        altitude_m: Some(0.0),
        indicated_airspeed_m_s: None,
        true_airspeed_m_s: Some(60.0),
        mach: None,
        fuel_fraction: Some(1.0),
        fuel_mass_kg: None,
    });

    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
    let evaluations = evaluate_requirements(&scenario, &mission, &performance, None)?;
    let cruise = evaluations
        .iter()
        .find(|evaluation| evaluation.id == "cruise_speed")
        .ok_or("missing achieved cruise-speed requirement")?;

    assert_eq!(performance.declared_cruise_true_airspeed_m_s, Some(60.0));
    assert_eq!(performance.cruise_conditions.len(), 1);
    assert!((performance.cruise_conditions[0].declared_true_airspeed_m_s - 60.0).abs() < 1.0e-8);
    assert_eq!(cruise.resolved_status(), RequirementStatus::Pass);
    Ok(())
}

#[test]
fn hard_landing_fuel_floor_uses_completed_mission_fuel() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = low_landing_fuel_sr71_scenario()?;
    scenario.requirements.items = vec![Requirement {
        id: "landing_fuel_floor".to_owned(),
        metric: "mission.landing_fuel".to_owned(),
        operator: "ge".to_owned(),
        required: 1_000.0,
        unit: "kg".to_owned(),
        severity: "hard".to_owned(),
        weight: None,
        provenance: test_provenance(),
    }];
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
    let evaluations = evaluate_requirements(&scenario, &mission, &performance, None)?;
    let landing_fuel = evaluations
        .first()
        .ok_or("missing landing-fuel requirement evaluation")?;

    assert_eq!(landing_fuel.metric, "mission.landing_fuel");
    assert_eq!(landing_fuel.actual.unit, "kg");
    assert_eq!(landing_fuel.resolved_status(), RequirementStatus::Fail);
    assert!(!hard_requirements_passed(
        mission.completed,
        &scenario.requirements.items,
        &evaluations
    ));
    Ok(())
}

fn test_provenance() -> RequirementProvenance {
    RequirementProvenance {
        kind: "test".to_owned(),
        source: "test fixture".to_owned(),
        citation: None,
        non_regulatory: true,
        template_id: None,
        template_version: None,
    }
}

#[test]
fn shipped_templates_evaluate_achieved_checks_without_changing_hard_verdicts()
-> Result<(), Box<dyn std::error::Error>> {
    for (name, template_id, reserve_s) in [
        ("c172", "light_aircraft_conceptual", 2_700.0),
        ("b777", "transport_conceptual", 1_800.0),
    ] {
        let scenario = example_scenario(name)?;
        let mission = MissionSimulator::new(scenario.clone()).simulate()?;
        let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
        let payload_range = PayloadRangeAnalyzer::new(scenario.clone()).analyze()?;
        let evaluations =
            evaluate_requirements(&scenario, &mission, &performance, Some(&payload_range))?;
        let template_items = scenario
            .requirements
            .items
            .iter()
            .filter(|requirement| {
                requirement.provenance.template_id.as_deref() == Some(template_id)
            })
            .collect::<Vec<_>>();
        assert!(!template_items.is_empty());
        assert!(template_items.iter().all(|requirement| {
            evaluations
                .iter()
                .find(|evaluation| evaluation.id == requirement.id)
                .and_then(|evaluation| evaluation.provenance.as_ref())
                .is_some_and(|provenance| provenance.template_id.as_deref() == Some(template_id))
        }));
        let reserve = evaluations
            .iter()
            .find(|evaluation| evaluation.id == "reserve_duration")
            .ok_or("missing reserve-duration evaluation")?;
        assert_eq!(reserve.actual.value, reserve_s);
        assert!(hard_requirements_passed(
            mission.completed,
            &scenario.requirements.items,
            &evaluations
        ));
    }
    Ok(())
}

#[test]
fn tailless_static_margin_requirement_is_indeterminate_and_nullable()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    set_inferred_configuration(&mut scenario, "tailless_flying_wing")?;
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
    let evaluations = evaluate_requirements(&scenario, &mission, &performance, None)?;
    let static_margin = evaluations
        .iter()
        .find(|item| item.metric == "stability.minimum_static_margin")
        .ok_or("missing static-margin evaluation")?;

    assert_eq!(
        static_margin.resolved_status(),
        RequirementStatus::Indeterminate
    );
    assert_eq!(static_margin.passed, None);
    assert_eq!(static_margin.validity.status, ValidityStatus::Unsupported);
    let stored = serde_json::to_value(static_margin)?;
    assert_eq!(stored["validity"]["status"], "unsupported");
    assert!(stored["passed"].is_null());
    assert!(!hard_requirements_passed(
        mission.completed,
        &scenario.requirements.items,
        &evaluations
    ));
    Ok(())
}

#[test]
fn c172_static_margin_bounds_are_both_bindable() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.requirements.items.extend([
        Requirement {
            id: "static_margin_upper".to_owned(),
            metric: "stability.maximum_static_margin".to_owned(),
            operator: "le".to_owned(),
            required: 0.25,
            unit: "1".to_owned(),
            severity: "hard".to_owned(),
            weight: None,
            provenance: test_provenance(),
        },
        Requirement {
            id: "forward_cg".to_owned(),
            metric: "mass_properties.minimum_center_of_gravity".to_owned(),
            operator: "ge".to_owned(),
            required: 3.30,
            unit: "m".to_owned(),
            severity: "hard".to_owned(),
            weight: None,
            provenance: test_provenance(),
        },
        Requirement {
            id: "aft_cg".to_owned(),
            metric: "mass_properties.maximum_center_of_gravity".to_owned(),
            operator: "le".to_owned(),
            required: 3.50,
            unit: "m".to_owned(),
            severity: "hard".to_owned(),
            weight: None,
            provenance: test_provenance(),
        },
    ]);
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
    let evaluations = evaluate_requirements(&scenario, &mission, &performance, None)?;

    for (id, metric) in [
        ("static_margin", "stability.minimum_static_margin"),
        ("static_margin_upper", "stability.maximum_static_margin"),
        ("forward_cg", "mass_properties.minimum_center_of_gravity"),
        ("aft_cg", "mass_properties.maximum_center_of_gravity"),
    ] {
        let evaluation = evaluations
            .iter()
            .find(|item| item.id == id)
            .ok_or("missing static-margin bound evaluation")?;
        assert_eq!(evaluation.metric, metric);
        assert_eq!(evaluation.resolved_status(), RequirementStatus::Pass);
    }
    Ok(())
}

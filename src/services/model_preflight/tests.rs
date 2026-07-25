#![allow(clippy::expect_used, clippy::panic)]

use crate::domain::diagnostic::AexError;
use crate::domain::schema::{EngineProfile, MissionInitialState};
use crate::test_support::example_scenario;

use super::{declarations, preflight_model_domains, preflight_operating_point};

#[test]
fn reference_aircraft_remain_inside_registered_domains() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["c172", "b777"] {
        preflight_model_domains(&example_scenario(name)?)?;
    }
    Ok(())
}

#[test]
fn every_aircraft_mass_declaration_is_preflight_input() -> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("c172")?;
    let declarations = declarations(&scenario)?;
    for path in [
        "aircraft.mass.maximum_takeoff_mass",
        "aircraft.mass.operating_empty_mass",
        "aircraft.mass.maximum_payload_mass",
        "aircraft.mass.maximum_fuel_mass",
    ] {
        assert!(declarations.iter().any(|item| item.path == path));
    }
    Ok(())
}

#[test]
fn initial_state_adds_effective_speed_altitude_and_mass_declarations()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.mission.initial_state = Some(MissionInitialState {
        altitude_m: Some(1_000.0),
        indicated_airspeed_m_s: Some(50.0),
        true_airspeed_m_s: None,
        mach: None,
        fuel_fraction: Some(0.5),
        fuel_mass_kg: None,
    });
    let values = declarations(&scenario)?;

    assert!(values.iter().any(|item| {
        item.path == "mission.initial_state.altitude"
            && item.variable == crate::domain::validity::ValidityVariable::Altitude
    }));
    for variable in [
        crate::domain::validity::ValidityVariable::TrueAirspeed,
        crate::domain::validity::ValidityVariable::Mach,
    ] {
        assert!(values.iter().any(|item| {
            item.path == "mission.initial_state.indicated_airspeed" && item.variable == variable
        }));
    }
    assert!(values.iter().any(|item| {
        item.path == "mission.initial_state.mass"
            && item.variable == crate::domain::validity::ValidityVariable::Mass
    }));
    Ok(())
}

#[test]
fn zero_initial_fuel_registers_only_the_positive_total_mass()
-> Result<(), Box<dyn std::error::Error>> {
    for state in [
        MissionInitialState {
            altitude_m: None,
            indicated_airspeed_m_s: None,
            true_airspeed_m_s: None,
            mach: None,
            fuel_fraction: None,
            fuel_mass_kg: Some(0.0),
        },
        MissionInitialState {
            altitude_m: None,
            indicated_airspeed_m_s: None,
            true_airspeed_m_s: None,
            mach: None,
            fuel_fraction: Some(0.0),
            fuel_mass_kg: None,
        },
    ] {
        let mut scenario = example_scenario("c172")?;
        scenario.mission.initial_state = Some(state);
        let values = declarations(&scenario)?;
        assert!(
            values
                .iter()
                .all(|item| item.path != "mission.initial_state.fuel_mass")
        );
        assert!(
            values
                .iter()
                .any(|item| { item.path == "mission.initial_state.mass" && item.value > 0.0 })
        );
        preflight_model_domains(&scenario)?;
    }
    Ok(())
}

#[test]
fn derived_initial_speed_is_checked_against_aircraft_limits()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.mission.initial_state = Some(MissionInitialState {
        altitude_m: Some(2_000.0),
        indicated_airspeed_m_s: Some(90.0),
        true_airspeed_m_s: None,
        mach: None,
        fuel_fraction: None,
        fuel_mass_kg: None,
    });
    let error = preflight_model_domains(&scenario)
        .err()
        .ok_or("initial IAS unexpectedly passed aircraft limits")?;

    assert_eq!(error.detail().code, "INITIAL_SPEED_LIMIT_EXCEEDED");
    assert_eq!(
        error.detail().path.as_deref(),
        Some("mission.initial_state.indicated_airspeed")
    );
    Ok(())
}

#[test]
fn initial_speed_outside_model_domain_uses_its_declared_path()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.limits.maximum_operating_speed_m_s = None;
    scenario.mission.initial_state = Some(MissionInitialState {
        altitude_m: Some(0.0),
        indicated_airspeed_m_s: None,
        true_airspeed_m_s: Some(400.0),
        mach: None,
        fuel_fraction: None,
        fuel_mass_kg: None,
    });
    let error = preflight_model_domains(&scenario)
        .err()
        .ok_or("initial speed unexpectedly passed model domains")?;
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.iter().any(|item| {
        item.path == "mission.initial_state.true_airspeed"
            && item.variable == crate::domain::validity::ValidityVariable::Mach
    }));
    Ok(())
}

#[test]
fn initial_altitude_outside_model_domain_is_rejected_before_simulation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.mission.initial_state = Some(MissionInitialState {
        altitude_m: Some(21_000.0),
        indicated_airspeed_m_s: None,
        true_airspeed_m_s: None,
        mach: None,
        fuel_fraction: None,
        fuel_mass_kg: None,
    });
    let error = preflight_model_domains(&scenario)
        .err()
        .ok_or("initial altitude unexpectedly passed model domains")?;
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.iter().any(|item| {
        item.path == "mission.initial_state.altitude" && item.model_id == "atmosphere.isa1976"
    }));
    Ok(())
}

#[test]
fn inherited_speed_is_recomputed_at_each_segment_altitude() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("b777")?;
    scenario.mission.initial_state = Some(MissionInitialState {
        altitude_m: Some(0.0),
        indicated_airspeed_m_s: None,
        true_airspeed_m_s: Some(295.0),
        mach: None,
        fuel_fraction: None,
        fuel_mass_kg: None,
    });
    scenario
        .mission
        .segments
        .retain(|segment| segment.id == "cruise_1");
    for segment in &mut scenario.mission.segments {
        segment.indicated_airspeed_m_s = None;
        segment.true_airspeed_m_s = None;
        segment.mach = None;
    }
    let error = preflight_model_domains(&scenario)
        .err()
        .ok_or("inherited cruise speed unexpectedly passed model domains")?;
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.iter().any(|item| {
        item.path == "mission.segments.cruise_1.inherited_speed"
            && item.variable == crate::domain::validity::ValidityVariable::Mach
            && item.maximum == Some(0.9)
    }));
    Ok(())
}

#[test]
fn sr71_violations_are_aggregated_and_sorted() -> Result<(), Box<dyn std::error::Error>> {
    let error = preflight_model_domains(&example_scenario("sr71")?)
        .expect_err("SR-71 declarations must exceed registered domains");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.len() > 1);
    assert!(violations.windows(2).all(|items| {
        (&items[0].path, &items[0].model_id, items[0].variable)
            <= (&items[1].path, &items[1].model_id, items[1].variable)
    }));
    assert!(violations.iter().any(|item| {
        item.path == "mission.segments.supersonic_cruise.altitude"
            && item.model_id == "atmosphere.isa1976"
            && item.maximum == Some(20_000.0)
            && item.bound_unit == "m"
    }));
    Ok(())
}

#[test]
fn x15_reports_multiple_paths_and_domain_bases() -> Result<(), Box<dyn std::error::Error>> {
    let error = preflight_model_domains(&example_scenario("x15")?)
        .expect_err("X-15 declarations must exceed registered domains");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.len() >= 5);
    assert!(violations.iter().all(|item| {
        !item.path.is_empty()
            && !item.model_id.is_empty()
            && item.declared_value.is_finite()
            && item.declared_unit == item.bound_unit
    }));
    assert!(violations.iter().any(|item| {
        item.path == "aircraft.geometry.wing.aspect_ratio"
            && item.model_id == "structures.conventional_conceptual_screen"
    }));
    assert!(violations.iter().any(|item| {
        item.path == "mission.segments.speed_run.mach" && item.model_id == "aero.parabolic_polar"
    }));
    Ok(())
}

#[test]
fn point_condition_is_preflighted_before_model_evaluation() -> Result<(), Box<dyn std::error::Error>>
{
    let scenario = example_scenario("c172")?;
    let error = preflight_operating_point(
        &scenario,
        21_000.0,
        Some(50.0),
        None,
        scenario.aircraft.mass.maximum_takeoff_mass_kg,
    )
    .expect_err("point altitude must be checked against model domains");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.iter().any(|item| {
        item.path == "condition.altitude"
            && item.model_id == "atmosphere.isa1976"
            && item.maximum == Some(20_000.0)
    }));
    Ok(())
}

#[test]
fn declared_aerodynamic_id_cannot_detach_the_runtime_polar_domain()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.aerodynamics.model = "aero.custom_typo".to_owned();
    scenario.aircraft.limits.maximum_operating_mach = Some(1.1);
    let error = preflight_model_domains(&scenario)
        .expect_err("runtime polar domain must not depend on the declared model string");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.iter().any(|item| {
        item.path == "aircraft.limits.maximum_operating_mach"
            && item.model_id == "aero.parabolic_polar"
    }));
    Ok(())
}

#[test]
fn point_preflight_uses_effective_models_and_speed_representation()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("c172")?;
    preflight_operating_point(&scenario, 0.0, Some(50.0), Some(2.0), 1_000.0)?;
    let speed_error = preflight_operating_point(&scenario, 0.0, Some(400.0), Some(0.2), 1_000.0)
        .expect_err("effective Mach derived from speed must be preflighted");
    let AexError::ModelDomainUnsupported { violations, .. } = speed_error else {
        panic!("expected model-domain error");
    };
    assert!(violations.iter().any(|item| {
        item.path == "condition.true_airspeed"
            && item.variable == crate::domain::validity::ValidityVariable::Mach
            && item.model_id == "aero.parabolic_polar"
            && item.declared_value > 0.9
    }));

    let error = preflight_operating_point(&scenario, 0.0, Some(50.0), None, -1.0)
        .expect_err("negative point mass must breach the aerodynamic domain");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].model_id, "aero.parabolic_polar");
    assert_eq!(violations[0].path, "condition.mass");
    Ok(())
}

#[test]
fn mission_preflight_mirrors_speed_precedence_and_ias_conversion()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    {
        let cruise = scenario
            .mission
            .segments
            .iter_mut()
            .find(|segment| segment.id == "cruise")
            .ok_or("missing cruise segment")?;
        cruise.true_airspeed_m_s = Some(400.0);
        cruise.mach = Some(0.2);
        cruise.indicated_airspeed_m_s = Some(30.0);
    }
    let error = preflight_model_domains(&scenario)
        .expect_err("true airspeed must take precedence and derive effective Mach");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };
    assert!(violations.iter().any(|item| {
        item.path == "mission.segments.cruise.true_airspeed"
            && item.variable == crate::domain::validity::ValidityVariable::Mach
            && item.declared_value > 0.9
    }));

    {
        let cruise = scenario
            .mission
            .segments
            .iter_mut()
            .find(|segment| segment.id == "cruise")
            .ok_or("missing cruise segment")?;
        cruise.true_airspeed_m_s = None;
        cruise.mach = None;
        cruise.indicated_airspeed_m_s = Some(400.0);
    }
    let error =
        preflight_model_domains(&scenario).expect_err("IAS must convert to true airspeed and Mach");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };
    assert!(violations.iter().any(|item| {
        item.path == "mission.segments.cruise.indicated_airspeed"
            && item.variable == crate::domain::validity::ValidityVariable::Mach
            && item.declared_value > 0.9
    }));
    Ok(())
}

#[test]
fn non_finite_declarations_are_rejected_before_domain_comparison()
-> Result<(), Box<dyn std::error::Error>> {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut scenario = example_scenario("c172")?;
        let cruise = scenario
            .mission
            .segments
            .iter_mut()
            .find(|segment| segment.id == "cruise")
            .ok_or("missing cruise segment")?;
        cruise.true_airspeed_m_s = None;
        cruise.mach = Some(value);
        let error = preflight_model_domains(&scenario)
            .expect_err("non-finite declarations must not reach model evaluation");
        let AexError::Validation { code, path, .. } = error else {
            panic!("expected validation error");
        };
        assert_eq!(code, "NON_FINITE_VALUE");
        assert_eq!(path, "mission.segments.cruise.mach");
    }
    Ok(())
}

#[test]
fn explicit_mach_at_an_inclusive_boundary_is_not_recomputed()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let cruise = scenario
        .mission
        .segments
        .iter_mut()
        .find(|segment| segment.id == "cruise")
        .ok_or("missing cruise segment")?;
    cruise.true_airspeed_m_s = None;
    cruise.mach = Some(0.9);
    cruise.indicated_airspeed_m_s = None;
    preflight_model_domains(&scenario)?;
    Ok(())
}

#[test]
fn engine_domain_role_is_independent_of_its_free_form_model_id()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let EngineProfile::Turbofan(profile) = &mut scenario.engine else {
        panic!("B777 must use a turbofan profile");
    };
    profile.model = "atmosphere.isa1976".to_owned();
    profile.maximum_mach = 0.2;

    let error = preflight_model_domains(&scenario)
        .expect_err("engine domain must retain propulsion scope when model IDs collide");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };
    assert!(violations.iter().any(|violation| {
        violation.model_id == "atmosphere.isa1976"
            && violation.path == "mission.segments.cruise_1.mach"
            && violation.maximum == Some(0.2)
    }));
    Ok(())
}

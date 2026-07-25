use crate::models::atmosphere::Isa1976;
use crate::test_support::example_scenario;

use super::{
    OperatingMode, PropulsionQuery, ThrustFuelBasis, evaluate, fuel_flow_for_required_thrust,
};

#[test]
fn piston_power_and_turbofan_thrust_lapse_with_altitude() {
    for name in ["c172", "b777"] {
        let scenario = example_scenario(name);
        assert!(scenario.is_ok());
        if let Ok(resolved) = scenario {
            let sea_level = Isa1976::new(0.0).evaluate(0.0);
            let altitude = Isa1976::new(0.0).evaluate(8000.0);
            if let (Ok(low_atmosphere), Ok(high_atmosphere)) = (sea_level, altitude) {
                let query = |altitude_m| PropulsionQuery {
                    altitude_m,
                    true_airspeed_m_s: 100.0,
                    mach: 0.3,
                    throttle: 1.0,
                    mode: OperatingMode::Cruise,
                };
                let low = evaluate(&resolved, &low_atmosphere, query(0.0));
                let high = evaluate(&resolved, &high_atmosphere, query(8000.0));
                assert!(low.is_ok() && high.is_ok());
                if let (Ok(low_value), Ok(high_value)) = (low, high) {
                    let low_power = low_value.propulsive_power_available_w.unwrap_or(0.0);
                    let high_power = high_value.propulsive_power_available_w.unwrap_or(0.0);
                    assert!(high_power < low_power);
                }
            }
        }
    }
}

#[test]
fn zero_throttle_has_exactly_zero_output_and_fuel() -> Result<(), Box<dyn std::error::Error>> {
    let atmosphere = Isa1976::new(0.0).evaluate(3_000.0)?;
    for name in ["c172", "b777"] {
        let scenario = example_scenario(name)?;
        let state = evaluate(
            &scenario,
            &atmosphere,
            PropulsionQuery {
                altitude_m: 3_000.0,
                true_airspeed_m_s: 100.0,
                mach: 0.3,
                throttle: 0.0,
                mode: OperatingMode::Economy,
            },
        )?;
        assert_eq!(state.thrust_available_n, Some(0.0));
        assert_eq!(state.propulsive_power_available_w, Some(0.0));
        assert!(
            state
                .shaft_power_available_w
                .is_none_or(|power| power == 0.0)
        );
        assert_eq!(state.fuel_flow_kg_s, 0.0);
    }
    Ok(())
}

#[test]
fn b777_table_reproduces_the_legacy_deck_across_modes_and_grid_points()
-> Result<(), Box<dyn std::error::Error>> {
    let table_scenario = example_scenario("b777")?;
    let mut simple_scenario = table_scenario.clone();
    let crate::domain::schema::EngineProfile::Turbofan(profile) = &mut simple_scenario.engine
    else {
        return Err("B777 requires a turbofan profile".into());
    };
    profile.table_deck = None;
    profile.simple_deck = Some(crate::domain::schema::SimpleTurbofanDeck {
        sea_level_static_thrust_n: 513_000.0,
        altitude_exponent: 0.72,
        mach_linear_coefficient: 0.30,
        minimum_thrust_fraction: 0.12,
        tsfc_takeoff_kg_n_hr: 0.0108,
        tsfc_cruise_kg_n_hr: 0.0372,
        cruise_reference_altitude_m: 35_000.0 * 0.3048,
        cruise_reference_mach: 0.84,
        maximum_mach: 0.90,
        maximum_altitude_m: 45_000.0 * 0.3048,
    });
    for mode in OperatingMode::ALL {
        for altitude_ft in [0.0, 35_000.0, 45_000.0] {
            for mach in [0.0, 0.84, 0.9] {
                compare_decks(
                    &table_scenario,
                    &simple_scenario,
                    mode,
                    altitude_ft * 0.3048,
                    mach,
                )?;
            }
        }
    }
    Ok(())
}

fn compare_decks(
    table_scenario: &crate::domain::schema::ResolvedScenario,
    simple_scenario: &crate::domain::schema::ResolvedScenario,
    mode: OperatingMode,
    altitude_m: f64,
    mach: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    let query = PropulsionQuery {
        altitude_m,
        true_airspeed_m_s: mach * atmosphere.speed_of_sound_m_s,
        mach,
        throttle: 1.0,
        mode,
    };
    let table = evaluate(table_scenario, &atmosphere, query)?;
    let simple = evaluate(simple_scenario, &atmosphere, query)?;
    assert_relative(table.thrust_available_n, simple.thrust_available_n, 2.0e-8)?;
    assert_relative(
        Some(table.fuel_flow_kg_s),
        Some(simple.fuel_flow_kg_s),
        3.0e-4,
    )?;
    Ok(())
}

#[test]
fn table_scaling_follows_interpolation_and_off_table_queries_warn()
-> Result<(), Box<dyn std::error::Error>> {
    let baseline = example_scenario("b777")?;
    let altitude_m = 46_000.0 * 0.3048;
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    let query = PropulsionQuery {
        altitude_m,
        true_airspeed_m_s: atmosphere.speed_of_sound_m_s,
        mach: 1.0,
        throttle: 1.0,
        mode: OperatingMode::Climb,
    };
    let full = evaluate(&baseline, &atmosphere, query)?;
    assert_eq!(full.model.validity_status, "extrapolated");
    assert_eq!(full.warnings.len(), 2);
    assert!(full.warnings.iter().all(|warning| {
        warning.code == "MODEL_EXTRAPOLATION" && warning.context["basis"] == "tabulated_data"
    }));

    let mut scaled = baseline;
    scaled.aircraft.propulsion.sizing_factor = 1.15;
    let scaled_state = evaluate(
        &scaled,
        &atmosphere,
        PropulsionQuery {
            throttle: 0.4,
            ..query
        },
    )?;
    assert_relative(
        scaled_state.thrust_available_n,
        full.thrust_available_n.map(|value| value * 1.15 * 0.4),
        1.0e-12,
    )?;
    assert_relative(
        Some(scaled_state.fuel_flow_kg_s),
        Some(full.fuel_flow_kg_s * 1.15 * 0.4),
        1.0e-12,
    )?;
    Ok(())
}

#[test]
fn specific_impulse_table_uses_the_rocket_mass_flow_relation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let crate::domain::schema::EngineProfile::Turbofan(profile) = &mut scenario.engine else {
        return Err("B777 requires a turbofan profile".into());
    };
    let deck = profile
        .table_deck
        .as_mut()
        .ok_or("B777 requires a table deck")?;
    let mode = deck
        .modes
        .iter_mut()
        .find(|item| item.mode == OperatingMode::Cruise)
        .ok_or("B777 requires a cruise table")?;
    mode.fuel = crate::domain::propulsion::TableFuelSchedule::SpecificImpulseS(vec![
        vec![300.0; deck.mach_axis.len()];
        deck.altitude_axis_m.len()
    ]);
    let atmosphere = Isa1976::new(0.0).evaluate(10_000.0)?;
    let state = evaluate(
        &scenario,
        &atmosphere,
        PropulsionQuery {
            altitude_m: 10_000.0,
            true_airspeed_m_s: 0.8 * atmosphere.speed_of_sound_m_s,
            mach: 0.8,
            throttle: 1.0,
            mode: OperatingMode::Cruise,
        },
    )?;
    let thrust = state
        .thrust_available_n
        .ok_or("turbofan result requires thrust")?;
    let expected = thrust / (300.0 * crate::domain::quantity::GRAVITY_M_S2);
    assert!((state.fuel_flow_kg_s - expected).abs() < 1.0e-12);
    Ok(())
}

#[test]
fn j58_subsonic_required_thrust_flow_is_in_calibration_band()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("sr71")?;
    let altitude_m = 25_000.0 * 0.3048;
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    let speed_m_s = 0.85 * atmosphere.speed_of_sound_m_s;
    let required_thrust = crate::models::performance::PointAnalyzer::new(scenario.clone())
        .point(
            altitude_m,
            speed_m_s,
            scenario.aircraft.mass.maximum_takeoff_mass_kg,
            "clean",
        )?
        .thrust_required_n;
    let crate::domain::schema::EngineProfile::Turbofan(profile) = &scenario.engine else {
        return Err("SR-71 requires a table thrust profile".into());
    };
    let flow = fuel_flow_for_required_thrust(
        profile,
        altitude_m,
        0.85,
        OperatingMode::Cruise,
        required_thrust,
    )?;
    let tonnes_per_hour = flow.flow_kg_s * 3.6;
    assert!(
        (3.0..=5.0).contains(&tonnes_per_hour),
        "J58 subsonic flow was {tonnes_per_hour} t/hr at {required_thrust} N"
    );
    assert_eq!(flow.basis, ThrustFuelBasis::Tsfc);
    assert!(flow.warnings.is_empty());
    let high_speed = fuel_flow_for_required_thrust(
        profile,
        78_000.0 * 0.3048,
        3.2,
        OperatingMode::Cruise,
        100_000.0,
    )?;
    assert!(high_speed.warnings.is_empty());
    Ok(())
}

#[test]
fn xlr99_full_throttle_uses_public_thrust_and_flow_anchors()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("x15")?;
    let altitude_m = 45_000.0 * 0.3048;
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    let state = evaluate(
        &scenario,
        &atmosphere,
        PropulsionQuery {
            altitude_m,
            true_airspeed_m_s: 0.5 * atmosphere.speed_of_sound_m_s,
            mach: 0.5,
            throttle: 1.0,
            mode: OperatingMode::Climb,
        },
    )?;
    let thrust = state
        .thrust_available_n
        .ok_or("rocket table requires thrust")?;
    assert!((thrust - 253_549.0).abs() < 1.0);
    assert!((95.0..=100.0).contains(&state.fuel_flow_kg_s));
    assert_eq!(state.model.validity_status, "valid");
    assert!(state.warnings.is_empty());
    let profile = match &scenario.engine {
        crate::domain::schema::EngineProfile::Turbofan(profile) => profile,
        crate::domain::schema::EngineProfile::Piston(_) => {
            return Err("X-15 requires a table thrust profile".into());
        }
    };
    let required =
        fuel_flow_for_required_thrust(profile, altitude_m, 0.5, OperatingMode::Climb, thrust)?;
    assert_eq!(required.basis, ThrustFuelBasis::SpecificImpulse);
    Ok(())
}

#[test]
fn table_deck_bilinearly_interpolates_altitude_and_mach() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("b777")?;
    let crate::domain::schema::EngineProfile::Turbofan(profile) = &mut scenario.engine else {
        return Err("B777 requires a turbofan profile".into());
    };
    profile.thrust_loss_fraction = 0.0;
    let deck = profile
        .table_deck
        .as_mut()
        .ok_or("B777 requires a table deck")?;
    deck.altitude_axis_m = vec![0.0, 10_000.0];
    deck.mach_axis = vec![0.0, 0.8];
    let cruise = deck
        .modes
        .iter_mut()
        .find(|item| item.mode == OperatingMode::Cruise)
        .ok_or("B777 requires a cruise table")?;
    cruise.thrust_n = vec![vec![100.0, 200.0], vec![300.0, 500.0]];
    cruise.fuel = crate::domain::propulsion::TableFuelSchedule::TsfcKgNHr(vec![
        vec![0.01, 0.02],
        vec![0.03, 0.05],
    ]);
    let atmosphere = Isa1976::new(0.0).evaluate(5_000.0)?;
    let state = evaluate(
        &scenario,
        &atmosphere,
        PropulsionQuery {
            altitude_m: 5_000.0,
            true_airspeed_m_s: 100.0,
            mach: 0.4,
            throttle: 1.0,
            mode: OperatingMode::Cruise,
        },
    )?;
    let expected_thrust = 275.0 * 2.0;
    assert_eq!(state.thrust_available_n, Some(expected_thrust));
    assert!((state.fuel_flow_kg_s - 0.0275 * expected_thrust / 3600.0).abs() < 1.0e-12);
    Ok(())
}

fn assert_relative(
    actual: Option<f64>,
    expected: Option<f64>,
    tolerance: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let actual = actual.ok_or("missing actual value")?;
    let expected = expected.ok_or("missing expected value")?;
    let scale = expected.abs().max(1.0);
    if (actual - expected).abs() <= tolerance * scale {
        Ok(())
    } else {
        Err(format!("expected {expected}, got {actual}").into())
    }
}

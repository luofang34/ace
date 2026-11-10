use crate::domain::aerodynamics::PolarTable;
use crate::test_support::example_scenario;

use super::{ConstraintAnalyzer, ConstraintTargets};
use crate::domain::propulsion::PowerLapse;
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::models::atmosphere::scenario_atmosphere;

#[test]
fn constraint_analysis_retains_each_configuration_extrapolation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    for configuration in [
        &mut scenario.aircraft.aerodynamics.clean,
        &mut scenario.aircraft.aerodynamics.takeoff,
        &mut scenario.aircraft.aerodynamics.landing,
    ] {
        configuration.polar_table = Some(PolarTable {
            mach: vec![0.5, 0.8],
            cd0: vec![configuration.cd0; 2],
            cl_max: vec![configuration.cl_max; 2],
            oswald_efficiency: Some(vec![configuration.oswald_efficiency; 2]),
            induced_drag_factor: None,
        });
    }
    let result = ConstraintAnalyzer::new(scenario).analyze(1_000.0, 2_000.0, 5)?;
    let paths = result
        .warnings
        .iter()
        .filter(|warning| warning.code == "MODEL_EXTRAPOLATION")
        .filter_map(|warning| warning.path.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(
        paths,
        [
            "aircraft.aerodynamics.clean.polar_table.mach",
            "aircraft.aerodynamics.takeoff.polar_table.mach",
            "aircraft.aerodynamics.landing.polar_table.mach",
        ]
    );
    Ok(())
}

const DESIGN_ALTITUDE_M: f64 = 6_000.0;
const DESIGN_SPEED_M_S: f64 = 90.0;

fn cruise_and_climb(
    scenario: &ResolvedScenario,
    speed_m_s: f64,
    altitude_m: f64,
) -> Result<[Vec<f64>; 2], Box<dyn std::error::Error>> {
    let targets = ConstraintTargets {
        design_speed_m_s: speed_m_s,
        design_altitude_m: altitude_m,
        ..ConstraintTargets::from_scenario(scenario)
    };
    let result =
        ConstraintAnalyzer::new(scenario.clone()).analyze_with(targets, 500.0, 1_500.0, 6)?;
    Ok([
        result.constraints["cruise_speed"].clone(),
        result.constraints["climb_rate"].clone(),
    ])
}

fn density_ratio(scenario: &ResolvedScenario) -> Result<f64, Box<dyn std::error::Error>> {
    let atmosphere = scenario_atmosphere(scenario);
    Ok(atmosphere.evaluate(DESIGN_ALTITUDE_M)?.density_kg_m3
        / atmosphere.evaluate(0.0)?.density_kg_m3)
}

/// Level flight at one equivalent airspeed needs the same thrust at any
/// altitude, so its power grows with true airspeed, by 1 / sqrt(sigma). The
/// sea-level rating that supplies it is that power over the engine's lapse.
fn assert_cruise_referred_to_sea_level(
    scenario: &ResolvedScenario,
    available_fraction: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let sigma = density_ratio(scenario)?;
    let [at_altitude, _] = cruise_and_climb(scenario, DESIGN_SPEED_M_S, DESIGN_ALTITUDE_M)?;
    let [same_equivalent_airspeed, _] =
        cruise_and_climb(scenario, DESIGN_SPEED_M_S * sigma.sqrt(), 0.0)?;
    for (required, sea_level) in at_altitude.iter().zip(&same_equivalent_airspeed) {
        let expected = sea_level / sigma.sqrt() / available_fraction;
        assert!(
            (required - expected).abs() <= 1e-9 * expected,
            "{required} W/kg against {expected}"
        );
    }
    Ok(())
}

#[test]
fn a_naturally_aspirated_cruise_at_altitude_is_rated_through_the_power_lapse()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("c172")?;
    let sigma = density_ratio(&scenario)?;
    let EngineProfile::Piston(profile) = &scenario.engine else {
        return Err("the C172 example has a piston engine".into());
    };
    let lapse = profile.power_lapse.fraction(sigma).fraction;
    assert!(lapse < 0.6, "{lapse}");
    assert_cruise_referred_to_sea_level(&scenario, lapse)
}

#[test]
fn a_flat_rated_engine_is_not_asked_for_its_altitude_speed_at_sea_level()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let sigma = density_ratio(&scenario)?;
    let EngineProfile::Piston(profile) = &mut scenario.engine else {
        return Err("the C172 example has a piston engine".into());
    };
    profile.power_lapse = PowerLapse::CriticalAltitude {
        critical_altitude_m: DESIGN_ALTITUDE_M,
        critical_density_ratio: sigma,
        exponent_below: 0.0,
        exponent_above: 1.0,
        minimum_power_fraction: None,
    };
    assert_cruise_referred_to_sea_level(&scenario, 1.0)?;

    // Flying the design true airspeed in sea-level air costs more parasite
    // power than the aircraft ever needs at its design altitude.
    let [at_altitude, climb_at_altitude] =
        cruise_and_climb(&scenario, DESIGN_SPEED_M_S, DESIGN_ALTITUDE_M)?;
    let [at_sea_level, _] = cruise_and_climb(&scenario, DESIGN_SPEED_M_S, 0.0)?;
    for ((altitude, sea_level), climb) in at_altitude
        .iter()
        .zip(&at_sea_level)
        .zip(&climb_at_altitude)
    {
        assert!(altitude < sea_level, "{altitude} W/kg against {sea_level}");
        assert!(climb > altitude);
    }
    Ok(())
}

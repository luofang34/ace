use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::propulsion::PowerLapse;
use crate::domain::quantity::FOOT_M;
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::{MetricValidity, ValidityStatus};
use crate::domain::warning::WarningCode;
use crate::models::atmosphere::Isa1976;
use crate::models::performance::PointAnalyzer;
use crate::test_support::example_scenario;

const CRITICAL_ALTITUDE_M: f64 = 5_500.0;

/// LC-ISR-M-class airframe (850 kg, 14 m^2, AR 11.9) with a 114 kW shaft engine,
/// built on the C172 fixture's propeller and configurations.
fn isr(lapse: PowerLapse) -> AexResult<ResolvedScenario> {
    let mut scenario = example_scenario("c172")?;
    let aircraft = &mut scenario.aircraft;
    aircraft.mass.maximum_takeoff_mass_kg = 850.0;
    aircraft.wing.area_m2 = 14.0;
    aircraft.wing.aspect_ratio = 11.9;
    aircraft.wing.span_m = (14.0_f64 * 11.9).sqrt();
    aircraft.aerodynamics.clean.cd0 = 0.025;
    aircraft.limits.maximum_operating_altitude_m = None;
    let EngineProfile::Piston(engine) = &mut scenario.engine else {
        return Err(crate::domain::diagnostic::AexError::analysis(
            "TEST_FIXTURE",
            "C172 fixture must use a shaft engine",
        ));
    };
    engine.rated_power_w = 114_000.0;
    engine.power_lapse = lapse;
    engine.maximum_altitude_m = None;
    Ok(scenario)
}

fn turbo(floor: Option<f64>) -> AexResult<PowerLapse> {
    Ok(PowerLapse::CriticalAltitude {
        critical_altitude_m: CRITICAL_ALTITUDE_M,
        critical_density_ratio: Isa1976::new(0.0)
            .evaluate(CRITICAL_ALTITUDE_M)?
            .density_kg_m3
            / 1.225,
        exponent_below: 0.0,
        exponent_above: 1.0,
        minimum_power_fraction: floor,
    })
}

fn service_ceiling(scenario: ResolvedScenario) -> AexResult<f64> {
    let analyzer = PointAnalyzer::new(scenario);
    let mass = 850.0;
    analyzer
        .ceiling_solution(mass, super::service_ceiling_threshold(&analyzer.scenario))?
        .value
        .ok_or_else(|| AexError::analysis("TEST_FIXTURE", "ceiling must be bracketed"))
}

#[test]
fn turbo_normalized_ceiling_exceeds_aspirated_and_follows_lapse_above_critical() -> AexResult<()> {
    let flat_rated = service_ceiling(isr(turbo(None)?)?)?;
    let aspirated = service_ceiling(isr(PowerLapse::DensityPower {
        exponent: 1.0,
        minimum_power_fraction: None,
    })?)?;
    assert!(
        aspirated < flat_rated,
        "NA {aspirated} m vs turbo {flat_rated} m"
    );
    assert!(flat_rated > CRITICAL_ALTITUDE_M);
    // A steeper lapse above the critical altitude can only lower the ceiling.
    let mut steep = turbo(None)?;
    if let PowerLapse::CriticalAltitude { exponent_above, .. } = &mut steep {
        *exponent_above = 1.5;
    }
    let steeper = service_ceiling(isr(steep)?)?;
    assert!(
        (CRITICAL_ALTITUDE_M..flat_rated).contains(&steeper),
        "{steeper} m"
    );
    Ok(())
}

#[test]
fn a_ceiling_held_up_by_the_power_floor_is_flagged() -> AexResult<()> {
    let floored = isr(turbo(Some(0.55))?)?;
    let analyzer = PointAnalyzer::new(floored.clone());
    let summary = analyzer.summary(None)?;
    assert!(
        summary
            .service_ceiling_m
            .is_some_and(|ceiling| ceiling > 9_000.0)
    );
    assert!(
        summary
            .warnings
            .iter()
            .any(|warning| { warning.code == WarningCode::CeilingReliesOnPowerFloor.as_str() })
    );
    let unfloored = PointAnalyzer::new(isr(turbo(None)?)?).summary(None)?;
    assert!(
        !unfloored
            .warnings
            .iter()
            .any(|warning| { warning.code == WarningCode::CeilingReliesOnPowerFloor.as_str() })
    );
    Ok(())
}

#[test]
fn declared_engine_altitude_limit_clamps_and_names_the_ceiling() -> AexResult<()> {
    let mut scenario = isr(turbo(None)?)?;
    let EngineProfile::Piston(engine) = &mut scenario.engine else {
        return Err(AexError::analysis("TEST_FIXTURE", "shaft engine expected"));
    };
    engine.maximum_altitude_m = Some(25_000.0 * FOOT_M);
    let boundary = format!("engine.{}.limits.maximum_altitude", engine.id);
    let summary = PointAnalyzer::new(scenario).summary(None)?;
    let ceiling = summary.service_ceiling_m.unwrap_or(f64::NAN);
    assert!((ceiling - 25_000.0 * FOOT_M).abs() < 1e-9);
    assert!((6_000.0..=9_000.0).contains(&ceiling));
    assert_eq!(
        summary.validity_for("performance.service_ceiling"),
        MetricValidity::boundary_limited(&boundary)
    );
    Ok(())
}

#[test]
fn an_unbracketed_ceiling_is_unavailable_not_the_search_limit() -> AexResult<()> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.propulsion.sizing_factor = 10.0;
    let EngineProfile::Piston(engine) = &mut scenario.engine else {
        return Err(AexError::analysis("TEST_FIXTURE", "shaft engine expected"));
    };
    // Without the engine's declared altitude limit only the atmosphere top remains.
    engine.maximum_altitude_m = None;
    let summary = PointAnalyzer::new(scenario).summary(None)?;
    assert_eq!(summary.service_ceiling_m, None);
    assert_eq!(summary.absolute_ceiling_m, None);
    for metric in [
        "performance.service_ceiling",
        "performance.absolute_ceiling",
    ] {
        assert_eq!(
            summary.validity_for(metric).status,
            ValidityStatus::Unsupported
        );
    }
    assert!(
        summary
            .warnings
            .iter()
            .any(|warning| warning.code == WarningCode::CeilingUnavailable.as_str())
    );
    Ok(())
}

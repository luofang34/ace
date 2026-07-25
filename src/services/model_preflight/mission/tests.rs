#![allow(clippy::expect_used, clippy::panic)]

use crate::domain::diagnostic::AexError;
use crate::domain::validity::ValidityVariable;
use crate::services::model_preflight::preflight_model_domains;
use crate::test_support::example_scenario;

#[test]
fn energy_schedule_points_are_preflighted_with_stable_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let point = scenario
        .mission
        .segments
        .iter_mut()
        .find_map(|segment| segment.energy_schedule.as_mut())
        .and_then(|schedule| schedule.last_mut())
        .ok_or("missing B777 energy schedule")?;
    point.mach = Some(0.95);
    point.indicated_airspeed_m_s = None;
    point.true_airspeed_m_s = None;
    let error = preflight_model_domains(&scenario)
        .expect_err("schedule Mach outside the polar domain must fail");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.iter().any(|violation| {
        violation.path == "mission.segments.climb_to_cruise.schedule.2.mach"
            && violation.variable == ValidityVariable::Mach
            && violation.model_id == "aero.parabolic_polar"
    }));
    Ok(())
}

#[test]
fn x15_schedule_violations_are_aggregated() -> Result<(), Box<dyn std::error::Error>> {
    let error = preflight_model_domains(&example_scenario("x15")?)
        .expect_err("X-15 schedule exceeds registered domains");
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };

    assert!(violations.iter().any(|violation| {
        violation.path == "mission.segments.boost_climb.schedule.2.mach"
            && violation.model_id == "aero.parabolic_polar"
    }));
    assert!(violations.windows(2).all(|pair| {
        (&pair[0].path, &pair[0].model_id, pair[0].variable)
            <= (&pair[1].path, &pair[1].model_id, pair[1].variable)
    }));
    Ok(())
}

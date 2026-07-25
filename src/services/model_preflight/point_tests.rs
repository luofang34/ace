#![allow(clippy::expect_used, clippy::panic)]

use crate::domain::aerodynamics::PolarTable;
use crate::domain::diagnostic::AexError;
use crate::test_support::example_scenario;

use super::preflight_operating_point;

#[test]
fn point_preflight_uses_requested_configuration_support() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("c172")?;
    set_table(&mut scenario.aircraft.aerodynamics.clean, 0.3);
    set_table(&mut scenario.aircraft.aerodynamics.landing, 0.9);
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;

    preflight_operating_point(&scenario, "landing", 0.0, None, Some(0.5), mass)?;
    assert_maximum(
        preflight_operating_point(&scenario, "clean", 0.0, None, Some(0.5), mass)
            .expect_err("clean support must reject Mach 0.5"),
        0.3,
    );

    set_table(&mut scenario.aircraft.aerodynamics.clean, 0.9);
    set_table(&mut scenario.aircraft.aerodynamics.landing, 0.3);
    preflight_operating_point(&scenario, "clean", 0.0, None, Some(0.5), mass)?;
    assert_maximum(
        preflight_operating_point(&scenario, "landing", 0.0, None, Some(0.5), mass)
            .expect_err("landing support must reject Mach 0.5"),
        0.3,
    );
    Ok(())
}

fn set_table(configuration: &mut crate::domain::schema::AeroConfiguration, maximum: f64) {
    configuration.polar_table = Some(PolarTable {
        mach: vec![0.0, maximum],
        cd0: vec![configuration.cd0; 2],
        cl_max: vec![configuration.cl_max; 2],
        oswald_efficiency: Some(vec![configuration.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });
}

fn assert_maximum(error: AexError, maximum: f64) {
    let AexError::ModelDomainUnsupported { violations, .. } = error else {
        panic!("expected model-domain error");
    };
    assert!(violations.iter().any(|violation| {
        violation.path == "condition.mach"
            && violation.model_id == "aero.polar_table"
            && violation.maximum == Some(maximum)
    }));
}

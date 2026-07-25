use crate::domain::aerodynamics::PolarTable;
use crate::domain::schema::EngineProfile;
use crate::test_support::example_scenario;

use super::estimate_takeoff_distance;

#[test]
fn piston_field_screen_uses_engine_count_and_sizing_factor()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let baseline = estimate_takeoff_distance(&scenario)?.distance_m;

    scenario.aircraft.propulsion.sizing_factor = 1.5;
    let resized = estimate_takeoff_distance(&scenario)?.distance_m;
    assert!(resized < baseline);

    scenario.aircraft.propulsion.sizing_factor = 1.0;
    scenario.aircraft.propulsion.engine_count = 2;
    let twin_engine = estimate_takeoff_distance(&scenario)?.distance_m;
    assert!(twin_engine < baseline);
    Ok(())
}

#[test]
fn turbofan_field_screen_uses_sizing_and_installation_loss()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    let baseline = estimate_takeoff_distance(&scenario)?.distance_m;

    scenario.aircraft.propulsion.sizing_factor = 1.5;
    let resized = estimate_takeoff_distance(&scenario)?.distance_m;
    assert!(resized < baseline);

    let EngineProfile::Turbofan(profile) = &mut scenario.engine else {
        return Err("B777 requires a turbofan profile".into());
    };
    profile.thrust_loss_fraction = 0.20;
    let installation_loss = estimate_takeoff_distance(&scenario)?.distance_m;
    assert!(installation_loss > resized);
    Ok(())
}

#[test]
fn field_screen_uses_polar_table_at_zero_mach() -> Result<(), Box<dyn std::error::Error>> {
    let baseline = example_scenario("c172")?;
    let baseline_distance = estimate_takeoff_distance(&baseline)?.distance_m;
    let mut tabulated = baseline;
    let takeoff = &mut tabulated.aircraft.aerodynamics.takeoff;
    takeoff.polar_table = Some(PolarTable {
        mach: vec![0.0, 0.5],
        cd0: vec![takeoff.cd0; 2],
        cl_max: vec![takeoff.cl_max * 1.25; 2],
        oswald_efficiency: Some(vec![takeoff.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });

    let estimate = estimate_takeoff_distance(&tabulated)?;
    assert!(estimate.distance_m < baseline_distance);
    assert!(estimate.warnings.is_empty());
    Ok(())
}

#[test]
fn field_screen_retains_zero_mach_table_extrapolation() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let takeoff = &mut scenario.aircraft.aerodynamics.takeoff;
    takeoff.polar_table = Some(PolarTable {
        mach: vec![0.2, 0.5],
        cd0: vec![takeoff.cd0; 2],
        cl_max: vec![takeoff.cl_max; 2],
        oswald_efficiency: Some(vec![takeoff.oswald_efficiency; 2]),
        induced_drag_factor: None,
    });

    let estimate = estimate_takeoff_distance(&scenario)?;
    assert_eq!(estimate.warnings.len(), 1);
    assert_eq!(estimate.warnings[0].code, "MODEL_EXTRAPOLATION");
    assert_eq!(
        estimate.warnings[0].path.as_deref(),
        Some("aircraft.aerodynamics.takeoff.polar_table.mach")
    );
    assert_eq!(
        estimate.validity.status,
        crate::domain::validity::ValidityStatus::Extrapolated
    );
    Ok(())
}

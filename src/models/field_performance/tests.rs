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

#[test]
fn hot_high_field_and_landing_mass_drive_field_lengths_with_validity()
-> Result<(), Box<dyn std::error::Error>> {
    use super::estimate_landing_distance;
    use crate::domain::validity::ValidityStatus;

    let standard = example_scenario("c172")?;
    let sea_level = estimate_takeoff_distance(&standard)?;
    let mut hot_high = standard.clone();
    hot_high.mission.field.elevation_m = 3_000.0;
    hot_high.mission.isa_offset_k = 20.0;
    let elevated = estimate_takeoff_distance(&hot_high)?;
    assert!(elevated.distance_m > sea_level.distance_m);
    assert_eq!(elevated.validity.status, ValidityStatus::Valid);
    // Density alone lengthens the roll by at least 1/sigma; lapsed power adds more.
    let sigma = crate::models::atmosphere::Isa1976::new(20.0)
        .evaluate(3_000.0)?
        .density_kg_m3
        / 1.225;
    assert!(elevated.distance_m > sea_level.distance_m / sigma);

    let mut light = standard.clone();
    light.mission.field.landing_mass_kg = Some(800.0);
    let mut heavy = standard.clone();
    heavy.mission.field.landing_mass_kg = Some(1_000.0);
    let (light, heavy) = (
        estimate_landing_distance(&light)?,
        estimate_landing_distance(&heavy)?,
    );
    assert!(light.distance_m < heavy.distance_m);
    assert_eq!(light.validity.status, ValidityStatus::Valid);

    let mut outside = standard;
    outside.mission.field.elevation_m = 5_000.0;
    assert_eq!(
        estimate_takeoff_distance(&outside)?.validity.status,
        ValidityStatus::Extrapolated
    );
    outside.mission.field.elevation_m = 0.0;
    outside.mission.field.landing_mass_kg = Some(2_000.0);
    assert_eq!(
        estimate_landing_distance(&outside)?.validity.status,
        ValidityStatus::Extrapolated
    );
    Ok(())
}

#[test]
fn runway_slope_and_wind_shift_field_lengths_and_floors_are_reported()
-> Result<(), Box<dyn std::error::Error>> {
    use super::estimate_landing_distance;
    use crate::domain::validity::ValidityStatus;

    let level = example_scenario("c172")?;
    let with = |slope: f64, headwind: f64| {
        let mut scenario = level.clone();
        scenario.mission.field.runway_slope = slope;
        scenario.mission.field.headwind_m_s = headwind;
        scenario
    };
    let takeoff = |scenario| estimate_takeoff_distance(&scenario).map(|e| e.distance_m);
    let landing = |scenario| estimate_landing_distance(&scenario).map(|e| e.distance_m);
    let (base_takeoff, base_landing) = (takeoff(level.clone())?, landing(level.clone())?);
    // Uphill lengthens the takeoff roll and shortens the landing roll.
    assert!(takeoff(with(0.02, 0.0))? > base_takeoff);
    assert!(landing(with(0.02, 0.0))? < base_landing);
    // A headwind shortens both; a tailwind lengthens both.
    assert!(takeoff(with(0.0, 5.0))? < base_takeoff);
    assert!(landing(with(0.0, 5.0))? < base_landing);
    assert!(takeoff(with(0.0, -3.0))? > base_takeoff);
    assert!(landing(with(0.0, -3.0))? > base_landing);
    // Strong tailwinds leave the screening domain.
    assert_eq!(
        estimate_takeoff_distance(&with(0.0, -8.0))?.validity.status,
        ValidityStatus::Extrapolated
    );
    // A grade steeper than the available acceleration is floored and reported.
    let steep = estimate_takeoff_distance(&with(0.5, 0.0))?;
    assert_eq!(steep.validity.status, ValidityStatus::Extrapolated);
    assert!(
        steep
            .warnings
            .iter()
            .any(|warning| warning.message.contains("screening floor"))
    );
    assert!(estimate_landing_distance(&with(-0.5, 0.0)).is_err());
    Ok(())
}

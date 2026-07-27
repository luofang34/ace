use crate::models::mission::MissionSimulator;
use crate::test_support::{example_scenario, set_inferred_configuration};

use super::evaluate;

#[test]
fn c172_statement_closes_and_start_cg_matches_hand_calculation()
-> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("c172")?;
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let analysis = evaluate(&scenario, &mission)?;
    let statement = &scenario.aircraft.mass.statement;
    let component_mass = statement
        .components
        .iter()
        .map(|component| component.mass.value)
        .sum::<f64>();
    let payload = scenario.mission.payload_mass_kg;
    let fuel =
        mission.initial_takeoff_mass_kg - scenario.aircraft.mass.operating_empty_mass_kg - payload;
    let expected_cg =
        (2_586.0 + payload * statement.payload_station.value + fuel * statement.fuel_station.value)
            / mission.initial_takeoff_mass_kg;

    assert!((component_mass - 767.0).abs() < 1.0e-9);
    let empty_moment = statement
        .components
        .iter()
        .map(|component| component.mass.value * component.station.value)
        .sum::<f64>();
    assert!((empty_moment - 2_586.0).abs() < 1.0e-9);
    assert!(statement.closure_error_kg.abs() < 1.0e-9);
    assert!((analysis.states[0].center_of_gravity.value - expected_cg).abs() < 1.0e-12);
    assert!(
        analysis
            .neutral_point
            .as_ref()
            .is_some_and(|point| (3.66..=3.68).contains(&point.value))
    );
    assert!((3.34..=3.36).contains(&analysis.minimum_center_of_gravity.value));
    assert!((3.34..=3.40).contains(&analysis.maximum_center_of_gravity.value));
    assert!(
        analysis
            .minimum_static_margin
            .is_some_and(|margin| (0.18..=0.23).contains(&margin))
    );
    assert!(
        analysis
            .maximum_static_margin
            .is_some_and(|margin| (0.20..=0.23).contains(&margin))
    );
    Ok(())
}

#[test]
fn tail_heavy_c172_fails_native_static_margin_screen() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    let fuselage = scenario
        .aircraft
        .mass
        .statement
        .components
        .iter_mut()
        .find(|component| component.component_id == "fuselage")
        .ok_or("missing fuselage mass component")?;
    fuselage.station.value = 5.5;
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let analysis = evaluate(&scenario, &mission)?;

    assert!(
        analysis
            .minimum_static_margin
            .is_some_and(|margin| margin < 0.0)
    );
    assert_eq!(
        analysis.failed_constraints,
        ["stability.static_margin".to_owned()]
    );
    Ok(())
}

#[test]
fn tailless_stability_is_explicitly_unsupported() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    set_inferred_configuration(&mut scenario, "tailless_flying_wing")?;
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let analysis = evaluate(&scenario, &mission)?;

    assert!(!analysis.stability_supported);
    assert!(analysis.neutral_point.is_none());
    assert!(analysis.minimum_static_margin.is_none());
    assert!(analysis.failed_constraints.is_empty());
    Ok(())
}

use crate::test_support::example_scenario;

use super::BlendedWingPlanform;

#[test]
fn opposed_center_edges_close_area_with_two_independent_angles()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.wing.center_body_edge_sweep_rad = Some(45_f64.to_radians());
    let planform = BlendedWingPlanform::from_wing(&scenario.aircraft.wing)?;
    let center_area = planform.inner_span_m * (planform.root_chord_m + planform.middle_chord_m);
    let outer_area = planform.outer_span_m * (planform.middle_chord_m + planform.tip_chord_m);
    assert!((center_area + outer_area - scenario.aircraft.wing.area_m2).abs() < 1.0e-9);
    assert!(planform.edge_alignment_error_rad() < 1.0e-12);
    assert_eq!(planform.independent_planform_angle_count(), 2);
    Ok(())
}

#[test]
fn impossible_center_edge_sweep_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.wing.center_body_edge_sweep_rad = Some(89_f64.to_radians());
    let result = BlendedWingPlanform::from_wing(&scenario.aircraft.wing);
    assert!(result.is_err());
    Ok(())
}

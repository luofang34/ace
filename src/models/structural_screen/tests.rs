use crate::test_support::{example_scenario, set_inferred_configuration};

use super::evaluate;

#[test]
fn blended_wing_uses_volume_and_outer_panel_screen() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolved = example_scenario("b777")?;
    set_inferred_configuration(&mut resolved, "blended_wing_body")?;
    resolved.aircraft.wing.center_body_edge_sweep_rad = Some(42_f64.to_radians());
    let screen = evaluate(&resolved)?;
    assert_eq!(screen.structural_configuration, "blended_wing_outer_panel");
    assert_eq!(screen.horizontal_tail_volume, 0.0);
    assert_eq!(screen.vertical_tail_volume, 0.0);
    assert!(screen.estimated_usable_internal_volume.is_some());
    assert!(screen.required_fuel_volume.is_some());
    assert!(screen.fuel_volume_utilization_ratio.is_some());
    assert!(
        screen
            .provenance
            .validity_range
            .iter()
            .any(|item| item.contains("blended-wing"))
    );
    Ok(())
}

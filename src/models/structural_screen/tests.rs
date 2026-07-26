use crate::test_support::{example_scenario, set_inferred_configuration};

use super::evaluate;

#[test]
fn conventional_tail_volumes_use_resolved_area_and_arm() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolved = example_scenario("c172")?;
    let horizontal = resolved
        .aircraft
        .geometry
        .horizontal_tail
        .as_mut()
        .ok_or_else(|| std::io::Error::other("missing horizontal tail"))?;
    horizontal.area.value = 4.0;
    horizontal.arm.value = 5.0;
    let vertical = resolved
        .aircraft
        .geometry
        .vertical_tail
        .as_mut()
        .ok_or_else(|| std::io::Error::other("missing vertical tail"))?;
    vertical.area.value = 2.0;
    vertical.arm.value = 4.0;
    let screen = evaluate(&resolved)?;
    let wing = &resolved.aircraft.wing;
    let mean_chord = wing.area_m2 / wing.span_m;

    assert!(
        (screen.horizontal_tail_volume - 4.0 * 5.0 / (wing.area_m2 * mean_chord)).abs() < 1.0e-12
    );
    assert!(
        (screen.vertical_tail_volume - 2.0 * 4.0 / (wing.area_m2 * wing.span_m)).abs() < 1.0e-12
    );
    Ok(())
}

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

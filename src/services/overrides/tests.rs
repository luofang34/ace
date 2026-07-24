use serde_yaml::Value;

use super::set_path;

#[test]
fn override_preserves_numeric_and_quantity_types() -> Result<(), Box<dyn std::error::Error>> {
    let mut document: Value = serde_yaml::from_str(
        "aircraft:\n  geometry:\n    wing:\n      aspect_ratio: 7.5\n      area: 16 m^2\n",
    )?;
    set_path(
        &mut document,
        &["aircraft", "geometry", "wing", "aspect_ratio"],
        "9.2",
        "aircraft.geometry.wing.aspect_ratio",
    )?;
    set_path(
        &mut document,
        &["aircraft", "geometry", "wing", "area"],
        "14 m^2",
        "aircraft.geometry.wing.area",
    )?;
    assert_eq!(
        document["aircraft"]["geometry"]["wing"]["aspect_ratio"].as_f64(),
        Some(9.2)
    );
    assert_eq!(
        document["aircraft"]["geometry"]["wing"]["area"].as_str(),
        Some("14 m^2")
    );
    Ok(())
}

#[test]
fn override_can_set_an_optional_scalar() -> Result<(), Box<dyn std::error::Error>> {
    let mut document: Value = serde_yaml::from_str(
        "aircraft:\n  geometry:\n    wing:\n      center_body_edge_sweep: null\n",
    )?;
    set_path(
        &mut document,
        &["aircraft", "geometry", "wing", "center_body_edge_sweep"],
        "65 deg",
        "aircraft.geometry.wing.center_body_edge_sweep",
    )?;
    assert_eq!(
        document["aircraft"]["geometry"]["wing"]["center_body_edge_sweep"].as_str(),
        Some("65 deg")
    );
    Ok(())
}

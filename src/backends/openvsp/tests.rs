use super::parsing::{polar_points, stability_summary};
use crate::backends::contracts::{GeometryBackend, GeometryRequest};
use crate::backends::native::NativeBackend;
use crate::test_support::example_scenario;

#[test]
fn parses_structured_polar_markers() -> Result<(), Box<dyn std::error::Error>> {
    let output = "
        ACE_POLAR= -2.0, -0.2, 0.03, 0.04
        solver chatter
        ACE_POLAR= 2.0, 0.3, 0.04, -0.02
    ";
    let points = polar_points(output)?;
    assert_eq!(points.len(), 2);
    let stability = stability_summary(&points);
    assert_eq!(stability.statically_stable, Some(true));
    Ok(())
}

#[test]
fn c172_script_places_and_sizes_the_concept() -> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("c172")?;
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("c172.vsp3"))?;
    assert!(script.contains("X_Rel_Location\", \"XForm\", 2.805000000000"));
    assert!(script.contains("Sym_Planar_Flag\", \"Sym\", 0"));
    assert!(script.contains("SetEllipse"));
    Ok(())
}

#[test]
fn blended_wing_script_is_tailless_and_reflexed() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.configuration = "tailless_blended_wing_body".to_owned();
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("bwb.vsp3"))?;
    assert!(!script.contains("ACE_Fuselage"));
    assert!(!script.contains("ACE_Horizontal_Tail"));
    assert!(script.contains("InsertXSec"));
    assert!(script.contains("XS_FIVE_DIGIT_MOD"));
    assert!(script.contains("TE_Flap_Deflection"));
    assert!(script.contains("ACE_Engine_Envelope"));
    Ok(())
}

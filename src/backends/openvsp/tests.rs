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

#[test]
fn blended_wing_center_has_parallel_opposite_edges() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.configuration = "tailless_blended_wing_body".to_owned();
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("bwb.vsp3"))?;
    let semispan = scenario.aircraft.wing.span_m * 0.5;
    let inner_span = semispan * 0.23;
    let outer_span = semispan - inner_span;
    let area_denominator = inner_span * 1.55 + outer_span * 0.55 * 1.16;
    let root_chord = scenario.aircraft.wing.area_m2 / area_denominator;
    let middle_chord = root_chord * 0.55;
    let sweep = (0.25 * (root_chord - middle_chord) / inner_span)
        .atan()
        .to_degrees();
    assert!(script.contains(&format!(
        "SetParmVal( wing, \"Sweep\", \"XSec_1\", {sweep:.12} )"
    )));
    Ok(())
}

#[test]
fn vspaero_excludes_the_non_lifting_engine_envelope() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.configuration = "tailless_blended_wing_body".to_owned();
    let script = super::analysis_script(&scenario, std::path::Path::new("bwb.vsp3"))?;
    assert!(script.contains("FindGeomsWithName( \"ACE_Engine_Envelope\" )"));
    assert!(script.contains("DeleteGeomVec( engine_envelopes )"));
    Ok(())
}

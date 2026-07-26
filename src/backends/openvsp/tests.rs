use super::parsing::{polar_points, positive_marker_number, stability_summary};
use crate::backends::contracts::{GeometryBackend, GeometryRequest};
use crate::backends::native::NativeBackend;
use crate::domain::schema::EngineProfile;
use crate::test_support::{example_scenario, set_inferred_configuration};

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
fn rejects_nonpositive_or_nonfinite_compgeom_area() {
    for value in ["0", "-1", "NaN", "inf"] {
        let output = format!("ACE_WETTED_AREA_M2={value}");
        assert!(positive_marker_number(&output, "ACE_WETTED_AREA_M2=").is_err());
    }
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
    assert!(script.contains("SetGeomName( propeller, \"ACE_Propeller\" )"));
    assert!(script.contains("Diameter\", \"Design\", 1.930000000000"));
    assert!(script.contains("AddEngineEnvelope( 0.0 )"));
    assert!(script.contains("engine, \"X_Rel_Location\", \"XForm\", 0.000000000000"));
    assert!(script.contains("propeller, \"X_Rel_Location\", \"XForm\", -0.030000000000"));
    let engine_length = match &scenario.engine {
        EngineProfile::Piston(profile) => 0.30 * profile.dry_mass_kg.cbrt(),
        EngineProfile::Turbofan(_) => 0.0,
    };
    assert!(script.contains(&format!("\"Length\", \"Design\", {engine_length:.12}")));
    assert!(script.contains("SetSetFlag( propeller, SET_FIRST_USER, false )"));
    assert!(script.contains("ComputeCompGeom( SET_ALL, false, 0 )"));
    assert_conventional_vspaero_membership(&script);
    Ok(())
}

#[test]
fn conventional_script_honors_independent_tail_arms() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    scenario
        .aircraft
        .geometry
        .horizontal_tail
        .as_mut()
        .ok_or_else(|| std::io::Error::other("missing horizontal tail"))?
        .arm
        .value = 12.34;
    scenario
        .aircraft
        .geometry
        .vertical_tail
        .as_mut()
        .ok_or_else(|| std::io::Error::other("missing vertical tail"))?
        .arm
        .value = 23.45;
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("c172.vsp3"))?;

    assert!(script.contains("horizontal_tail, \"X_Rel_Location\", \"XForm\", 15.145000000000"));
    assert!(script.contains("vertical_tail, \"X_Rel_Location\", \"XForm\", 26.255000000000"));
    Ok(())
}

#[test]
fn conventional_script_skips_absent_resolved_components() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.geometry.horizontal_tail = None;
    scenario
        .aircraft
        .topology
        .components
        .retain(|component| component.kind != "horizontal_tail");
    scenario
        .aircraft
        .topology
        .relationships
        .retain(|relationship| {
            relationship.source != "horizontal_tail" && relationship.target != "horizontal_tail"
        });
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("c172.vsp3"))?;

    assert!(script.contains(
        "if ( 0 == 1 )\n    {\n        string horizontal_tail = AddGeom( \"WING\", \"\" );"
    ));
    assert!(script.contains(
        "if ( 1 == 1 )\n    {\n        string vertical_tail = AddGeom( \"WING\", \"\" );"
    ));
    Ok(())
}

#[test]
fn transport_script_places_two_engine_envelopes() -> Result<(), Box<dyn std::error::Error>> {
    let scenario = example_scenario("b777")?;
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("b777.vsp3"))?;
    let engine_y = scenario.aircraft.wing.span_m * 0.22;
    assert!(script.contains("if ( 0 == 1 )"));
    assert!(script.contains("if ( 2 == 1 )"));
    assert!(script.contains(&format!("AddEngineEnvelope( -{engine_y:.12} )")));
    assert!(script.contains(&format!("AddEngineEnvelope( {engine_y:.12} )")));
    let (engine_length, engine_diameter) = match &scenario.engine {
        EngineProfile::Turbofan(profile) => (
            0.34 * profile.dry_mass_kg.cbrt(),
            0.17 * profile.dry_mass_kg.cbrt(),
        ),
        EngineProfile::Piston(_) => (0.0, 1.0),
    };
    assert!(script.contains(&format!("\"Length\", \"Design\", {engine_length:.12}")));
    assert!(script.contains(&format!(
        "\"FineRatio\", \"Design\", {:.12}",
        engine_length / engine_diameter
    )));
    assert_conventional_vspaero_membership(&script);
    Ok(())
}

#[test]
fn profile_dimensions_override_turbofan_mass_scaling() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("b777")?;
    if let EngineProfile::Turbofan(profile) = &mut scenario.engine {
        profile.overall_length_m = Some(8.25);
        profile.maximum_diameter_m = Some(3.75);
    }
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("b777.vsp3"))?;
    assert!(script.contains("\"Length\", \"Design\", 8.250000000000"));
    assert!(script.contains("\"FineRatio\", \"Design\", 2.200000000000"));
    Ok(())
}

#[test]
fn blended_wing_script_is_tailless_and_reflexed() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    set_inferred_configuration(&mut scenario, "tailless_blended_wing_body")?;
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
    assert!(script.contains("SetSetName( SET_FIRST_USER, \"ACE_VSPAERO_LIFTING\" )"));
    assert!(script.contains("SetSetFlag( wing, SET_FIRST_USER, true )"));
    assert!(script.contains("SetSetFlag( engine, SET_FIRST_USER, false )"));
    assert!(!script.contains("SetSetFlag( engine, SET_FIRST_USER, true )"));
    Ok(())
}

#[test]
fn twin_engine_blended_wing_uses_two_symmetric_envelopes() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.propulsion.engine_count = 2;
    set_inferred_configuration(&mut scenario, "tailless_blended_wing_body")?;
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("bwb.vsp3"))?;
    let y_location = scenario.aircraft.wing.span_m * 0.12;
    assert!(script.contains("if ( 2 == 1 )"));
    assert!(script.contains("AddEngineEnvelope( -"));
    assert!(script.contains(&format!("AddEngineEnvelope( {y_location:.12} )")));
    Ok(())
}

#[test]
fn blended_wing_center_has_parallel_opposite_edges() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    set_inferred_configuration(&mut scenario, "tailless_blended_wing_body")?;
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
fn blended_wing_center_accepts_explicit_opposite_edge_sweeps()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    set_inferred_configuration(&mut scenario, "tailless_blended_wing_body")?;
    scenario.aircraft.wing.center_body_edge_sweep_rad = Some(65_f64.to_radians());
    let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let script =
        super::geometry::geometry_script(&scenario, &native, std::path::Path::new("bwb.vsp3"))?;
    let quarter_sweep = (0.5 * 65_f64.to_radians().tan()).atan().to_degrees();
    assert!(script.contains(&format!(
        "SetParmVal( wing, \"Sweep\", \"XSec_1\", {quarter_sweep:.12} )"
    )));
    let center_of_gravity_x = super::geometry::blended_wing_center_of_gravity_x(&scenario)?;
    let analysis = super::analysis_script(&scenario, std::path::Path::new("bwb.vsp3"))?;
    assert!(analysis.contains(&format!(
        "center_of_gravity_x.push_back( {center_of_gravity_x:.12} )"
    )));
    Ok(())
}

#[test]
fn vspaero_uses_only_the_named_lifting_set() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    set_inferred_configuration(&mut scenario, "tailless_blended_wing_body")?;
    let script = super::analysis_script(&scenario, std::path::Path::new("bwb.vsp3"))?;
    assert!(script.contains("GetSetIndex( \"ACE_VSPAERO_LIFTING\" )"));
    assert!(script.contains("\"GeomSet\",\n        thick_geometry_set"));
    assert!(script.contains("\"ThinGeomSet\",\n        thin_geometry_set"));
    assert!(!script.contains("DeleteGeomVec"));
    Ok(())
}

#[test]
fn conventional_visual_snapshots_are_stable() -> Result<(), Box<dyn std::error::Error>> {
    let c172 = super::geometry::visual_snapshot(&example_scenario("c172")?);
    let b777 = super::geometry::visual_snapshot(&example_scenario("b777")?);
    assert_eq!(c172, include_str!("../../../tests/golden/openvsp/c172.svg"));
    assert_eq!(b777, include_str!("../../../tests/golden/openvsp/b777.svg"));
    Ok(())
}

#[test]
fn conventional_visual_snapshot_omits_absent_components() -> Result<(), Box<dyn std::error::Error>>
{
    let mut scenario = example_scenario("c172")?;
    scenario.aircraft.geometry.fuselage = None;
    scenario.aircraft.geometry.horizontal_tail = None;
    scenario.aircraft.geometry.vertical_tail = None;
    let omitted = ["fuselage", "horizontal_tail", "vertical_tail"];
    scenario
        .aircraft
        .topology
        .components
        .retain(|component| !omitted.contains(&component.kind.as_str()));
    scenario
        .aircraft
        .topology
        .relationships
        .retain(|relationship| {
            !omitted.contains(&relationship.source.as_str())
                && !omitted.contains(&relationship.target.as_str())
        });
    let snapshot = super::geometry::visual_snapshot(&scenario);

    assert!(snapshot.contains("id=\"wing\""));
    assert!(!snapshot.contains("id=\"fuselage\""));
    assert!(!snapshot.contains("id=\"horizontal-tail\""));
    assert!(!snapshot.contains("id=\"vertical-tail\""));
    assert!(!snapshot.contains("NaN"));
    assert!(!snapshot.contains("inf"));
    Ok(())
}

fn assert_conventional_vspaero_membership(script: &str) {
    for expected in [
        "SetSetFlag( fuselage, SET_FIRST_USER, false )",
        "SetSetFlag( wing, SET_FIRST_USER, true )",
        "SetSetFlag( horizontal_tail, SET_FIRST_USER, true )",
        "SetSetFlag( vertical_tail, SET_FIRST_USER, true )",
        "SetSetFlag( engine, SET_FIRST_USER, false )",
        "SetSetFlag( propeller, SET_FIRST_USER, false )",
    ] {
        assert!(script.contains(expected), "missing {expected}");
    }
    for forbidden in [
        "SetSetFlag( fuselage, SET_FIRST_USER, true )",
        "SetSetFlag( engine, SET_FIRST_USER, true )",
        "SetSetFlag( propeller, SET_FIRST_USER, true )",
        "SetSetFlag( wing, SET_FIRST_USER, false )",
        "SetSetFlag( horizontal_tail, SET_FIRST_USER, false )",
        "SetSetFlag( vertical_tail, SET_FIRST_USER, false )",
    ] {
        assert!(!script.contains(forbidden), "found {forbidden}");
    }
}

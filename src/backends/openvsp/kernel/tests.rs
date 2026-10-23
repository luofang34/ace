use super::*;

fn objects() -> Vec<KernelObject> {
    vec![
        KernelObject {
            id: "fuselage".into(),
            shape: KernelShape::Body {
                length_m: 40.0,
                width_m: 4.0,
                height_m: 4.0,
                nose_m: 6.0,
                tail_m: 10.0,
            },
            location_m: [0.0, 0.0, 0.0],
            rotation_deg: [0.0, 0.0, 0.0],
        },
        KernelObject {
            id: "fin".into(),
            shape: KernelShape::Wing {
                span_m: 6.0,
                area_m2: 24.0,
                taper: 0.5,
                sweep_deg: 35.0,
                dihedral_deg: 0.0,
                thickness: 0.1,
                symmetric: false,
            },
            location_m: [32.0, 0.0, 1.5],
            rotation_deg: [90.0, 0.0, 0.0],
        },
    ]
}

#[test]
fn build_script_places_every_shape_and_reads_the_kernel_back() {
    let script = script::build(&objects()).unwrap();
    assert!(script.contains("SetGeomName(g0, \"ACE_fuselage\");"));
    assert!(script.contains("SetParmVal(g0, \"Length\", \"Design\", 40.000000000000000);"));
    assert!(
        script.contains(
            "SetParmVal(GetXSecParm(GetXSec(g0s, 1), \"XLocPercent\"), 0.150000000000000);"
        )
    );
    assert!(
        script.contains(
            "SetParmVal(GetXSecParm(GetXSec(g0s, 3), \"XLocPercent\"), 0.750000000000000);"
        )
    );
    assert!(script.contains("SetParmVal(g1, \"Sym_Planar_Flag\", \"Sym\", 0);"));
    assert!(script.contains("SetParmVal(GetParm(g1, \"Span\", \"XSec_1\"), 6.000000000000000);"));
    assert!(
        script.contains("SetParmVal(GetParm(g1, \"Root_Chord\", \"XSec_1\"), 5.333333333333333);")
    );
    assert!(script.contains("SetParmVal(g1, \"X_Rel_Rotation\", \"XForm\", 90.000000000000000);"));
    assert!(script.contains("Print(\"ACE_GEOMETRY_fin.TotalProjectedSpan=\", false);"));
    assert!(script.contains("WriteVSPFile(\"model.vsp3\", SET_ALL);"));
    assert!(
        script.ends_with("Print(\"ACE_BUILD=1\");\nPrint(\"ACE_COMPLETE=1\");\nreturn 0;\n}\n")
    );
    let expected = script::expected_readback(&objects());
    assert_eq!(expected["fin.TotalProjectedSpan"], 6.0);
    assert_eq!(expected["fin.TotalArea"], 24.0);
    assert_eq!(expected["fin.X_Rel_Rotation"], 90.0);
    assert_eq!(expected["fuselage.XLocPercent_2"], 0.45);
}

#[test]
fn symmetric_wings_halve_the_panel_span_and_keep_projected_totals() {
    let mut wing = objects().remove(1);
    if let KernelShape::Wing { symmetric, .. } = &mut wing.shape {
        *symmetric = true;
    }
    let script = script::build(std::slice::from_ref(&wing)).unwrap();
    assert!(script.contains("SetParmVal(g0, \"Sym_Planar_Flag\", \"Sym\", SYM_XZ);"));
    assert!(script.contains("SetParmVal(GetParm(g0, \"Span\", \"XSec_1\"), 3.000000000000000);"));
    let expected = script::expected_readback(std::slice::from_ref(&wing));
    assert_eq!(expected["fin.TotalProjectedSpan"], 6.0);
    assert_eq!(expected["fin.TotalArea"], 24.0);
    if let KernelShape::Wing { dihedral_deg, .. } = &mut wing.shape {
        *dihedral_deg = 60.0;
    }
    let inclined = script::expected_readback(std::slice::from_ref(&wing));
    assert_eq!(inclined["fin.TotalProjectedSpan"], 6.0);
    assert!((inclined["fin.TotalArea"] - 48.0).abs() < 1e-9);
}

#[test]
fn unsafe_identifiers_never_reach_the_script() {
    let mut bad = objects();
    bad[0].id = "fuse\"); Print(\"x".into();
    assert!(script::build(&bad).is_err());
}

#[test]
fn compgeom_components_keep_only_canonical_objects() {
    let output = "ACE_COMPONENT=ACE_wing,11.5226,0.690449\nACE_COMPONENT=MeshGeom,40.4841,16.9993\nACE_COMPONENT=ACE_fuselage,29.8372,18.4916\n";
    let parsed = components(output).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].id, "wing");
    assert_eq!(parsed[1].volume_m3, 18.4916);
    assert!(components("ACE_COMPONENT=ACE_wing,nan,1\n").is_err());
    assert!(components("ACE_COMPONENT=ACE_wing,1\n").is_err());
}

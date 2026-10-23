//! AngelScript generation for the build pass; the export pass is a fixed script.

use crate::domain::diagnostic::{AexError, AexResult};
use std::collections::BTreeMap;
use std::fmt::Write;

pub(crate) const MODEL_FILE: &str = "model.vsp3";
pub(crate) const CANONICAL_FILE: &str = "canonical.vsp3";
pub(crate) const STEP_FILE: &str = "model.step";
pub(crate) const OBJ_FILE: &str = "model.obj";
pub(crate) const NAME_PREFIX: &str = "ACE_";
pub(crate) const EXPORT: &str = include_str!("../scripts/kernel_export.vspscript");

/// Native conceptual shape resolved to kernel dimensions.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum KernelShape {
    /// Lofted elliptical body with its origin at the nose, along local +x.
    Body {
        /// Total length.
        length_m: f64,
        /// Maximum full width.
        width_m: f64,
        /// Maximum full height.
        height_m: f64,
        /// Forebody length to the constant section.
        nose_m: f64,
        /// Afterbody length from the constant section.
        tail_m: f64,
    },
    /// Trapezoidal surface with NACA four-series sections and its root leading edge at the origin.
    Wing {
        /// Full projected span of a symmetric surface, exposed span of a one-sided surface.
        span_m: f64,
        /// Projected planform area over that span.
        area_m2: f64,
        /// Tip/root chord ratio.
        taper: f64,
        /// Leading-edge sweep.
        sweep_deg: f64,
        /// Dihedral from the local x/y plane.
        dihedral_deg: f64,
        /// Thickness/chord ratio.
        thickness: f64,
        /// Mirror across the local x/z plane.
        symmetric: bool,
    },
}

/// One canonical object placed in vehicle coordinates with kernel rotation angles.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct KernelObject {
    /// Canonical geometry identifier.
    pub id: String,
    /// Resolved shape.
    pub shape: KernelShape,
    /// Local origin in vehicle coordinates.
    pub location_m: [f64; 3],
    /// `X_Rel_Rotation`, `Y_Rel_Rotation` and `Z_Rel_Rotation`.
    pub rotation_deg: [f64; 3],
}

const AXES: [&str; 3] = ["X", "Y", "Z"];

/// Generate every object, read the kernel's values back, and write the model file.
pub(crate) fn build(objects: &[KernelObject]) -> AexResult<String> {
    let mut script = String::from("int main() {\nClearVSPModel();\n");
    for (index, object) in objects.iter().enumerate() {
        validate_id(&object.id)?;
        let geom = format!("g{index}");
        match &object.shape {
            KernelShape::Body { .. } => body(&mut script, &geom, object),
            KernelShape::Wing { .. } => wing(&mut script, &geom, object),
        }
        placement(&mut script, &geom, object);
    }
    script.push_str("Update();\n");
    for (index, object) in objects.iter().enumerate() {
        readback(&mut script, &format!("g{index}"), object);
    }
    writeln!(script, "WriteVSPFile(\"{MODEL_FILE}\", SET_ALL);").ok();
    script.push_str("while (GetNumTotalErrors() > 0) { ErrorObj err = PopLastError(); Print(\"ACE_ERROR=\" + err.GetErrorString()); }\n");
    script.push_str("Print(\"ACE_BUILD=1\");\nPrint(\"ACE_COMPLETE=1\");\nreturn 0;\n}\n");
    Ok(script)
}

/// Kernel values that must reproduce the canonical request, keyed `<id>.<parameter>`.
pub(crate) fn expected_readback(objects: &[KernelObject]) -> BTreeMap<String, f64> {
    let mut expected = BTreeMap::new();
    for object in objects {
        for (axis, value) in AXES.iter().zip(object.location_m) {
            expected.insert(format!("{}.{axis}_Rel_Location", object.id), value);
        }
        for (axis, value) in AXES.iter().zip(object.rotation_deg) {
            expected.insert(format!("{}.{axis}_Rel_Rotation", object.id), value);
        }
        match &object.shape {
            KernelShape::Body {
                length_m,
                nose_m,
                tail_m,
                ..
            } => {
                expected.insert(format!("{}.Length", object.id), *length_m);
                for (section, station) in body_stations(*length_m, *nose_m, *tail_m) {
                    expected.insert(format!("{}.XLocPercent_{section}", object.id), station);
                }
            }
            KernelShape::Wing {
                span_m,
                area_m2,
                dihedral_deg,
                ..
            } => {
                expected.insert(format!("{}.TotalProjectedSpan", object.id), *span_m);
                expected.insert(
                    format!("{}.TotalArea", object.id),
                    area_m2 / dihedral_deg.to_radians().cos(),
                );
            }
        }
    }
    expected
}

fn validate_id(id: &str) -> AexResult<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(AexError::validation(
            "INVALID_GEOMETRY_ID",
            id,
            "kernel object identifiers use ASCII letters, digits, '_', '-' and '.'",
        ));
    }
    Ok(())
}

/// Constant-section stations as fractions of the length; the middle section splits the
/// constant region so the loft keeps the declared width and height there.
fn body_stations(length_m: f64, nose_m: f64, tail_m: f64) -> [(u8, f64); 3] {
    let nose = nose_m / length_m;
    let tail = 1.0 - tail_m / length_m;
    [(1, nose), (2, 0.5 * (nose + tail)), (3, tail)]
}

fn body(script: &mut String, geom: &str, object: &KernelObject) {
    let KernelShape::Body {
        length_m,
        width_m,
        height_m,
        nose_m,
        tail_m,
    } = &object.shape
    else {
        return;
    };
    writeln!(
        script,
        "string {geom} = AddGeom(\"FUSELAGE\", \"\");\nSetGeomName({geom}, \"{NAME_PREFIX}{}\");\nSetParmVal({geom}, \"Length\", \"Design\", {length_m:.15});\nstring {geom}s = GetXSecSurf({geom}, 0);",
        object.id
    ).ok();
    for section in 1..=3 {
        writeln!(script, "ChangeXSecShape({geom}s, {section}, XS_ELLIPSE);").ok();
    }
    script.push_str("Update();\n");
    for (section, station) in body_stations(*length_m, *nose_m, *tail_m) {
        writeln!(
            script,
            "SetParmVal(GetXSecParm(GetXSec({geom}s, {section}), \"XLocPercent\"), {station:.15});\nSetParmVal(GetXSecParm(GetXSec({geom}s, {section}), \"Ellipse_Width\"), {width_m:.15});\nSetParmVal(GetXSecParm(GetXSec({geom}s, {section}), \"Ellipse_Height\"), {height_m:.15});"
        ).ok();
    }
}

fn wing(script: &mut String, geom: &str, object: &KernelObject) {
    let KernelShape::Wing {
        span_m,
        area_m2,
        taper,
        sweep_deg,
        dihedral_deg,
        thickness,
        symmetric,
    } = &object.shape
    else {
        return;
    };
    // Canonical span, area and sweep are projected; kernel section values lie in the dihedral
    // plane, and the kernel measures a one-sided surface as a single panel.
    let cos_dihedral = dihedral_deg.to_radians().cos();
    let panel_span = if *symmetric { span_m * 0.5 } else { *span_m };
    let root_chord = 2.0 * area_m2 / (span_m * (1.0 + taper));
    let sweep = (sweep_deg.to_radians().tan() * cos_dihedral)
        .atan()
        .to_degrees();
    let symmetry = if *symmetric { "SYM_XZ" } else { "0" };
    writeln!(
        script,
        "string {geom} = AddGeom(\"WING\", \"\");\nSetGeomName({geom}, \"{NAME_PREFIX}{}\");\nSetDriverGroup({geom}, 1, SPAN_WSECT_DRIVER, ROOTC_WSECT_DRIVER, TIPC_WSECT_DRIVER);\nUpdate();\nSetParmVal({geom}, \"Sym_Planar_Flag\", \"Sym\", {symmetry});\nChangeXSecShape(GetXSecSurf({geom}, 0), 0, XS_FOUR_SERIES);\nChangeXSecShape(GetXSecSurf({geom}, 0), 1, XS_FOUR_SERIES);\nUpdate();",
        object.id
    ).ok();
    for (name, value) in [
        ("Span", panel_span / cos_dihedral),
        ("Root_Chord", root_chord),
        ("Tip_Chord", root_chord * taper),
        ("Sweep_Location", 0.0),
        ("Sweep", sweep),
        ("Dihedral", *dihedral_deg),
        ("Twist", 0.0),
    ] {
        writeln!(
            script,
            "SetParmVal(GetParm({geom}, \"{name}\", \"XSec_1\"), {value:.15});"
        )
        .ok();
    }
    for section in 0..2 {
        writeln!(
            script,
            "SetParmVal(GetXSecParm(GetXSec(GetXSecSurf({geom}, 0), {section}), \"ThickChord\"), {thickness:.15});\nSetParmVal(GetXSecParm(GetXSec(GetXSecSurf({geom}, 0), {section}), \"Camber\"), 0.0);"
        ).ok();
    }
}

fn placement(script: &mut String, geom: &str, object: &KernelObject) {
    for (axis, value) in AXES.iter().zip(object.location_m) {
        writeln!(
            script,
            "SetParmVal({geom}, \"{axis}_Rel_Location\", \"XForm\", {value:.15});"
        )
        .ok();
    }
    for (axis, value) in AXES.iter().zip(object.rotation_deg) {
        writeln!(
            script,
            "SetParmVal({geom}, \"{axis}_Rel_Rotation\", \"XForm\", {value:.15});"
        )
        .ok();
    }
}

fn readback(script: &mut String, geom: &str, object: &KernelObject) {
    let print = |script: &mut String, name: &str, expression: String| {
        writeln!(
            script,
            "Print(\"ACE_GEOMETRY_{}.{name}=\", false); Print({expression});",
            object.id
        )
        .ok();
    };
    for axis in AXES {
        for group in ["Location", "Rotation"] {
            print(
                script,
                &format!("{axis}_Rel_{group}"),
                format!("GetParmVal({geom}, \"{axis}_Rel_{group}\", \"XForm\")"),
            );
        }
    }
    match &object.shape {
        KernelShape::Body { .. } => {
            print(
                script,
                "Length",
                format!("GetParmVal({geom}, \"Length\", \"Design\")"),
            );
            for section in 1..=3 {
                print(
                    script,
                    &format!("XLocPercent_{section}"),
                    format!(
                        "GetParmVal(GetXSecParm(GetXSec({geom}s, {section}), \"XLocPercent\"))"
                    ),
                );
            }
        }
        KernelShape::Wing { .. } => {
            for name in ["TotalProjectedSpan", "TotalArea"] {
                print(
                    script,
                    name,
                    format!("GetParmVal({geom}, \"{name}\", \"WingGeom\")"),
                );
            }
        }
    }
}

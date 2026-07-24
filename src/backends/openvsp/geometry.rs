use std::path::Path;

use crate::backends::contracts::GeometryOutput;
use crate::domain::diagnostic::AexResult;
use crate::domain::schema::ResolvedScenario;
use crate::models::concept_geometry::ConceptGeometry;

use super::script_string;

pub(super) fn geometry_script(
    scenario: &ResolvedScenario,
    native: &GeometryOutput,
    artifact: &Path,
) -> AexResult<String> {
    let wing = &scenario.aircraft.wing;
    let sweep_deg = wing.sweep_quarter_chord_rad.to_degrees();
    let concept = ConceptGeometry::from_scenario(scenario);
    let artifact = script_string(artifact)?;
    Ok(format!(
        r#"void PrintErrors()
{{
    while ( GetNumTotalErrors() > 0 )
    {{
        ErrorObj err = PopLastError();
        Print( "ACE_ERROR=" + err.GetErrorString() );
    }}
}}

void SetEllipse( string surface, int index, double width, double height )
{{
    ChangeXSecShape( surface, index, XS_ELLIPSE );
    string section = GetXSec( surface, index );
    SetParmVal( GetXSecParm( section, "Ellipse_Width" ), width );
    SetParmVal( GetXSecParm( section, "Ellipse_Height" ), height );
}}

void main()
{{
    ClearVSPModel();

    string fuselage = AddGeom( "FUSELAGE", "" );
    SetGeomName( fuselage, "ACE_Fuselage" );
    SetParmVal( fuselage, "Length", "Design", {fuselage_length:.12} );
    string fuselage_surface = GetXSecSurf( fuselage, 0 );
    SetEllipse( fuselage_surface, 1, {nose_width:.12}, {nose_height:.12} );
    SetEllipse( fuselage_surface, 2, {fuselage_width:.12}, {fuselage_height:.12} );
    SetEllipse( fuselage_surface, 3, {tail_width:.12}, {tail_height:.12} );

    string wing = AddGeom( "WING", "" );
    SetGeomName( wing, "ACE_Main_Wing" );
    SetParmVal( wing, "TotalArea", "WingGeom", {wing_area:.12} );
    SetParmVal( wing, "TotalAR", "WingGeom", {aspect_ratio:.12} );
    SetParmVal( wing, "Taper", "XSec_1", {taper_ratio:.12} );
    SetParmVal( wing, "Sweep", "XSec_1", {sweep_deg:.12} );
    SetParmVal( wing, "Dihedral", "XSec_1", {dihedral_deg:.12} );
    SetParmVal( wing, "Twist", "XSec_1", {twist_deg:.12} );
    SetParmVal( wing, "X_Rel_Location", "XForm", {wing_x:.12} );
    SetParmVal( wing, "Z_Rel_Location", "XForm", {wing_z:.12} );
    SetParmVal( wing, "Sym_Planar_Flag", "Sym", SYM_XZ );
    SetParmVal( wing, "Camber", "XSecCurve_0", {wing_camber:.12} );
    SetParmVal( wing, "CamberLoc", "XSecCurve_0", 0.4 );
    SetParmVal( wing, "ThickChord", "XSecCurve_0", 0.12 );
    SetParmVal( wing, "Camber", "XSecCurve_1", {wing_camber:.12} );
    SetParmVal( wing, "CamberLoc", "XSecCurve_1", 0.4 );
    SetParmVal( wing, "ThickChord", "XSecCurve_1", 0.12 );

    string horizontal_tail = AddGeom( "WING", "" );
    SetGeomName( horizontal_tail, "ACE_Horizontal_Tail" );
    SetParmVal( horizontal_tail, "TotalArea", "WingGeom", {horizontal_tail_area:.12} );
    SetParmVal( horizontal_tail, "TotalAR", "WingGeom", 4.0 );
    SetParmVal( horizontal_tail, "Taper", "XSec_1", 0.55 );
    SetParmVal( horizontal_tail, "X_Rel_Location", "XForm", {tail_x:.12} );
    SetParmVal( horizontal_tail, "Z_Rel_Location", "XForm", {tail_z:.12} );
    SetParmVal( horizontal_tail, "Sym_Planar_Flag", "Sym", SYM_XZ );

    string vertical_tail = AddGeom( "WING", "" );
    SetGeomName( vertical_tail, "ACE_Vertical_Tail" );
    SetParmVal( vertical_tail, "TotalArea", "WingGeom", {vertical_tail_area:.12} );
    SetParmVal( vertical_tail, "TotalAR", "WingGeom", 1.8 );
    SetParmVal( vertical_tail, "Taper", "XSec_1", 0.45 );
    SetParmVal( vertical_tail, "X_Rel_Location", "XForm", {tail_x:.12} );
    SetParmVal( vertical_tail, "Z_Rel_Location", "XForm", {tail_z:.12} );
    SetParmVal( vertical_tail, "X_Rel_Rotation", "XForm", 90.0 );
    SetParmVal( vertical_tail, "Sym_Planar_Flag", "Sym", 0 );

    Update();
    WriteVSPFile( "{artifact}", SET_ALL );

    SetAnalysisInputDefaults( "CompGeom" );
    string result_id = ExecAnalysis( "CompGeom" );
    array<double> wet_areas = GetDoubleResults( result_id, "Wet_Area" );
    double wetted_area = 0.0;
    for ( uint i = 0; i < wet_areas.size(); i++ )
    {{
        wetted_area += wet_areas[i];
    }}
    Print( "ACE_WETTED_AREA_M2=", false );
    Print( wetted_area );
    PrintErrors();
    Print( "ACE_COMPLETE=1" );
}}
"#,
        fuselage_length = concept.fuselage_length_m,
        fuselage_width = concept.fuselage_width_m,
        fuselage_height = concept.fuselage_height_m,
        nose_width = concept.fuselage_width_m * 0.72,
        nose_height = concept.fuselage_height_m * 0.78,
        tail_width = concept.fuselage_width_m * 0.42,
        tail_height = concept.fuselage_height_m * 0.52,
        wing_area = wing.area_m2,
        aspect_ratio = wing.aspect_ratio,
        taper_ratio = concept.taper_ratio,
        dihedral_deg = if scenario.aircraft.category.contains("transport") {
            5.0
        } else {
            1.5
        },
        twist_deg = if scenario.aircraft.category.contains("transport") {
            -1.5
        } else {
            -2.0
        },
        wing_camber = if scenario.aircraft.category.contains("transport") {
            0.015
        } else {
            0.02
        },
        wing_x = concept.wing_x_m,
        wing_z = concept.wing_z_m,
        horizontal_tail_area = native.metrics.horizontal_tail_area.value,
        vertical_tail_area = native.metrics.vertical_tail_area.value,
        tail_x = concept.tail_x_m,
        tail_z = concept.tail_z_m,
    ))
}

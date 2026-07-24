use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::NamedTempFile;

use crate::backends::contracts::{
    AnalysisBackend, AnalysisOutput, AnalysisRequest, BackendDescriptor, GeometryBackend,
    GeometryOutput, GeometryRequest, ResultProvenance,
};
use crate::backends::native::NativeBackend;
use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::schema::{ResolvedScenario, SegmentKind};

mod geometry;
mod parsing;

use geometry::{blended_wing_center_of_gravity_x, geometry_script, is_blended_wing_body};
use parsing::{marker_number, maximum_lift_to_drag_ratio, polar_points, stability_summary};

#[derive(Debug, Clone)]
pub(crate) struct OpenVspBackend {
    executable: PathBuf,
    version: Option<String>,
    search_path: OsString,
}

impl OpenVspBackend {
    pub(crate) fn new(executable: PathBuf, version: Option<String>, search_path: OsString) -> Self {
        Self {
            executable,
            version,
            search_path,
        }
    }
}

impl GeometryBackend for OpenVspBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor_value()
    }

    fn generate_geometry_blocking(
        &self,
        request: GeometryRequest<'_>,
    ) -> AexResult<GeometryOutput> {
        let artifact = request.artifact_path.ok_or_else(|| {
            AexError::validation(
                "MISSING_OPENVSP_ARTIFACT",
                "artifact_path",
                "OpenVSP geometry generation requires a .vsp3 artifact path",
            )
        })?;
        let artifact = absolute_path_blocking(artifact)?;
        let parent = artifact.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| AexError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
        let native = NativeBackend.generate_geometry_blocking(GeometryRequest {
            scenario: request.scenario,
            artifact_path: None,
        })?;
        let script = geometry_script(request.scenario, &native, &artifact)?;
        let output = self.run_script_blocking(
            parent,
            &script,
            "geometry generation",
            "ACE_WETTED_AREA_M2=",
        )?;
        let wetted_area = marker_number(&output, "ACE_WETTED_AREA_M2=")?;
        let mut metrics = native.metrics;
        metrics.wetted_area = QuantityOutput::si(wetted_area, "m^2");
        if is_blended_wing_body(request.scenario) {
            metrics.horizontal_tail_area = QuantityOutput::si(0.0, "m^2");
            metrics.vertical_tail_area = QuantityOutput::si(0.0, "m^2");
        }
        Ok(GeometryOutput {
            metrics,
            artifact_path: Some(artifact),
            provenance: openvsp_geometry_provenance(request.scenario),
        })
    }
}

impl AnalysisBackend for OpenVspBackend {
    fn analyze_blocking(&self, request: AnalysisRequest<'_>) -> AexResult<AnalysisOutput> {
        let artifact = request.geometry.artifact_path.as_deref().ok_or_else(|| {
            AexError::validation(
                "MISSING_OPENVSP_ARTIFACT",
                "geometry.artifact_path",
                "VSPAERO requires a generated .vsp3 artifact",
            )
        })?;
        let parent = artifact.parent().unwrap_or_else(|| Path::new("."));
        let script = analysis_script(request.scenario, artifact)?;
        let output = self.run_script_blocking(parent, &script, "VSPAERO polar", "ACE_POLAR=")?;
        let polar = polar_points(&output)?;
        let stability = stability_summary(&polar);
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "geometry.wetted_area".to_owned(),
            request.geometry.metrics.wetted_area.clone(),
        );
        if let Some(maximum_ratio) = maximum_lift_to_drag_ratio(&polar) {
            metrics.insert(
                "aerodynamics.maximum_lift_to_drag_ratio".to_owned(),
                QuantityOutput::si(maximum_ratio, "1"),
            );
        }
        Ok(AnalysisOutput {
            metrics,
            polar,
            stability,
            structural_screen: None,
            mission_power_screen: None,
            requirements: Vec::new(),
            feasible: None,
            failed_constraints: Vec::new(),
            provenance: openvsp_analysis_provenance(request.scenario),
        })
    }
}

impl OpenVspBackend {
    fn descriptor_value(&self) -> BackendDescriptor {
        BackendDescriptor {
            id: "openvsp".to_owned(),
            display_name: "OpenVSP subprocess refinement".to_owned(),
            available: true,
            version: self.version.clone(),
            capabilities: vec![
                "vsp3_geometry".to_owned(),
                "tailless_bwb_geometry".to_owned(),
                "wetted_area".to_owned(),
                "vspaero_polar".to_owned(),
                "static_pitching_moment".to_owned(),
            ],
            unavailable_reason: None,
        }
    }

    fn run_script_blocking(
        &self,
        directory: &Path,
        script: &str,
        operation: &str,
        success_marker: &str,
    ) -> AexResult<String> {
        let mut script_file =
            NamedTempFile::new_in(directory).map_err(|source| AexError::Write {
                path: directory.to_path_buf(),
                source,
            })?;
        script_file
            .write_all(script.as_bytes())
            .and_then(|()| script_file.flush())
            .map_err(|source| AexError::Write {
                path: script_file.path().to_path_buf(),
                source,
            })?;
        let result = Command::new(&self.executable)
            .args(["-script"])
            .arg(script_file.path())
            .current_dir(directory)
            .env("PATH", &self.search_path)
            .output()
            .map_err(|source| AexError::BackendLaunch {
                executable: self.executable.clone(),
                source,
            })?;
        let mut output = String::from_utf8_lossy(&result.stdout).into_owned();
        output.push_str(&String::from_utf8_lossy(&result.stderr));
        if let Some(error) = output
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix("ACE_ERROR="))
        {
            return Err(backend_failure(operation, error, result.status.code()));
        }
        let lines = output.lines().map(str::trim).collect::<Vec<_>>();
        let protocol_complete = lines.iter().any(|line| line.starts_with(success_marker))
            && lines.contains(&"ACE_COMPLETE=1");
        if !protocol_complete {
            return Err(backend_failure(operation, &output, result.status.code()));
        }
        Ok(output)
    }
}

fn analysis_script(scenario: &ResolvedScenario, artifact: &Path) -> AexResult<String> {
    let artifact = script_string(artifact)?;
    let mach = cruise_mach(scenario).clamp(0.05, 0.90);
    let concept = crate::models::concept_geometry::ConceptGeometry::from_scenario(scenario);
    let mean_chord = scenario.aircraft.wing.area_m2 / scenario.aircraft.wing.span_m;
    let center_of_gravity_x = if is_blended_wing_body(scenario) {
        blended_wing_center_of_gravity_x(scenario)?
    } else {
        concept.wing_x_m + 0.30 * mean_chord
    };
    Ok(format!(
        r#"void PrintErrors()
{{
    while ( GetNumTotalErrors() > 0 )
    {{
        ErrorObj err = PopLastError();
        Print( "ACE_ERROR=" + err.GetErrorString() );
    }}
}}

void main()
{{
    ReadVSPFile( "{artifact}" );
    array<string> fuselages = FindGeomsWithName( "ACE_Fuselage" );
    DeleteGeomVec( fuselages );
    array<string> engine_envelopes = FindGeomsWithName( "ACE_Engine_Envelope" );
    DeleteGeomVec( engine_envelopes );
    Update();

    SetAnalysisInputDefaults( "VSPAEROComputeGeometry" );
    string geometry_id = ExecAnalysis( "VSPAEROComputeGeometry" );
    Print( "ACE_VSPAERO_GEOMETRY_ID=" + geometry_id );

    SetAnalysisInputDefaults( "VSPAEROSweep" );
    array<int> reference_flag;
    reference_flag.push_back( 1 );
    SetIntAnalysisInput( "VSPAEROSweep", "RefFlag", reference_flag );
    array<string> wings = FindGeomsWithName( "ACE_Main_Wing" );
    SetStringAnalysisInput( "VSPAEROSweep", "WingID", wings );
    array<double> center_of_gravity_x;
    center_of_gravity_x.push_back( {center_of_gravity_x:.12} );
    SetDoubleAnalysisInput( "VSPAEROSweep", "Xcg", center_of_gravity_x );
    array<double> alpha_start;
    alpha_start.push_back( -2.0 );
    SetDoubleAnalysisInput( "VSPAEROSweep", "AlphaStart", alpha_start );
    array<double> alpha_end;
    alpha_end.push_back( 8.0 );
    SetDoubleAnalysisInput( "VSPAEROSweep", "AlphaEnd", alpha_end );
    array<int> alpha_count;
    alpha_count.push_back( 6 );
    SetIntAnalysisInput( "VSPAEROSweep", "AlphaNpts", alpha_count );
    array<double> mach_start;
    mach_start.push_back( {mach:.12} );
    SetDoubleAnalysisInput( "VSPAEROSweep", "MachStart", mach_start );
    array<int> mach_count;
    mach_count.push_back( 1 );
    SetIntAnalysisInput( "VSPAEROSweep", "MachNpts", mach_count );
    ExecAnalysis( "VSPAEROSweep" );

    string polar_id = FindLatestResultsID( "VSPAERO_Polar" );
    array<double> alpha = GetDoubleResults( polar_id, "Alpha" );
    array<double> cl = GetDoubleResults( polar_id, "CLtot" );
    array<double> cd = GetDoubleResults( polar_id, "CDtot" );
    array<double> cm = GetDoubleResults( polar_id, "CMytot" );
    for ( uint i = 0; i < alpha.size(); i++ )
    {{
        Print( "ACE_POLAR=", false );
        Print( alpha[i], false );
        Print( ",", false );
        Print( cl[i], false );
        Print( ",", false );
        Print( cd[i], false );
        Print( ",", false );
        Print( cm[i] );
    }}
    PrintErrors();
    Print( "ACE_COMPLETE=1" );
}}
"#,
        center_of_gravity_x = center_of_gravity_x,
    ))
}

pub(super) fn script_string(path: &Path) -> AexResult<String> {
    let raw = path.to_string_lossy();
    if raw.contains(['\n', '\r', '\0']) {
        return Err(AexError::validation(
            "INVALID_ARTIFACT_PATH",
            path.display().to_string(),
            "OpenVSP artifact paths cannot contain control characters",
        ));
    }
    Ok(raw.replace('\\', "\\\\").replace('"', "\\\""))
}

fn absolute_path_blocking(path: &Path) -> AexResult<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|directory| directory.join(path))
        .map_err(|source| AexError::Read {
            path: PathBuf::from("."),
            source,
        })
}

fn cruise_mach(scenario: &ResolvedScenario) -> f64 {
    scenario
        .mission
        .segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Cruise)
        .and_then(|segment| segment.mach)
        .unwrap_or(0.2)
}

fn backend_failure(operation: &str, output: &str, code: Option<i32>) -> AexError {
    let tail = output
        .lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    AexError::BackendExecution {
        backend: "openvsp".to_owned(),
        operation: operation.to_owned(),
        message: format!("exit code {code:?}: {tail}"),
    }
}

fn openvsp_geometry_provenance(scenario: &ResolvedScenario) -> ResultProvenance {
    if is_blended_wing_body(scenario) {
        return blended_wing_body_geometry_provenance(scenario);
    }
    ResultProvenance {
        method: "OpenVSP parametric geometry and CompGeom".to_owned(),
        backend: "openvsp".to_owned(),
        assumptions: vec![
            "fuselage length and tail areas use native conceptual sizing inputs".to_owned(),
            "OpenVSP component parameters remain adapter-internal".to_owned(),
        ],
        validity_range: vec!["conventional fixed-wing planforms".to_owned()],
        units: BTreeMap::from([
            ("area".to_owned(), "m^2".to_owned()),
            ("length".to_owned(), "m".to_owned()),
        ]),
        warnings: vec![Diagnostic::limitation(
            "OpenVSP geometry refines shape metrics but does not validate structural packaging.",
        )],
    }
}

fn blended_wing_body_geometry_provenance(scenario: &ResolvedScenario) -> ResultProvenance {
    ResultProvenance {
        method: "OpenVSP two-panel flying-wing geometry and CompGeom".to_owned(),
        backend: "openvsp".to_owned(),
        assumptions: vec![
            "two spanwise panels approximate the blended centerbody and outer wing".to_owned(),
            "modified five-digit sections with a three-degree upward trailing edge approximate reflex"
                .to_owned(),
            format!(
                "one aft pod represents the {} installation envelope",
                scenario.engine.profile_id()
            ),
            "OpenVSP component parameters remain adapter-internal".to_owned(),
        ],
        validity_range: vec!["visual and low-order tailless BWB concepts".to_owned()],
        units: BTreeMap::from([
            ("area".to_owned(), "m^2".to_owned()),
            ("length".to_owned(), "m".to_owned()),
        ]),
        warnings: vec![Diagnostic::limitation(
            "The BWB geometry does not resolve inlet flow, internal volume, control-system sizing, or structural load paths.",
        )],
    }
}

fn openvsp_analysis_provenance(scenario: &ResolvedScenario) -> ResultProvenance {
    let excluded_geometry = if is_blended_wing_body(scenario) {
        "the engine envelope is non-lifting in the vortex-lattice interpretation"
    } else {
        "fuselage excluded from the lifting-surface solve"
    };
    ResultProvenance {
        method: "VSPAERO vortex-lattice alpha sweep".to_owned(),
        backend: "openvsp".to_owned(),
        assumptions: vec![
            "six alpha points from -2 to 8 degrees".to_owned(),
            "single mission cruise Mach".to_owned(),
            excluded_geometry.to_owned(),
            "pitching moments referenced to an area-weighted CG at 30% mean aerodynamic chord"
                .to_owned(),
        ],
        validity_range: vec![
            "attached subsonic flow".to_owned(),
            "low-order static aerodynamic trends".to_owned(),
        ],
        units: BTreeMap::from([
            ("angle_of_attack".to_owned(), "deg".to_owned()),
            ("aerodynamic_coefficients".to_owned(), "1".to_owned()),
        ]),
        warnings: vec![Diagnostic::limitation(
            "VSPAERO output is low-order and is not a certification or CFD result.",
        )],
    }
}

#[cfg(test)]
mod tests;

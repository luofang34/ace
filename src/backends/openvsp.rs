use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::NamedTempFile;

use crate::backends::contracts::{
    AnalysisBackend, AnalysisOutput, AnalysisRequest, BackendDescriptor,
    BackendTopologyCapabilities, GeometryBackend, GeometryOutput, GeometryRequest,
    ResultProvenance,
};
use crate::backends::native::NativeBackend;
use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::schema::{ResolvedScenario, SegmentKind};
use crate::models::blended_wing::is_blended_wing_body;

mod geometry;
mod parsing;

use geometry::{blended_wing_center_of_gravity_x, geometry_script};
use parsing::{
    maximum_lift_to_drag_ratio, polar_points, positive_marker_number, stability_summary,
};

const ANALYSIS_TEMPLATE: &str = include_str!("openvsp/scripts/analysis.vspscript");

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
        let wetted_area = positive_marker_number(&output, "ACE_WETTED_AREA_M2=")?;
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
            disciplines: vec!["geometry".to_owned(), "aerodynamics".to_owned()],
            fidelity_levels: vec![1, 2],
            topology: openvsp_topology_capabilities(),
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
    let values = [
        ("__ARTIFACT__", artifact),
        (
            "__CENTER_OF_GRAVITY_X__",
            format!("{center_of_gravity_x:.12}"),
        ),
        ("__MACH__", format!("{mach:.12}")),
    ];
    Ok(values
        .iter()
        .fold(ANALYSIS_TEMPLATE.to_owned(), |script, (key, value)| {
            script.replace(key, value)
        }))
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

pub(super) fn openvsp_topology_capabilities() -> BackendTopologyCapabilities {
    BackendTopologyCapabilities {
        component_kinds: vec![
            "fuselage".to_owned(),
            "wing".to_owned(),
            "horizontal_tail".to_owned(),
            "vertical_tail".to_owned(),
            "lifting_body".to_owned(),
            "engine".to_owned(),
            "propeller".to_owned(),
        ],
        relationship_kinds: vec!["attached_to".to_owned(), "symmetric_about".to_owned()],
        delegated_relationship_kinds: vec!["carries_load_to".to_owned()],
    }
}

pub(crate) fn openvsp_topology_violations(scenario: &ResolvedScenario) -> Vec<String> {
    let mut violations = Vec::new();
    let engine_count = scenario.aircraft.propulsion.engine_count;
    if engine_count > 2 {
        violations.push("OpenVSP topology supports at most two engine instances".to_owned());
    }
    if scenario.propeller.is_some() && engine_count != 1 {
        violations.push(
            "OpenVSP topology supports a propeller only for a single-engine configuration"
                .to_owned(),
        );
    }
    if is_blended_wing_body(scenario) && scenario.propeller.is_some() {
        violations.push("OpenVSP lifting-body geometry does not represent propellers".to_owned());
    }
    violations
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
            engine_envelope_assumption(scenario),
            "CompGeom wetted area includes the full airframe and propulsion envelope".to_owned(),
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
            "canonical planform and internal-volume fields retain native estimates; CompGeom refines wetted area"
                .to_owned(),
            format!(
                "{} aft pod envelope(s) represent the {} installation",
                scenario.aircraft.propulsion.engine_count,
                scenario.engine.profile_id()
            ),
            engine_envelope_assumption(scenario),
            "CompGeom wetted area includes the full lifting body and propulsion envelope".to_owned(),
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
        "the named lifting set excludes engine envelopes from the vortex-lattice interpretation"
    } else {
        "the named lifting set excludes the fuselage, engine envelopes, and propeller from the lifting-surface solve"
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

fn engine_envelope_assumption(scenario: &ResolvedScenario) -> String {
    match &scenario.engine {
        crate::domain::schema::EngineProfile::Turbofan(profile)
            if profile.overall_length_m.is_some() && profile.maximum_diameter_m.is_some() =>
        {
            "engine envelope dimensions come from the resolved propulsion profile".to_owned()
        }
        crate::domain::schema::EngineProfile::Turbofan(_) => {
            "missing engine envelope dimensions use dry-mass cube-root correlations".to_owned()
        }
        crate::domain::schema::EngineProfile::Piston(_) => {
            "piston engine envelope dimensions use dry-mass cube-root correlations".to_owned()
        }
    }
}

#[cfg(test)]
mod tests;

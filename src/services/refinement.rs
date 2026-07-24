use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::backends::contracts::{
    AnalysisBackend, AnalysisRequest, GeometryBackend, GeometryRequest, ResultProvenance,
};
use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::schema::{EngineProfile, ResolvedScenario, ScenarioDocument};
use crate::services::analysis::ApplicationService;
use crate::services::design_experiments::FeasibilityResult;
use crate::storage::design_store::DesignRecord;
use crate::storage::project_store::read_yaml_blocking;

#[derive(Debug, Clone)]
pub(crate) struct RefinementSpec<'a> {
    pub(crate) scenario_path: &'a Path,
    pub(crate) output_design_id: &'a str,
    pub(crate) display_name: &'a str,
    pub(crate) design_root: &'a Path,
    pub(crate) backend: &'a str,
    pub(crate) artifact_path: Option<&'a Path>,
    pub(crate) max_iterations: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RefinementStep {
    pub(crate) iteration: u32,
    pub(crate) score: f64,
    pub(crate) parameters: BTreeMap<String, String>,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) unmet_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RefinementResult {
    pub(crate) source_scenario_id: String,
    pub(crate) converged: bool,
    pub(crate) iterations: u32,
    pub(crate) evaluated_candidates: u32,
    pub(crate) selected_parameters: BTreeMap<String, String>,
    pub(crate) history: Vec<RefinementStep>,
    pub(crate) design: DesignRecord,
    pub(crate) evaluation: FeasibilityResult,
    pub(crate) backend_verification_passed: bool,
    pub(crate) verification_failures: Vec<String>,
    pub(crate) provenance: ResultProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DesignState {
    wing_area_m2: f64,
    aspect_ratio: f64,
    propulsion_sizing_factor: f64,
    maximum_fuel_mass_kg: f64,
}

#[derive(Debug, Clone, Copy)]
struct SearchSteps {
    wing_area_m2: f64,
    aspect_ratio: f64,
    propulsion_sizing_factor: f64,
    maximum_fuel_mass_kg: f64,
}

#[derive(Debug, Clone)]
struct Candidate {
    state: DesignState,
    score: f64,
    failed_constraints: Vec<String>,
    unmet_requirements: Vec<String>,
    conceptually_feasible: bool,
}

impl Candidate {
    fn converged(&self) -> bool {
        self.conceptually_feasible && self.unmet_requirements.is_empty()
    }
}

impl ApplicationService {
    pub(crate) fn auto_refine_design_blocking(
        &self,
        spec: RefinementSpec<'_>,
    ) -> AexResult<RefinementResult> {
        let baseline = self.resolve_blocking(spec.scenario_path, &BTreeMap::new())?;
        let initial = DesignState::from_scenario(&baseline);
        let mut steps = SearchSteps::from_state(initial);
        let mut evaluated_candidates = 0_u32;
        let mut current = self.evaluate_candidate(
            spec.scenario_path,
            &baseline,
            initial,
            initial,
            &mut evaluated_candidates,
        )?;
        let mut history = vec![step(0, &current, &baseline)];
        let mut iterations = 0_u32;
        for iteration in 1..=spec.max_iterations.clamp(1, 40) {
            if current.converged() {
                break;
            }
            iterations = iterations.wrapping_add(1);
            let mut best = current.clone();
            for state in neighbors(current.state, initial, steps) {
                let candidate = self.evaluate_candidate(
                    spec.scenario_path,
                    &baseline,
                    state,
                    initial,
                    &mut evaluated_candidates,
                )?;
                if candidate.score < best.score {
                    best = candidate;
                }
            }
            if best.state == current.state {
                steps = steps.halved();
                if steps.converged() {
                    history.push(step(iteration, &current, &baseline));
                    break;
                }
            } else {
                current = best;
            }
            history.push(step(iteration, &current, &baseline));
        }
        let selected_parameters = parameters(current.state, &baseline);
        let output_parameters = output_parameters(spec.scenario_path, &selected_parameters)?;
        let design = self.create_design_blocking(
            spec.output_design_id,
            spec.display_name,
            spec.design_root,
            None,
            Some(spec.scenario_path),
            &output_parameters,
        )?;
        let evaluation = self.evaluate_feasibility_blocking(
            &design.scenario_path,
            spec.backend,
            spec.artifact_path,
        )?;
        let verification_failures = backend_verification_failures(&evaluation);
        let backend_verification_passed = verification_failures.is_empty();
        let converged = evaluation.baseline.analysis.feasible.unwrap_or(false)
            && evaluation
                .baseline
                .analysis
                .requirements
                .iter()
                .all(|requirement| requirement.passed)
            && backend_verification_passed;
        Ok(RefinementResult {
            source_scenario_id: baseline.id,
            converged,
            iterations,
            evaluated_candidates,
            selected_parameters,
            history,
            design,
            evaluation,
            backend_verification_passed,
            verification_failures,
            provenance: refinement_provenance(),
        })
    }

    fn evaluate_candidate(
        &self,
        scenario_path: &Path,
        baseline: &ResolvedScenario,
        state: DesignState,
        initial: DesignState,
        evaluated_candidates: &mut u32,
    ) -> AexResult<Candidate> {
        *evaluated_candidates = evaluated_candidates.wrapping_add(1);
        let scenario = self.resolve_blocking(scenario_path, &parameters(state, baseline))?;
        let native = self.backends.native();
        let geometry = native.generate_geometry_blocking(GeometryRequest {
            scenario: &scenario,
            artifact_path: None,
        })?;
        let analysis = native.analyze_blocking(AnalysisRequest {
            scenario: &scenario,
            geometry: &geometry,
        })?;
        let unmet_requirements = analysis
            .requirements
            .iter()
            .filter(|requirement| !requirement.passed)
            .map(|requirement| requirement.id.clone())
            .collect::<Vec<_>>();
        let score = candidate_score(&analysis, state, initial);
        Ok(Candidate {
            state,
            score,
            failed_constraints: analysis.failed_constraints,
            unmet_requirements,
            conceptually_feasible: analysis.feasible.unwrap_or(false),
        })
    }
}

fn output_parameters(
    scenario_path: &Path,
    selected: &BTreeMap<String, String>,
) -> AexResult<BTreeMap<String, String>> {
    let source: ScenarioDocument = read_yaml_blocking(scenario_path)?;
    let mut output = source.scenario.overrides;
    output.extend(selected.clone());
    Ok(output)
}

impl DesignState {
    fn from_scenario(scenario: &ResolvedScenario) -> Self {
        Self {
            wing_area_m2: scenario.aircraft.wing.area_m2,
            aspect_ratio: scenario.aircraft.wing.aspect_ratio,
            propulsion_sizing_factor: scenario.aircraft.propulsion.sizing_factor,
            maximum_fuel_mass_kg: scenario.aircraft.mass.maximum_fuel_mass_kg,
        }
    }
}

impl SearchSteps {
    fn from_state(state: DesignState) -> Self {
        Self {
            wing_area_m2: state.wing_area_m2 * 0.05,
            aspect_ratio: 0.5,
            propulsion_sizing_factor: 0.05,
            maximum_fuel_mass_kg: state.maximum_fuel_mass_kg * 0.05,
        }
    }

    fn halved(self) -> Self {
        Self {
            wing_area_m2: self.wing_area_m2 * 0.5,
            aspect_ratio: self.aspect_ratio * 0.5,
            propulsion_sizing_factor: self.propulsion_sizing_factor * 0.5,
            maximum_fuel_mass_kg: self.maximum_fuel_mass_kg * 0.5,
        }
    }

    fn converged(self) -> bool {
        self.aspect_ratio < 0.05 && self.propulsion_sizing_factor < 0.01
    }
}

fn neighbors(current: DesignState, initial: DesignState, steps: SearchSteps) -> Vec<DesignState> {
    let area_bounds = (initial.wing_area_m2 * 0.8, initial.wing_area_m2 * 1.25);
    let fuel_bounds = (
        initial.maximum_fuel_mass_kg * 0.8,
        initial.maximum_fuel_mass_kg * 1.25,
    );
    let bounds = |value: f64, delta: f64, limits: (f64, f64)| {
        [
            (value - delta).clamp(limits.0, limits.1),
            (value + delta).clamp(limits.0, limits.1),
        ]
    };
    let mut states = Vec::with_capacity(8);
    for value in bounds(current.wing_area_m2, steps.wing_area_m2, area_bounds) {
        states.push(DesignState {
            wing_area_m2: value,
            ..current
        });
    }
    for value in bounds(current.aspect_ratio, steps.aspect_ratio, (5.0, 12.0)) {
        states.push(DesignState {
            aspect_ratio: value,
            ..current
        });
    }
    for value in bounds(
        current.propulsion_sizing_factor,
        steps.propulsion_sizing_factor,
        (0.85, 1.5),
    ) {
        states.push(DesignState {
            propulsion_sizing_factor: value,
            ..current
        });
    }
    for value in bounds(
        current.maximum_fuel_mass_kg,
        steps.maximum_fuel_mass_kg,
        fuel_bounds,
    ) {
        states.push(DesignState {
            maximum_fuel_mass_kg: value,
            ..current
        });
    }
    states
}

fn parameters(state: DesignState, baseline: &ResolvedScenario) -> BTreeMap<String, String> {
    let span = (state.wing_area_m2 * state.aspect_ratio).sqrt();
    let engine_mass =
        engine_mass_kg(&baseline.engine) * f64::from(baseline.aircraft.propulsion.engine_count);
    let empty_mass = baseline.aircraft.mass.operating_empty_mass_kg
        + 1.15 * engine_mass * (state.propulsion_sizing_factor - 1.0);
    BTreeMap::from([
        (
            "aircraft.geometry.wing.area".to_owned(),
            format!("{:.6} m^2", state.wing_area_m2),
        ),
        (
            "aircraft.geometry.wing.aspect_ratio".to_owned(),
            format!("{:.6}", state.aspect_ratio),
        ),
        (
            "aircraft.geometry.wing.span".to_owned(),
            format!("{span:.6} m"),
        ),
        (
            "aircraft.mass.maximum_fuel_mass".to_owned(),
            format!("{:.6} kg", state.maximum_fuel_mass_kg),
        ),
        (
            "aircraft.mass.operating_empty_mass".to_owned(),
            format!("{empty_mass:.6} kg"),
        ),
        (
            "aircraft.propulsion.sizing_factor".to_owned(),
            format!("{:.6}", state.propulsion_sizing_factor),
        ),
    ])
}

fn engine_mass_kg(engine: &EngineProfile) -> f64 {
    match engine {
        EngineProfile::Piston(profile) => profile.dry_mass_kg,
        EngineProfile::Turbofan(profile) => profile.dry_mass_kg,
    }
}

fn candidate_score(
    analysis: &crate::backends::contracts::AnalysisOutput,
    state: DesignState,
    initial: DesignState,
) -> f64 {
    let requirement_penalty = analysis
        .requirements
        .iter()
        .filter(|requirement| !requirement.passed)
        .map(|requirement| {
            let severity = if requirement.severity == "hard" {
                10_000.0
            } else {
                1_000.0
            };
            severity
                + 100.0 * (-requirement.absolute_margin / requirement.required.value.abs()).max(0.0)
        })
        .sum::<f64>();
    let requirement_ids = analysis
        .requirements
        .iter()
        .map(|requirement| requirement.id.as_str())
        .collect::<Vec<_>>();
    let gate_penalty = analysis
        .failed_constraints
        .iter()
        .filter(|failure| !requirement_ids.contains(&failure.as_str()))
        .count() as f64
        * 10_000.0;
    let power_penalty = analysis
        .mission_power_screen
        .as_ref()
        .map_or(0.0, |screen| {
            (-screen.minimum_reserve_margin.value / 1_000.0).max(0.0)
        });
    requirement_penalty + gate_penalty + power_penalty + change_penalty(state, initial)
}

fn change_penalty(state: DesignState, initial: DesignState) -> f64 {
    let relative = |value: f64, reference: f64| (value / reference - 1.0).powi(2);
    10.0 * relative(state.wing_area_m2, initial.wing_area_m2)
        + 5.0 * relative(state.aspect_ratio, initial.aspect_ratio)
        + 20.0
            * relative(
                state.propulsion_sizing_factor,
                initial.propulsion_sizing_factor,
            )
        + 5.0 * relative(state.maximum_fuel_mass_kg, initial.maximum_fuel_mass_kg)
}

fn step(iteration: u32, candidate: &Candidate, baseline: &ResolvedScenario) -> RefinementStep {
    RefinementStep {
        iteration,
        score: candidate.score,
        parameters: parameters(candidate.state, baseline),
        failed_constraints: candidate.failed_constraints.clone(),
        unmet_requirements: candidate.unmet_requirements.clone(),
    }
}

fn backend_verification_failures(result: &FeasibilityResult) -> Vec<String> {
    result
        .refinement
        .as_ref()
        .map_or_else(Vec::new, |refinement| {
            if refinement.analysis.stability.statically_stable == Some(false) {
                vec!["openvsp.static_pitch_stability".to_owned()]
            } else {
                Vec::new()
            }
        })
}

fn refinement_provenance() -> ResultProvenance {
    ResultProvenance {
        method: "deterministic bounded coordinate refinement".to_owned(),
        backend: "native with optional final backend verification".to_owned(),
        assumptions: vec![
            "wing span follows sqrt(area times aspect ratio)".to_owned(),
            "powerplant mass scales at 1.15 times engine dry-mass change".to_owned(),
            "search variables are wing area, aspect ratio, propulsion sizing, and fuel capacity"
                .to_owned(),
        ],
        validity_range: vec!["local refinement around an existing fixed-wing concept".to_owned()],
        units: BTreeMap::from([("score".to_owned(), "ranking-only".to_owned())]),
        warnings: vec![Diagnostic::limitation(
            "Convergence means all implemented conceptual screens pass; it is not structural substantiation or certification.",
        )],
    }
}

#[cfg(test)]
mod tests;

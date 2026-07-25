mod constraints;
mod validity;
mod verdict;
use std::collections::BTreeMap;

use crate::backends::contracts::{
    AnalysisBackend, AnalysisRequest, GeometryBackend, GeometryMetrics, GeometryRequest,
};
use crate::domain::content_identity::digest_serializable;
use crate::domain::diagnostic::{AexError, AexResult, Diagnostic, Severity};
use crate::domain::evidence::{
    CandidateDescriptor, CandidateOutcome, EvaluationStatus, EvidenceAnalysis, EvidenceDraft,
    EvidenceEnvelope, EvidenceProvenance, EvidenceResults,
};
use crate::domain::quantity::{Dimension, QuantityOutput, parse_quantity};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::study::StudyDefinition;
use crate::services::analysis::ApplicationService;
use crate::services::study::loading::PreparedStudy;

use validity::{combined_domains, recorded_metrics};
use verdict::{candidate_is_feasible, insert_metric_aliases};

const FAILED_RANK: f64 = 1.0e12;

pub(super) struct EvaluatedCandidate {
    pub(super) candidate: CandidateDescriptor,
    pub(super) outcome: CandidateOutcome,
    pub(super) evidence: EvidenceEnvelope,
}

pub(super) fn candidate_descriptor(
    prepared: &PreparedStudy,
    genes: &BTreeMap<String, String>,
) -> AexResult<CandidateDescriptor> {
    let mut parameters = BTreeMap::new();
    for variable in &prepared.document.study.variables {
        if active(&variable.active_when, genes)
            && let Some(value) = genes.get(&variable.id)
        {
            parameters.insert(variable.path.clone(), value.clone());
        }
    }
    apply_derivations(prepared, &mut parameters)?;
    CandidateDescriptor::new(prepared.baseline_digest.clone(), parameters)
}

pub(super) fn evaluate_candidate_blocking(
    service: &ApplicationService,
    prepared: &PreparedStudy,
    candidate: CandidateDescriptor,
    generation: u32,
) -> AexResult<EvaluatedCandidate> {
    let input_digest = digest_serializable(&serde_json::json!({
        "study_digest": prepared.study_digest,
        "candidate": candidate,
        "evaluator": evaluator_signature(),
    }))?;
    match evaluate_native_blocking(service, prepared, &candidate, &input_digest) {
        Ok((evidence, feasible, violation, objective_values)) => Ok(EvaluatedCandidate {
            outcome: outcome(
                &candidate,
                &evidence,
                generation,
                feasible,
                violation,
                objective_values,
            ),
            candidate,
            evidence,
        }),
        Err(source) => failed_evaluation(prepared, candidate, generation, input_digest, &source),
    }
}

pub(super) fn evaluator_signature() -> String {
    format!("native-study-evidence-v8:{}", env!("CARGO_PKG_VERSION"))
}

fn evaluate_native_blocking(
    service: &ApplicationService,
    prepared: &PreparedStudy,
    candidate: &CandidateDescriptor,
    input_digest: &str,
) -> AexResult<(EvidenceEnvelope, bool, f64, BTreeMap<String, f64>)> {
    let scenario = service.resolve_blocking(prepared.scenario_path(), &candidate.parameters)?;
    if let Some(unsupported) = service.feasibility_preflight(&scenario, "native")? {
        return Err(unsupported.as_error());
    }
    let native = service.backends.native();
    let geometry = native.generate_geometry_blocking(GeometryRequest {
        scenario: &scenario,
        artifact_path: None,
    })?;
    let analysis = native.analyze_blocking(AnalysisRequest {
        scenario: &scenario,
        geometry: &geometry,
    })?;
    let mut metrics = analysis.metrics.clone();
    insert_requirement_metrics(&mut metrics, &analysis.requirements);
    insert_geometry_metrics(&mut metrics, &geometry.metrics);
    insert_scenario_metrics(&mut metrics, &scenario);
    let mission_completed = insert_metric_aliases(
        &mut metrics,
        &scenario.requirements.items,
        &analysis.requirements,
    );
    let constraints = constraints::collect(
        &analysis.requirements,
        &analysis.failed_constraints,
        &prepared.document.study.constraints,
        &metrics,
        &analysis.metric_validity,
    )?;
    let mut diagnostics = geometry.provenance.warnings.clone();
    diagnostics.extend(analysis.provenance.warnings.clone());
    let objective_values = objective_values(&prepared.document.study, &metrics, &mut diagnostics);
    let objectives_available = objective_values.len() == prepared.document.study.objectives.len();
    let feasible = candidate_is_feasible(
        &scenario.requirements.items,
        &analysis.requirements,
        mission_completed,
        analysis.feasible.unwrap_or(false),
        &constraints,
        objectives_available,
    );
    let violation = hard_constraint_violation(&constraints);
    let status = if objectives_available {
        EvaluationStatus::Succeeded
    } else {
        EvaluationStatus::Failed
    };
    let evidence = EvidenceEnvelope::from_draft(EvidenceDraft {
        study_id: prepared.document.study.id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        input_digest: input_digest.to_owned(),
        status,
        analysis: evidence_analysis(&scenario),
        results: EvidenceResults {
            metric_validity: recorded_metrics(&metrics, &analysis.metric_validity),
            metrics,
            constraints,
            diagnostics,
        },
        provenance: EvidenceProvenance {
            assumptions: combined(
                &geometry.provenance.assumptions,
                &analysis.provenance.assumptions,
            ),
            validity_range: combined(
                &geometry.provenance.validity_range,
                &analysis.provenance.validity_range,
            ),
            validity_domains: combined_domains(
                &geometry.provenance.validity_domains,
                &analysis.provenance.validity_domains,
            ),
            confidence: None,
            dependencies: Vec::new(),
            artifacts: Vec::new(),
        },
    })?;
    Ok((evidence, feasible, violation, objective_values))
}

fn evidence_analysis(scenario: &ResolvedScenario) -> EvidenceAnalysis {
    EvidenceAnalysis {
        discipline: "multidisciplinary_conceptual_screen".to_owned(),
        operating_condition: scenario.mission.id.clone(),
        method: "native conceptual geometry, performance, mission, and structural screens"
            .to_owned(),
        backend: "native".to_owned(),
        software_version: env!("CARGO_PKG_VERSION").to_owned(),
        model_versions: BTreeMap::from([
            (
                "aerodynamics".to_owned(),
                scenario.aircraft.aerodynamics.model.clone(),
            ),
            (
                "propulsion".to_owned(),
                scenario.engine.model_id().to_owned(),
            ),
            (
                "structures".to_owned(),
                "structures.conceptual_screen.v2".to_owned(),
            ),
        ]),
        fidelity_level: 1,
    }
}

fn outcome(
    candidate: &CandidateDescriptor,
    evidence: &EvidenceEnvelope,
    generation: u32,
    feasible: bool,
    violation: f64,
    objective_values: BTreeMap<String, f64>,
) -> CandidateOutcome {
    CandidateOutcome {
        candidate_id: candidate.candidate_id.clone(),
        evaluation_id: evidence.evaluation_id.clone(),
        generation,
        feasible,
        normalized_constraint_violation: violation,
        objective_values,
        rank_score: FAILED_RANK,
    }
}

fn insert_geometry_metrics(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    geometry: &GeometryMetrics,
) {
    for (id, value) in [
        ("geometry.wing_span", &geometry.wing_span),
        (
            "geometry.mean_aerodynamic_chord",
            &geometry.mean_aerodynamic_chord,
        ),
        ("geometry.wetted_area", &geometry.wetted_area),
    ] {
        metrics.insert(id.to_owned(), value.clone());
    }
    metrics.insert(
        "geometry.aspect_ratio".to_owned(),
        QuantityOutput::si(geometry.aspect_ratio, "1"),
    );
}

fn insert_requirement_metrics(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    requirements: &[crate::domain::result::RequirementEvaluation],
) {
    for requirement in requirements {
        metrics
            .entry(requirement.metric.clone())
            .or_insert_with(|| requirement.actual.clone());
    }
}

fn insert_scenario_metrics(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    scenario: &ResolvedScenario,
) {
    let mass = &scenario.aircraft.mass;
    for (id, value) in [
        ("mass.maximum_takeoff", mass.maximum_takeoff_mass_kg),
        ("mass.operating_empty", mass.operating_empty_mass_kg),
        ("mass.maximum_payload", mass.maximum_payload_mass_kg),
        ("mass.maximum_fuel", mass.maximum_fuel_mass_kg),
        ("mission.payload_mass", scenario.mission.payload_mass_kg),
    ] {
        metrics.insert(id.to_owned(), QuantityOutput::si(value, "kg"));
    }
}

fn objective_values(
    study: &StudyDefinition,
    metrics: &BTreeMap<String, QuantityOutput>,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, f64> {
    study
        .objectives
        .iter()
        .filter_map(|objective| {
            metrics.get(&objective.metric).map_or_else(
                || {
                    diagnostics.push(Diagnostic {
                        code: "STUDY_OBJECTIVE_UNAVAILABLE".to_owned(),
                        severity: Severity::Error,
                        message: format!(
                            "objective {} requires missing metric {}",
                            objective.id, objective.metric
                        ),
                        path: Some(format!("study.objectives.{}", objective.id)),
                        context: serde_json::Value::Null,
                    });
                    None
                },
                |metric| Some((objective.id.clone(), metric.value)),
            )
        })
        .collect()
}

fn hard_constraint_violation(constraints: &[crate::domain::evidence::EvidenceConstraint]) -> f64 {
    constraints
        .iter()
        .filter(|constraint| constraint.severity == "hard")
        .map(|constraint| constraint.normalized_violation)
        .sum()
}

fn apply_derivations(
    prepared: &PreparedStudy,
    parameters: &mut BTreeMap<String, String>,
) -> AexResult<()> {
    for derived in &prepared.document.study.derived_parameters {
        let value = match derived.method.as_str() {
            "derive_from_area_and_aspect_ratio" | "wing_span_from_area_and_aspect_ratio" => {
                derive_wing_span(prepared, parameters)?
            }
            "preserve_baseline_mass_closure" => {
                apply_mass_closure(prepared, parameters)?;
                continue;
            }
            "operating_empty_mass_from_propulsion_sizing" => {
                derive_mass_closure(prepared, parameters, &derived.target)?
            }
            _ => {
                return Err(AexError::validation(
                    "UNKNOWN_STUDY_DERIVATION",
                    "study.derived_parameters.method",
                    &derived.method,
                ));
            }
        };
        parameters.insert(derived.target.clone(), value);
    }
    Ok(())
}

fn derive_wing_span(
    prepared: &PreparedStudy,
    parameters: &BTreeMap<String, String>,
) -> AexResult<String> {
    let area = parameters
        .get("aircraft.geometry.wing.area")
        .map(|value| parse_quantity(value, Dimension::Area))
        .transpose()?
        .unwrap_or(prepared.scenario.aircraft.wing.area_m2);
    let aspect_ratio = parameter_number(
        parameters,
        "aircraft.geometry.wing.aspect_ratio",
        prepared.scenario.aircraft.wing.aspect_ratio,
    )?;
    Ok(format!("{} m", (area * aspect_ratio).sqrt()))
}

fn apply_mass_closure(
    prepared: &PreparedStudy,
    parameters: &mut BTreeMap<String, String>,
) -> AexResult<()> {
    let sizing = propulsion_sizing(prepared, parameters)?;
    let propulsion_delta = propulsion_mass_delta(prepared, sizing);
    let mass = &prepared.scenario.aircraft.mass;
    let fuel = parameters
        .get("aircraft.mass.maximum_fuel_mass")
        .map(|value| parse_quantity(value, Dimension::Mass))
        .transpose()?
        .unwrap_or(mass.maximum_fuel_mass_kg);
    parameters.insert(
        "aircraft.mass.operating_empty_mass".to_owned(),
        format!("{} kg", mass.operating_empty_mass_kg + propulsion_delta),
    );
    parameters.insert(
        "aircraft.mass.maximum_takeoff_mass".to_owned(),
        format!(
            "{} kg",
            mass.maximum_takeoff_mass_kg + propulsion_delta + fuel - mass.maximum_fuel_mass_kg
        ),
    );
    Ok(())
}

fn derive_mass_closure(
    prepared: &PreparedStudy,
    parameters: &BTreeMap<String, String>,
    target: &str,
) -> AexResult<String> {
    let sizing = propulsion_sizing(prepared, parameters)?;
    let mass = &prepared.scenario.aircraft.mass;
    let propulsion_delta = propulsion_mass_delta(prepared, sizing);
    let value = if target == "aircraft.mass.operating_empty_mass" {
        mass.operating_empty_mass_kg + propulsion_delta
    } else {
        let fuel = parameters
            .get("aircraft.mass.maximum_fuel_mass")
            .map(|value| parse_quantity(value, Dimension::Mass))
            .transpose()?
            .unwrap_or(mass.maximum_fuel_mass_kg);
        mass.maximum_takeoff_mass_kg + propulsion_delta + fuel - mass.maximum_fuel_mass_kg
    };
    Ok(format!("{value} kg"))
}

fn propulsion_sizing(
    prepared: &PreparedStudy,
    parameters: &BTreeMap<String, String>,
) -> AexResult<f64> {
    parameter_number(
        parameters,
        "aircraft.propulsion.sizing_factor",
        prepared.scenario.aircraft.propulsion.sizing_factor,
    )
}

fn propulsion_mass_delta(prepared: &PreparedStudy, sizing: f64) -> f64 {
    let dry_mass = match &prepared.scenario.engine {
        EngineProfile::Piston(profile) => profile.dry_mass_kg,
        EngineProfile::Turbofan(profile) => profile.dry_mass_kg,
    };
    1.15 * dry_mass * f64::from(prepared.scenario.aircraft.propulsion.engine_count) * (sizing - 1.0)
}

fn parameter_number(
    parameters: &BTreeMap<String, String>,
    path: &str,
    default: f64,
) -> AexResult<f64> {
    parameters
        .get(path)
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|source| {
            AexError::validation("INVALID_STUDY_DERIVATION_INPUT", path, source.to_string())
        })
        .map(|value| value.unwrap_or(default))
}

fn active(required: &BTreeMap<String, String>, genes: &BTreeMap<String, String>) -> bool {
    required
        .iter()
        .all(|(id, value)| genes.get(id) == Some(value))
}

fn combined(first: &[String], second: &[String]) -> Vec<String> {
    first.iter().chain(second).cloned().collect()
}

fn failed_evaluation(
    prepared: &PreparedStudy,
    candidate: CandidateDescriptor,
    generation: u32,
    input_digest: String,
    source: &AexError,
) -> AexResult<EvaluatedCandidate> {
    let evidence = EvidenceEnvelope::from_draft(EvidenceDraft {
        study_id: prepared.document.study.id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        input_digest,
        status: EvaluationStatus::Failed,
        analysis: EvidenceAnalysis {
            discipline: "multidisciplinary_conceptual_screen".to_owned(),
            operating_condition: prepared.scenario.mission.id.clone(),
            method: "native conceptual screening".to_owned(),
            backend: "native".to_owned(),
            software_version: env!("CARGO_PKG_VERSION").to_owned(),
            model_versions: BTreeMap::new(),
            fidelity_level: 1,
        },
        results: EvidenceResults {
            metrics: BTreeMap::new(),
            metric_validity: BTreeMap::new(),
            constraints: Vec::new(),
            diagnostics: vec![Diagnostic {
                code: "CANDIDATE_EVALUATION_FAILED".to_owned(),
                severity: Severity::Error,
                message: source.to_string(),
                path: Some("study.candidate".to_owned()),
                context: serde_json::Value::Null,
            }],
        },
        provenance: EvidenceProvenance {
            assumptions: Vec::new(),
            validity_range: Vec::new(),
            validity_domains: Vec::new(),
            confidence: None,
            dependencies: Vec::new(),
            artifacts: Vec::new(),
        },
    })?;
    Ok(EvaluatedCandidate {
        outcome: outcome(
            &candidate,
            &evidence,
            generation,
            false,
            1.0,
            BTreeMap::new(),
        ),
        candidate,
        evidence,
    })
}

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use crate::domain::content_identity::{validate_content_id, validate_safe_id};
use crate::domain::diagnostic::{AexError, AexResult};

use super::{EmbeddedStudyBaseline, StudyDocument, StudyVariableKind};

pub(crate) fn validate_study_document(document: &StudyDocument) -> AexResult<()> {
    if document.schema_version != 1 {
        return Err(AexError::validation(
            "UNSUPPORTED_SCHEMA_VERSION",
            "schema_version",
            format!(
                "study documents require schema version 1, got {}",
                document.schema_version
            ),
        ));
    }
    validate_identity(document)?;
    validate_baseline(document)?;
    validate_variables(document)?;
    validate_objectives_and_constraints(document)?;
    validate_policies(document)?;
    validate_selected_candidates(document)
}

fn validate_identity(document: &StudyDocument) -> AexResult<()> {
    validate_safe_id(&document.study.id, "study.id")?;
    if document.study.name.trim().is_empty() {
        return Err(AexError::validation(
            "INVALID_STUDY_NAME",
            "study.name",
            "study name cannot be empty",
        ));
    }
    if document.study.objectives.is_empty() {
        return Err(AexError::validation(
            "MISSING_STUDY_OBJECTIVE",
            "study.objectives",
            "at least one objective is required",
        ));
    }
    Ok(())
}

fn validate_baseline(document: &StudyDocument) -> AexResult<()> {
    match (
        &document.study.baseline.scenario_path,
        &document.study.baseline.embedded,
    ) {
        (Some(path), None) if !path.as_os_str().is_empty() && !path.is_absolute() => Ok(()),
        (None, Some(embedded)) => validate_embedded_baseline(embedded),
        (Some(path), None) if path.is_absolute() => Err(AexError::validation(
            "ABSOLUTE_STUDY_REFERENCE",
            "study.baseline.scenario_path",
            "shareable studies require a relative scenario path",
        )),
        _ => Err(AexError::validation(
            "INVALID_STUDY_BASELINE",
            "study.baseline",
            "provide exactly one non-empty scenario_path or embedded baseline",
        )),
    }
}

fn validate_embedded_baseline(embedded: &EmbeddedStudyBaseline) -> AexResult<()> {
    for (path, version) in [
        (
            "study.baseline.embedded.scenario.schema_version",
            embedded.scenario.schema_version,
        ),
        (
            "study.baseline.embedded.aircraft.schema_version",
            embedded.aircraft.schema_version,
        ),
        (
            "study.baseline.embedded.mission.schema_version",
            embedded.mission.schema_version,
        ),
        (
            "study.baseline.embedded.requirements.schema_version",
            embedded.requirements.schema_version,
        ),
    ] {
        if version != 1 {
            return Err(AexError::validation(
                "UNSUPPORTED_SCHEMA_VERSION",
                path,
                format!("embedded documents require schema version 1, got {version}"),
            ));
        }
    }
    if embedded
        .profiles
        .iter()
        .any(|profile| profile.schema_version != 1)
    {
        return Err(AexError::validation(
            "UNSUPPORTED_SCHEMA_VERSION",
            "study.baseline.embedded.profiles",
            "embedded profiles require schema version 1",
        ));
    }
    Ok(())
}

fn validate_variables(document: &StudyDocument) -> AexResult<()> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for variable in &document.study.variables {
        validate_safe_id(&variable.id, "study.variables[].id")?;
        if variable.path.trim().is_empty()
            || variable.values.is_empty()
            || variable.values.iter().any(|value| value.trim().is_empty())
            || !ids.insert(variable.id.as_str())
            || !paths.insert(variable.path.as_str())
        {
            return Err(AexError::validation(
                "INVALID_STUDY_VARIABLE",
                "study.variables",
                "variable ids and paths must be unique and values must be non-empty",
            ));
        }
        if matches!(variable.kind, StudyVariableKind::Integer)
            && variable
                .values
                .iter()
                .any(|value| value.parse::<i64>().is_err())
        {
            return Err(AexError::validation(
                "INVALID_INTEGER_STUDY_VALUE",
                format!("study.variables.{}.values", variable.id),
                "integer variables require base-10 integer values",
            ));
        }
    }
    if document.study.variables.iter().any(|variable| {
        variable
            .active_when
            .keys()
            .any(|dependency| !ids.contains(dependency.as_str()))
    }) {
        return Err(AexError::validation(
            "UNKNOWN_CONDITIONAL_VARIABLE",
            "study.variables[].active_when",
            "conditional variable dependencies must reference declared variable ids",
        ));
    }
    validate_derived_parameters(document)
}

fn validate_derived_parameters(document: &StudyDocument) -> AexResult<()> {
    let mut targets = BTreeSet::new();
    if document.study.derived_parameters.iter().any(|derived| {
        derived.target.trim().is_empty()
            || derived.method.trim().is_empty()
            || !targets.insert(derived.target.as_str())
    }) {
        return Err(AexError::validation(
            "INVALID_DERIVED_PARAMETER",
            "study.derived_parameters",
            "derived targets must be unique and target and method must be non-empty",
        ));
    }
    Ok(())
}

fn validate_objectives_and_constraints(document: &StudyDocument) -> AexResult<()> {
    let mut objective_ids = BTreeSet::new();
    for objective in &document.study.objectives {
        validate_safe_id(&objective.id, "study.objectives[].id")?;
        if objective.metric.trim().is_empty()
            || !objective.weight.is_finite()
            || objective.weight <= 0.0
            || !objective_ids.insert(objective.id.as_str())
        {
            return Err(AexError::validation(
                "INVALID_STUDY_OBJECTIVE",
                "study.objectives",
                "objective ids must be unique with a metric and positive finite weight",
            ));
        }
    }
    validate_constraints(document)
}

fn validate_constraints(document: &StudyDocument) -> AexResult<()> {
    let mut ids = BTreeSet::new();
    for constraint in &document.study.constraints {
        validate_safe_id(&constraint.id, "study.constraints[].id")?;
        let valid = !constraint.metric.trim().is_empty()
            && matches!(
                constraint.operator.as_str(),
                "ge" | "gt" | "le" | "lt" | "eq"
            )
            && !constraint.value.trim().is_empty()
            && matches!(constraint.severity.as_str(), "hard" | "soft")
            && constraint.weight.is_finite()
            && constraint.weight > 0.0
            && ids.insert(constraint.id.as_str());
        if !valid {
            return Err(AexError::validation(
                "INVALID_STUDY_CONSTRAINT",
                "study.constraints",
                "constraints require unique ids, metric, operator, value, severity, and weight",
            ));
        }
    }
    Ok(())
}

fn validate_policies(document: &StudyDocument) -> AexResult<()> {
    let analysis = &document.study.analysis;
    if analysis.screening_backend.trim().is_empty()
        || analysis
            .refinement_backend
            .as_ref()
            .is_some_and(|backend| backend.trim().is_empty())
        || analysis.refinement_candidate_limit == 0
    {
        return Err(AexError::validation(
            "INVALID_STUDY_ANALYSIS_POLICY",
            "study.analysis",
            "backend ids must be non-empty and refinement limit must be positive",
        ));
    }
    let search = &document.study.search;
    if !matches!(search.strategy.as_str(), "grid" | "evolutionary")
        || search.max_evaluations == 0
        || search.population == 0
        || search.generations == 0
        || !search.mutation_rate.is_finite()
        || !(0.0..=1.0).contains(&search.mutation_rate)
    {
        return Err(AexError::validation(
            "INVALID_STUDY_SEARCH_POLICY",
            "study.search",
            "use grid or evolutionary search with positive limits and mutation rate 0-1",
        ));
    }
    Ok(())
}

fn validate_selected_candidates(document: &StudyDocument) -> AexResult<()> {
    let mut candidate_ids = BTreeSet::new();
    for snapshot in &document.study.selected_candidates {
        snapshot.candidate.validate()?;
        if !candidate_ids.insert(snapshot.candidate.candidate_id.as_str()) {
            return Err(AexError::validation(
                "DUPLICATE_SELECTED_CANDIDATE",
                "study.selected_candidates",
                "selected candidate identifiers must be unique",
            ));
        }
        for evaluation_id in &snapshot.evaluation_ids {
            validate_content_id(
                evaluation_id,
                "eval_",
                "study.selected_candidates.evaluation_ids",
            )?;
        }
    }
    Ok(())
}

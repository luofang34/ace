use std::collections::BTreeMap;

use crate::domain::diagnostic::{Diagnostic, Severity};
use crate::domain::quantity::QuantityOutput;
use crate::domain::study::StudyDefinition;
use crate::domain::validity::{MetricValidity, ValidityStatus};

pub(super) fn objective_values(
    study: &StudyDefinition,
    metrics: &BTreeMap<String, QuantityOutput>,
    metric_validity: &BTreeMap<String, MetricValidity>,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, f64> {
    study
        .objectives
        .iter()
        .filter_map(|objective| {
            objective_metric_value(
                &objective.id,
                &objective.metric,
                metrics,
                metric_validity,
                diagnostics,
            )
            .map(|value| (objective.id.clone(), value))
        })
        .collect()
}

pub(super) fn objective_metric_value(
    objective_id: &str,
    metric_id: &str,
    metrics: &BTreeMap<String, QuantityOutput>,
    metric_validity: &BTreeMap<String, MetricValidity>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f64> {
    let validity = metric_validity.get(metric_id).cloned().unwrap_or_default();
    let unavailable = matches!(
        validity.status,
        ValidityStatus::BoundaryLimited | ValidityStatus::Unsupported
    );
    if let Some(metric) = metrics.get(metric_id).filter(|_| !unavailable) {
        return Some(metric.value);
    }
    diagnostics.push(Diagnostic {
        code: "STUDY_OBJECTIVE_UNAVAILABLE".to_owned(),
        severity: Severity::Error,
        message: format!(
            "objective {objective_id} requires available metric {metric_id}; validity is {}",
            validity.status.wire_name()
        ),
        path: Some(format!("study.objectives.{objective_id}")),
        context: serde_json::Value::Null,
    });
    None
}

use std::collections::BTreeMap;

use crate::domain::quantity::QuantityOutput;
use crate::domain::validity::{MetricValidity, ModelValidityDomain};

pub(super) fn recorded_metrics(
    metrics: &BTreeMap<String, QuantityOutput>,
    validity: &BTreeMap<String, MetricValidity>,
) -> BTreeMap<String, MetricValidity> {
    validity
        .iter()
        .filter(|(metric, _)| metrics.contains_key(*metric))
        .map(|(metric, item)| (metric.clone(), item.clone()))
        .collect()
}

pub(super) fn combined_domains(
    first: &[ModelValidityDomain],
    second: &[ModelValidityDomain],
) -> Vec<ModelValidityDomain> {
    let mut domains = Vec::new();
    for domain in first.iter().chain(second) {
        if !domains
            .iter()
            .any(|existing: &ModelValidityDomain| existing.model_id == domain.model_id)
        {
            domains.push(domain.clone());
        }
    }
    domains
}

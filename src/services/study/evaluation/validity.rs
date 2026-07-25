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
        if !domains.iter().any(|existing: &ModelValidityDomain| {
            existing.model_id == domain.model_id
                && existing.applicability_path == domain.applicability_path
        }) {
            domains.push(domain.clone());
        }
    }
    domains
}

#[cfg(test)]
mod tests {
    use crate::domain::aerodynamics::{PolarTable, TABLE_POLAR_MODEL_ID};
    use crate::models::validity::scenario_domains;
    use crate::test_support::example_scenario;

    use super::combined_domains;

    #[test]
    fn combination_preserves_same_model_with_distinct_applicability()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut scenario = example_scenario("c172")?;
        for configuration in [
            &mut scenario.aircraft.aerodynamics.clean,
            &mut scenario.aircraft.aerodynamics.takeoff,
            &mut scenario.aircraft.aerodynamics.landing,
        ] {
            configuration.polar_table = Some(PolarTable {
                mach: vec![0.0, 0.5],
                cd0: vec![configuration.cd0; 2],
                cl_max: vec![configuration.cl_max; 2],
                oswald_efficiency: Some(vec![configuration.oswald_efficiency; 2]),
                induced_drag_factor: None,
            });
        }
        let domains = scenario_domains(&scenario)?;
        let combined = combined_domains(&domains[..2], &domains[2..]);
        assert_eq!(
            combined
                .iter()
                .filter(|domain| domain.model_id == TABLE_POLAR_MODEL_ID)
                .count(),
            3
        );
        Ok(())
    }
}

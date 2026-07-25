use serde_json::json;

use crate::models::atmosphere::Isa1976;
use crate::test_support::example_scenario;

use super::{RegisteredModel, ValidityDomainProvider, ValidityVariable, scenario_domains};

#[test]
fn every_registered_model_has_a_valid_nonempty_domain() -> Result<(), Box<dyn std::error::Error>> {
    let mut ids = Vec::new();
    for model in RegisteredModel::ALL {
        let domain = model.validity_domain();
        domain.validate()?;
        assert!(!domain.bounds.is_empty());
        ids.push(domain.model_id);
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), RegisteredModel::ALL.len());
    Ok(())
}

#[test]
fn resolved_propulsion_domains_use_profile_limits() -> Result<(), Box<dyn std::error::Error>> {
    let c172 = example_scenario("c172")?;
    let b777 = example_scenario("b777")?;
    let c172_domain = c172.engine.validity_domain();
    let b777_domain = b777.engine.validity_domain();

    assert!(
        c172_domain
            .bounds
            .iter()
            .find(|bound| bound.variable == ValidityVariable::Altitude)
            .and_then(|bound| bound.maximum)
            .is_some_and(|maximum| (maximum - 5_486.4).abs() < 1.0e-9)
    );
    assert_eq!(
        b777_domain
            .bounds
            .iter()
            .find(|bound| bound.variable == ValidityVariable::Mach)
            .and_then(|bound| bound.maximum),
        Some(0.9)
    );
    for domain in scenario_domains(&b777)? {
        domain.validate()?;
    }
    Ok(())
}

#[test]
fn validity_domain_serialization_is_stable_and_round_trips()
-> Result<(), Box<dyn std::error::Error>> {
    let domain = Isa1976::new(0.0).validity_domain();
    let value = serde_json::to_value(&domain)?;
    assert_eq!(
        value,
        json!({
            "model_id": "atmosphere.isa1976",
            "bounds": [{
                "variable": "altitude",
                "minimum": -2000.0,
                "maximum": 20000.0,
                "minimum_inclusive": true,
                "maximum_inclusive": true,
                "unit": "m",
                "basis": "published_specification"
            }]
        })
    );
    let decoded = serde_json::from_value(value)?;
    assert_eq!(domain, decoded);
    Ok(())
}

use serde_json::json;

use crate::domain::aerodynamics::{PolarTable, TABLE_POLAR_MODEL_ID};
use crate::domain::validity::ValidityBasis;
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
fn generic_turbofan_domain_declares_profile_resolved_mach() -> Result<(), Box<dyn std::error::Error>>
{
    let generic = RegisteredModel::TurbofanPropulsion.validity_domain();
    let mach = generic
        .bounds
        .iter()
        .find(|bound| bound.variable == ValidityVariable::Mach)
        .ok_or("generic turbofan Mach bound is missing")?;
    assert_eq!(mach.maximum, None);
    assert_eq!(mach.basis, ValidityBasis::ResolvedProfile);

    let b777 = example_scenario("b777")?;
    let resolved = b777.engine.validity_domain();
    let resolved_mach = resolved
        .bounds
        .iter()
        .find(|bound| bound.variable == ValidityVariable::Mach)
        .ok_or("resolved turbofan Mach bound is missing")?;
    assert_eq!(resolved_mach.maximum, Some(0.9));
    assert_eq!(resolved_mach.minimum, Some(0.0));
    assert_eq!(resolved_mach.basis, ValidityBasis::TabulatedData);
    Ok(())
}

#[test]
fn resolved_polar_table_domain_uses_exact_mach_support() -> Result<(), Box<dyn std::error::Error>> {
    let sr71 = example_scenario("sr71")?;
    let domain = scenario_domains(&sr71)?
        .into_iter()
        .find(|domain| domain.model_id == TABLE_POLAR_MODEL_ID)
        .ok_or("SR-71 table-polar domain is missing")?;
    let mach = domain
        .bounds
        .iter()
        .find(|bound| bound.variable == ValidityVariable::Mach)
        .ok_or("table-polar Mach bound is missing")?;

    assert_eq!(mach.minimum, Some(0.0));
    assert_eq!(mach.maximum, Some(3.3));
    assert_eq!(mach.basis, ValidityBasis::TabulatedData);
    assert_eq!(
        domain.applicability_path.as_deref(),
        Some("aircraft.aerodynamics.clean")
    );
    let decoded = serde_json::from_value(serde_json::to_value(&domain)?)?;
    assert_eq!(domain, decoded);
    Ok(())
}

#[test]
fn every_configuration_table_publishes_its_own_domain() -> Result<(), Box<dyn std::error::Error>> {
    let mut scenario = example_scenario("c172")?;
    for (configuration, maximum) in [
        (&mut scenario.aircraft.aerodynamics.clean, 0.8),
        (&mut scenario.aircraft.aerodynamics.takeoff, 0.4),
        (&mut scenario.aircraft.aerodynamics.landing, 0.3),
    ] {
        configuration.polar_table = Some(PolarTable {
            mach: vec![0.0, maximum],
            cd0: vec![configuration.cd0; 2],
            cl_max: vec![configuration.cl_max; 2],
            oswald_efficiency: Some(vec![configuration.oswald_efficiency; 2]),
            induced_drag_factor: None,
        });
    }
    let domains = scenario_domains(&scenario)?
        .into_iter()
        .filter(|domain| domain.model_id == TABLE_POLAR_MODEL_ID)
        .collect::<Vec<_>>();

    assert_eq!(domains.len(), 3);
    for (domain, (path, maximum)) in domains.iter().zip([
        ("aircraft.aerodynamics.clean", 0.8),
        ("aircraft.aerodynamics.takeoff", 0.4),
        ("aircraft.aerodynamics.landing", 0.3),
    ]) {
        assert_eq!(domain.applicability_path.as_deref(), Some(path));
        assert_eq!(
            domain
                .bounds
                .iter()
                .find(|bound| bound.variable == ValidityVariable::Mach)
                .and_then(|bound| bound.maximum),
            Some(maximum)
        );
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

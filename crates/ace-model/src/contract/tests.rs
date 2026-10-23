use super::*;
use std::collections::BTreeMap;

#[test]
fn semantics_require_matching_meaning_and_reference_not_just_units() {
    let pressure = ScalarPort::semantic(Unit::Pascal, "pressure/1", "absolute");
    let stress = ScalarPort::semantic(Unit::Pascal, "stress/1", "normal");
    assert!(!pressure.accepts(&stress));
    assert!(!pressure.accepts(&Unit::Pascal.into()));
    assert!(pressure.accepts(&ScalarPort::semantic(
        Unit::Megapascal,
        "pressure/1",
        "absolute"
    )));
    assert!(
        !ScalarPort::semantic(Unit::Ratio, "cl/1", "wing-area").accepts(&ScalarPort::semantic(
            Unit::Ratio,
            "cl/1",
            "wetted-area"
        ))
    );
}

#[test]
fn domain_checks_converted_units_closed_endpoints_and_missing_state() {
    let variable = DomainVariable::Condition("altitude".into());
    let domain = ValidityDomain {
        bounds: vec![DomainBound::closed(
            variable.clone(),
            Unit::Meter,
            0.0,
            3048.0,
        )],
        assumptions: vec!["geometric altitude in the declared atmosphere".into()],
    };
    let inputs = BTreeMap::new();
    for altitude in [0.0, 10000.0] {
        let state = BTreeMap::from([(
            "altitude".into(),
            Quantity::new(altitude, Unit::Foot).unwrap(),
        )]);
        domain.check(&inputs, Some(&state)).unwrap();
    }
    let state = BTreeMap::from([(
        "altitude".into(),
        Quantity::new(10001.0, Unit::Foot).unwrap(),
    )]);
    assert!(matches!(
        domain.check(&inputs, Some(&state)),
        Err(ContractError::Outside { .. })
    ));
    assert!(matches!(
        domain.check(&inputs, None),
        Err(ContractError::Missing { .. })
    ));
    let roundtrip: ValidityDomain =
        serde_json::from_slice(&serde_json::to_vec(&domain).unwrap()).unwrap();
    assert_eq!(roundtrip, domain);
}

#[test]
fn malformed_nonfinite_and_open_endpoint_domains_fail() {
    let bound = DomainBound::positive(DomainVariable::Input("power".into()), Unit::Watt);
    let mut domain = ValidityDomain {
        bounds: vec![bound.clone()],
        assumptions: vec![],
    };
    let inputs = BTreeMap::from([("power".into(), Quantity::new(0.0, Unit::Watt).unwrap())]);
    assert!(matches!(
        domain.check(&inputs, None),
        Err(ContractError::Outside { .. })
    ));
    domain.bounds.push(bound);
    assert!(domain.validate().is_err());
    domain.bounds.pop();
    domain.bounds[0].lower.as_mut().unwrap().value = f64::NAN;
    assert!(domain.validate().is_err());
    domain.bounds[0] =
        DomainBound::closed(DomainVariable::Input("power".into()), Unit::Watt, 2.0, 1.0);
    assert!(domain.validate().is_err());
}

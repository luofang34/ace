//! Public library clients execute frozen reference cases and bounded variant matrices.
#![cfg(test)]

use ace_engine::{Registry, Session};
use ace_model::{Quantity, Unit, VehicleModel};
use ace_study::{ReferenceValue, VariantMatrix, VerificationCase, VerificationRecord};
use std::error::Error;

fn registry() -> Result<Registry, Box<dyn Error>> {
    let mut registry = Registry::default();
    ace_physics::register(&mut registry)?;
    Ok(registry)
}

fn baseline() -> Result<Session, Box<dyn Error>> {
    let model: VehicleModel =
        serde_json::from_str(include_str!("../examples/synthesis/constant-rate.json"))?;
    Ok(Session::new(model, registry()?)?)
}

fn case() -> Result<VerificationCase, Box<dyn Error>> {
    Ok(serde_json::from_str(include_str!(
        "../examples/synthesis/constant-rate-verification.json"
    ))?)
}

fn expected(value: f64, unit: Unit) -> Result<ReferenceValue, Box<dyn Error>> {
    Ok(ReferenceValue {
        value: Quantity::new(value, unit)?,
        tolerance: ace_model::Tolerance {
            absolute: Quantity::new(1e-8, unit)?,
            relative: 1e-12,
        },
    })
}

#[test]
fn reference_case_reopens_replays_and_detects_contract_and_reference_tampering()
-> Result<(), Box<dyn Error>> {
    let baseline = baseline()?;
    let record = case()?.run(&baseline)?;
    assert!(record.passed());
    assert!(baseline.snapshot().runs.is_empty());
    assert!(
        record
            .snapshot
            .runs
            .iter()
            .all(|run| run.contract.is_some())
    );
    let reopened: VerificationRecord = serde_json::from_slice(&serde_json::to_vec(&record)?)?;
    assert_eq!(reopened.replay(registry()?)?.id, record.id);
    let sheet = ace_sheet::project(&reopened.model, &reopened.snapshot)?;
    assert_eq!(sheet.snapshot.model_digest, record.snapshot.model_digest);
    let mut tampered = reopened.clone();
    tampered
        .case
        .expected
        .get_mut("burn")
        .ok_or("missing burn")?
        .value
        .value = 99.0;
    assert!(tampered.verify_integrity().is_err());
    assert!(!tampered.passed());
    let mut tampered = reopened;
    tampered
        .contracts
        .get_mut("phase")
        .ok_or("missing phase")?
        .domain
        .assumptions
        .push("changed".into());
    assert!(tampered.verify_integrity().is_err());
    assert!(!tampered.passed());
    assert!(record.replay(Registry::default()).is_err());
    Ok(())
}

#[test]
fn every_small_matrix_variant_executes_against_independent_integrals() -> Result<(), Box<dyn Error>>
{
    let baseline = baseline()?;
    let matrix: VariantMatrix = serde_json::from_str(include_str!(
        "../examples/synthesis/constant-rate-variants.json"
    ))?;
    let variants = matrix.expand()?;
    assert_eq!(variants.len(), 8);
    let mut identities = std::collections::BTreeSet::new();
    for variant in variants {
        let mut case = case()?;
        let updates = &variant.configuration.updates;
        let time = updates["duration"].to(Unit::Second)?.value;
        let speed = updates["speed"].to(Unit::MeterPerSecond)?.value;
        let flow = updates["fuel_flow"].to(Unit::KilogramPerSecond)?.value;
        case.expected
            .insert("elapsed".into(), expected(time, Unit::Second)?);
        case.expected
            .insert("distance".into(), expected(time * speed, Unit::Meter)?);
        case.expected
            .insert("burn".into(), expected(time * flow, Unit::Kilogram)?);
        case.expected
            .insert("cost".into(), expected(time * 0.2, Unit::Dollar)?);
        case.name = variant.id;
        case.reference.citation = "Analytic integrals of prescribed constant rates: distance=v*t, fuel=flow*t, cost=0.2*t.".into();
        case.configuration = variant.configuration;
        let record = case.run(&baseline)?;
        assert!(record.passed(), "{}", case.name);
        assert!(identities.insert(record.id.clone()));
        assert_eq!(record.replay(registry()?)?.id, record.id);
    }
    assert_eq!(baseline.model().revision, 0);
    Ok(())
}

#[test]
fn bad_references_failed_domains_and_missing_solvers_never_pass() -> Result<(), Box<dyn Error>> {
    let mut case = case()?;
    case.expected
        .get_mut("burn")
        .ok_or("missing burn")?
        .value
        .value = 4.0;
    assert!(!case.run(&baseline()?)?.passed());
    case.configuration
        .updates
        .insert("duration".into(), Quantity::new(-1.0, Unit::Second)?);
    let failed = case.run(&baseline()?)?;
    assert!(!failed.passed());
    assert!(failed.checks.values().all(|check| check.actual.is_none()));
    let missing = Session::new(baseline()?.model().clone(), Registry::default())?;
    assert!(!case.run(&missing)?.passed());
    Ok(())
}

#[test]
fn compatible_input_units_preserve_reference_results() -> Result<(), Box<dyn Error>> {
    let mut case = case()?;
    case.configuration
        .updates
        .insert("duration".into(), Quantity::new(1.0 / 60.0, Unit::Hour)?);
    assert!(case.run(&baseline()?)?.passed());
    Ok(())
}

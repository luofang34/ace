use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::services::analysis::ApplicationService;

use super::RefinementSpec;

#[test]
fn c172_refinement_returns_a_screened_design() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let result = service.auto_refine_design_blocking(RefinementSpec {
        scenario_path: &scenario,
        output_design_id: "refined_c172",
        display_name: "Refined C172",
        design_root: temporary.path(),
        backend: "native",
        artifact_path: None,
        max_iterations: 12,
    })?;
    assert!(result.converged);
    assert!(result.evaluation.failed_constraints.is_empty());
    assert!(result.backend_verification_passed);
    let structural = result
        .evaluation
        .baseline
        .analysis
        .structural_screen
        .as_ref()
        .ok_or_else(|| std::io::Error::other("missing structural screen"))?;
    assert!(structural.passed);
    let power = result
        .evaluation
        .baseline
        .analysis
        .mission_power_screen
        .as_ref()
        .ok_or_else(|| std::io::Error::other("missing mission power screen"))?;
    assert!(power.passed);
    assert!(power.minimum_reserve_margin.value >= 0.0);
    assert!(result.evaluated_candidates > 1);
    Ok(())
}

#[test]
fn refinement_preserves_source_design_overrides() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let baseline = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let source = service.create_design_blocking(
        "source_design",
        "Source Design",
        temporary.path(),
        None,
        Some(&baseline),
        &BTreeMap::from([(
            "aircraft.name".to_owned(),
            "Override-Preserving Aircraft".to_owned(),
        )]),
    )?;
    let result = service.auto_refine_design_blocking(RefinementSpec {
        scenario_path: &source.scenario_path,
        output_design_id: "refined_design",
        display_name: "Refined Design",
        design_root: temporary.path(),
        backend: "native",
        artifact_path: None,
        max_iterations: 2,
    })?;
    let resolved = service.resolve_blocking(&result.design.scenario_path, &BTreeMap::new())?;
    assert_eq!(resolved.aircraft.name, "Override-Preserving Aircraft");
    Ok(())
}

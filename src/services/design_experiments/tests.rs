use std::io;

use crate::services::analysis::ApplicationService;

#[test]
fn installed_openvsp_refines_without_replacing_native_results()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let openvsp_available = service
        .list_analysis_backends()
        .into_iter()
        .find(|backend| backend.id == "openvsp")
        .is_some_and(|backend| backend.available);
    if !openvsp_available {
        return Ok(());
    }
    let scenario =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172/scenario.yaml");
    let artifact = temporary.path().join("c172.vsp3");
    let result = service.evaluate_feasibility_blocking(&scenario, "openvsp", Some(&artifact))?;
    let refinement = result
        .refinement
        .ok_or_else(|| io::Error::other("missing OpenVSP refinement"))?;
    assert_eq!(result.baseline.analysis.provenance.backend, "native");
    assert_eq!(refinement.analysis.provenance.backend, "openvsp");
    assert!(!refinement.analysis.polar.is_empty());
    assert!(refinement.geometry.metrics.wetted_area.value > 30.0);
    assert!(refinement.geometry.metrics.wetted_area.value < 80.0);
    assert!(artifact.is_file());
    Ok(())
}

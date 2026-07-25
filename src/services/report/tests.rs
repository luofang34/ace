use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::services::analysis::ApplicationService;

fn unsupported_report_fixture(destination: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/c172");
    fs::create_dir_all(destination.join("profiles"))?;
    for path in [
        "aircraft.yaml",
        "mission.yaml",
        "requirements.yaml",
        "scenario.yaml",
        "profiles/engine.yaml",
        "profiles/propeller.yaml",
    ] {
        fs::copy(source.join(path), destination.join(path))?;
    }
    remove_horizontal_tail(&destination.join("aircraft.yaml"))?;
    remove_cruise_segment(&destination.join("mission.yaml"))?;
    Ok(destination.join("scenario.yaml"))
}

fn remove_horizontal_tail(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    let components = document["aircraft"]["topology"]["components"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology components"))?;
    components.retain(|component| component["kind"].as_str() != Some("horizontal_tail"));
    let relationships = document["aircraft"]["topology"]["relationships"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing topology relationships"))?;
    relationships.retain(|relationship| {
        relationship["source"].as_str() != Some("horizontal_tail")
            && relationship["target"].as_str() != Some("horizontal_tail")
    });
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

fn remove_cruise_segment(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut document: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    let segments = document["mission"]["segments"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing mission segments"))?;
    segments.retain(|segment| segment["type"].as_str() != Some("cruise"));
    fs::write(path, serde_yaml::to_string(&document)?)?;
    Ok(())
}

#[test]
fn report_stops_at_topology_preflight_before_downstream_analysis()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let scenario = unsupported_report_fixture(&temporary.path().join("source"))?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));

    let downstream = service.payload_range_blocking(&scenario, &Default::default());
    assert!(downstream.is_err_and(|error| error.to_string().contains("NO_CRUISE_SEGMENT")));
    let result = service.concept_report_blocking(&scenario, "native");

    assert!(result.is_err_and(|error| error.to_string().contains("UNSUPPORTED_BACKEND_TOPOLOGY")));
    Ok(())
}

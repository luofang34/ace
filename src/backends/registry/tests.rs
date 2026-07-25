use std::ffi::OsString;
use std::path::Path;

use crate::backends::openvsp::openvsp_descriptor;

use super::{executable_path, probe_version_blocking};

#[cfg(unix)]
#[test]
fn regular_data_file_is_not_an_executable_backend() {
    assert!(executable_path(Path::new("Cargo.toml")).is_none());
}

#[test]
fn executable_without_openvsp_identity_fails_the_probe() -> Result<(), Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let search_path = OsString::from("/usr/bin:/bin");
    assert!(probe_version_blocking(&executable, &search_path).is_err());
    Ok(())
}

#[test]
fn openvsp_vocabulary_is_independent_of_availability() {
    let available = openvsp_descriptor(true, Some("test".to_owned()), None);
    let unavailable = openvsp_descriptor(false, None, Some("missing".to_owned()));
    assert_eq!(available.capabilities, unavailable.capabilities);
    assert_eq!(available.disciplines, unavailable.disciplines);
    assert_eq!(available.fidelity_levels, unavailable.fidelity_levels);
    assert_eq!(
        available.topology.component_kinds,
        unavailable.topology.component_kinds
    );
    assert_eq!(
        available.topology.relationship_kinds,
        unavailable.topology.relationship_kinds
    );
}

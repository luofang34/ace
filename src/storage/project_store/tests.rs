use std::fs;

use crate::domain::diagnostic::AexError;
use crate::domain::presentation::DisplayUnitSystem;

use super::display_unit_system_blocking;

#[test]
fn missing_project_uses_si_and_explicit_units_take_precedence()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let scenario = directory.path().join("scenario.yaml");

    assert_eq!(
        display_unit_system_blocking(&scenario, None)?,
        DisplayUnitSystem::Si
    );
    fs::write(
        directory.path().join("aircraft-explorer.yaml"),
        "project:\n  default_unit_system: aviation_us\n",
    )?;
    assert_eq!(
        display_unit_system_blocking(&scenario, None)?,
        DisplayUnitSystem::AviationUs
    );
    assert_eq!(
        display_unit_system_blocking(&scenario, Some("si"))?,
        DisplayUnitSystem::Si
    );
    Ok(())
}

#[test]
fn unknown_project_unit_system_has_a_stable_error() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let scenario = directory.path().join("scenario.yaml");
    fs::write(
        directory.path().join("aircraft-explorer.yaml"),
        "project:\n  default_unit_system: nautical_furlongs\n",
    )?;

    let result = display_unit_system_blocking(&scenario, None);
    assert!(matches!(
        result,
        Err(AexError::Validation {
            code: "UNSUPPORTED_UNIT_SYSTEM",
            path,
            ..
        }) if path == "project.default_unit_system"
    ));
    Ok(())
}

use std::fs;
use std::path::PathBuf;

use serde::de::DeserializeOwned;

use crate::domain::schema::{
    AircraftDocument, MissionDocument, ProfileDocument, RequirementsDocument, ScenarioDocument,
};
use crate::domain::study::EmbeddedStudyBaseline;

use super::resolve_embedded_study;

fn read_c172<T: DeserializeOwned>(relative: &str) -> Result<T, Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/c172")
        .join(relative);
    Ok(serde_yaml::from_str(&fs::read_to_string(path)?)?)
}

fn embedded_c172() -> Result<EmbeddedStudyBaseline, Box<dyn std::error::Error>> {
    Ok(EmbeddedStudyBaseline {
        scenario: read_c172::<ScenarioDocument>("scenario.yaml")?,
        aircraft: read_c172::<AircraftDocument>("aircraft.yaml")?,
        mission: read_c172::<MissionDocument>("mission.yaml")?,
        requirements: read_c172::<RequirementsDocument>("requirements.yaml")?,
        profiles: vec![
            read_c172::<ProfileDocument>("profiles/engine.yaml")?,
            read_c172::<ProfileDocument>("profiles/propeller.yaml")?,
        ],
    })
}

#[test]
fn embedded_study_finalizes_engine_mass_from_profile() -> Result<(), Box<dyn std::error::Error>> {
    let scenario = resolve_embedded_study(&embedded_c172()?)?;
    let powerplant = scenario
        .aircraft
        .mass
        .statement
        .components
        .iter()
        .find(|component| component.component_id == "powerplant")
        .ok_or_else(|| std::io::Error::other("missing powerplant mass component"))?;

    assert_eq!(powerplant.mass.value, 135.0);
    assert_eq!(powerplant.mass.provenance.kind, "profile");
    assert_eq!(
        powerplant.mass.provenance.source,
        "engine profile engine.lycoming_io360_class"
    );
    Ok(())
}

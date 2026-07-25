use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn copy_c172_scenario(destination: &Path) -> Result<PathBuf, Box<dyn Error>> {
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
    Ok(destination.join("scenario.yaml"))
}

pub(super) fn fuel_exhaustion_scenario(destination: &Path) -> Result<String, Box<dyn Error>> {
    let scenario_path = copy_c172_scenario(destination)?;
    let aircraft_path = destination.join("aircraft.yaml");
    let mut aircraft: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&aircraft_path)?)?;
    aircraft["aircraft"]["mass"]["maximum_fuel_mass"] =
        serde_yaml::Value::String("1 kg".to_owned());
    fs::write(aircraft_path, serde_yaml::to_string(&aircraft)?)?;
    Ok(scenario_path.display().to_string())
}

pub(super) fn no_cruise_scenario(destination: &Path) -> Result<String, Box<dyn Error>> {
    let scenario_path = copy_c172_scenario(destination)?;
    let mission_path = destination.join("mission.yaml");
    let mut mission: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&mission_path)?)?;
    let segments = mission["mission"]["segments"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing mission segments"))?;
    segments.retain(|segment| segment["type"].as_str() != Some("cruise"));
    fs::write(mission_path, serde_yaml::to_string(&mission)?)?;
    let requirements_path = destination.join("requirements.yaml");
    let mut requirements: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&requirements_path)?)?;
    let items = requirements["requirements"]["items"]
        .as_sequence_mut()
        .ok_or_else(|| io::Error::other("missing requirement items"))?;
    let range = items
        .iter_mut()
        .find(|item| item["id"].as_str() == Some("range"))
        .ok_or_else(|| io::Error::other("missing range requirement"))?;
    range["value"] = serde_yaml::Value::String("0 nmi".to_owned());
    let cruise_speed = items
        .iter_mut()
        .find(|item| item["id"].as_str() == Some("cruise_speed"))
        .ok_or_else(|| io::Error::other("missing cruise speed requirement"))?;
    cruise_speed["value"] = serde_yaml::Value::String("90 kt".to_owned());
    items.push(serde_yaml::from_str(
        r#"
id: advisory_payload_range
metric: performance.full_payload_range
operator: ge
value: 0 nmi
severity: soft
"#,
    )?);
    fs::write(requirements_path, serde_yaml::to_string(&requirements)?)?;
    Ok(scenario_path.display().to_string())
}

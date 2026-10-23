use crate::domain::diagnostic::{AexError, AexResult};
use ace_model::Quantity;
use ace_study::{
    CalibrationCase, CalibrationRecord, Campaign, CampaignRecord, CapabilityMap, Demand,
    Environment, Fleet, UncertaintyPlan, UncertaintyRecord, VehicleCapability,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Campaign file whose fleets reference vehicle models instead of embedding capabilities.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CampaignDocument {
    schema_version: u32,
    name: String,
    demand: Demand,
    environment: Environment,
    fleets: BTreeMap<String, FleetDocument>,
    daily_success_fraction: f64,
    required_success_rate: f64,
    replications: u32,
    seed: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FleetDocument {
    /// Vehicle model path relative to the campaign file.
    vehicle: PathBuf,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
    /// Overrides the vehicle's declared capability map.
    #[serde(default)]
    capabilities: Option<CapabilityMap>,
    initial_units: u32,
    replacement_lead_time: Quantity,
    max_replacements: u32,
    operators_per_station: f64,
    operator_cost_per_hour: Quantity,
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> AexResult<T> {
    let content = std::fs::read_to_string(path).map_err(|source| AexError::Read {
        path: path.into(),
        source,
    })?;
    serde_yaml::from_str(&content).map_err(|source| AexError::Yaml {
        path: path.into(),
        source,
    })
}

/// Propagate the vehicle's declared input uncertainty under a sampling plan.
pub(crate) fn uncertainty_blocking(
    vehicle: &Path,
    plan: &Path,
    overrides: &BTreeMap<String, String>,
) -> AexResult<UncertaintyRecord> {
    let plan: UncertaintyPlan = read(plan)?;
    let session = super::synthesis::open_blocking(vehicle, overrides)?;
    plan.run(&session).map_err(AexError::from)
}

/// Fit the vehicle's calibrated inputs to a measured case.
pub(crate) fn calibrate_blocking(
    vehicle: &Path,
    case: &Path,
    overrides: &BTreeMap<String, String>,
) -> AexResult<CalibrationRecord> {
    let case: CalibrationCase = read(case)?;
    let session = super::synthesis::open_blocking(vehicle, overrides)?;
    case.run(&session).map_err(AexError::from)
}

/// Synthesize every fleet's vehicle, read its capability record and run the campaign.
pub(crate) fn campaign_blocking(path: &Path) -> AexResult<CampaignRecord> {
    let document: CampaignDocument = read(path)?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut fleets = BTreeMap::new();
    for (id, fleet) in document.fleets {
        if fleet.vehicle.is_absolute() {
            return Err(AexError::validation(
                "ABSOLUTE_VEHICLE_PATH",
                format!("fleets.{id}.vehicle"),
                "vehicle paths are relative to the campaign file so campaigns stay portable",
            ));
        }
        let mut session =
            super::synthesis::open_blocking(&base.join(&fleet.vehicle), &fleet.overrides)?;
        let snapshot = session.run()?;
        let capability = match &fleet.capabilities {
            Some(map) => VehicleCapability::from_snapshot(session.model(), &snapshot, map)?,
            None => VehicleCapability::from_declared(session.model(), &snapshot)?,
        };
        fleets.insert(
            id,
            Fleet {
                capability,
                initial_units: fleet.initial_units,
                replacement_lead_time: fleet.replacement_lead_time,
                max_replacements: fleet.max_replacements,
                operators_per_station: fleet.operators_per_station,
                operator_cost_per_hour: fleet.operator_cost_per_hour,
            },
        );
    }
    Campaign {
        schema_version: document.schema_version,
        name: document.name,
        demand: document.demand,
        environment: document.environment,
        fleets,
        daily_success_fraction: document.daily_success_fraction,
        required_success_rate: document.required_success_rate,
        replications: document.replications,
        seed: document.seed,
    }
    .run()
    .map_err(AexError::from)
}

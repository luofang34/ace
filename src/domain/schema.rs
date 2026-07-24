use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::domain::diagnostic::Diagnostic;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct AircraftDocument {
    pub(crate) schema_version: u32,
    pub(crate) aircraft: RawAircraft,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawAircraft {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) category: String,
    pub(crate) configuration: String,
    pub(crate) propulsion_architecture: String,
    pub(crate) metadata: ConceptMetadata,
    pub(crate) mass: RawMass,
    pub(crate) geometry: RawGeometry,
    pub(crate) aerodynamics: RawAerodynamics,
    pub(crate) propulsion: RawPropulsion,
    pub(crate) limits: RawLimits,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ConceptMetadata {
    pub(crate) purpose: String,
    pub(crate) certification_use: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawMass {
    pub(crate) maximum_takeoff_mass: String,
    pub(crate) operating_empty_mass: String,
    pub(crate) maximum_payload_mass: String,
    pub(crate) maximum_fuel_mass: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawGeometry {
    pub(crate) wing: RawWing,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawWing {
    pub(crate) area: String,
    pub(crate) span: String,
    pub(crate) aspect_ratio: f64,
    pub(crate) sweep_quarter_chord: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawAerodynamics {
    pub(crate) model: String,
    pub(crate) clean: RawAeroConfiguration,
    pub(crate) takeoff: RawAeroConfiguration,
    pub(crate) landing: RawAeroConfiguration,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawAeroConfiguration {
    pub(crate) cd0: f64,
    pub(crate) oswald_efficiency: f64,
    pub(crate) cl_max: f64,
    #[serde(default)]
    pub(crate) additional_cd: f64,
    pub(crate) mach_critical: Option<f64>,
    pub(crate) wave_drag: Option<WaveDrag>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct WaveDrag {
    pub(crate) coefficient: f64,
    pub(crate) exponent: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawPropulsion {
    pub(crate) profile: String,
    pub(crate) engine_count: u32,
    pub(crate) propeller_profile: Option<String>,
    #[serde(default = "default_propulsion_sizing_factor")]
    pub(crate) sizing_factor: f64,
}

fn default_propulsion_sizing_factor() -> f64 {
    1.0
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(crate) struct RawLimits {
    pub(crate) maximum_operating_speed: Option<String>,
    pub(crate) maximum_operating_mach: Option<f64>,
    pub(crate) maximum_operating_altitude: Option<String>,
    pub(crate) maximum_load_factor: Option<f64>,
    pub(crate) minimum_load_factor: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct MissionDocument {
    pub(crate) schema_version: u32,
    pub(crate) mission: RawMission,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawMission {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) payload: RawPayload,
    pub(crate) segments: Vec<RawMissionSegment>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawPayload {
    pub(crate) mass: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawMissionSegment {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) duration: Option<String>,
    pub(crate) distance: Option<String>,
    pub(crate) target_altitude: Option<String>,
    pub(crate) altitude: Option<String>,
    pub(crate) indicated_airspeed: Option<String>,
    pub(crate) true_airspeed: Option<String>,
    pub(crate) mach: Option<f64>,
    pub(crate) power_fraction: Option<f64>,
    pub(crate) thrust_fraction: Option<f64>,
    pub(crate) fuel_fraction: Option<f64>,
    pub(crate) fuel_mass: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RequirementsDocument {
    pub(crate) schema_version: u32,
    pub(crate) requirements: RawRequirements,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawRequirements {
    pub(crate) id: String,
    pub(crate) items: Vec<RawRequirement>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawRequirement {
    pub(crate) id: String,
    pub(crate) metric: String,
    pub(crate) operator: String,
    pub(crate) value: Value,
    pub(crate) severity: String,
    pub(crate) weight: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ScenarioDocument {
    pub(crate) schema_version: u32,
    pub(crate) scenario: RawScenario,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawScenario {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) aircraft: PathBuf,
    pub(crate) mission: PathBuf,
    pub(crate) requirements: PathBuf,
    #[serde(default)]
    pub(crate) overrides: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ProfileDocument {
    pub(crate) schema_version: u32,
    pub(crate) profile: RawProfile,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawProfile {
    pub(crate) id: String,
    pub(crate) version: u32,
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) model: String,
    pub(crate) metadata: Option<ProfileMetadata>,
    pub(crate) parameters: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ProfileMetadata {
    pub(crate) display_name: String,
    pub(crate) source: String,
    pub(crate) confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AssumptionEntry {
    pub(crate) parameter_path: String,
    pub(crate) resolved_value: serde_json::Value,
    pub(crate) unit: Option<String>,
    pub(crate) provenance_kind: String,
    pub(crate) source: String,
    pub(crate) confidence: String,
    pub(crate) explicitly_provided: bool,
    pub(crate) inherited_from_profile: bool,
    pub(crate) supplied_by_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ResolvedScenario {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) aircraft: Aircraft,
    pub(crate) mission: Mission,
    pub(crate) requirements: Requirements,
    pub(crate) engine: EngineProfile,
    pub(crate) propeller: Option<PropellerProfile>,
    pub(crate) assumptions: Vec<AssumptionEntry>,
    pub(crate) warnings: Vec<Diagnostic>,
    pub(crate) source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Aircraft {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) category: String,
    pub(crate) configuration: String,
    pub(crate) propulsion_architecture: String,
    pub(crate) metadata: ConceptMetadata,
    pub(crate) mass: MassProperties,
    pub(crate) wing: Wing,
    pub(crate) aerodynamics: Aerodynamics,
    pub(crate) propulsion: Propulsion,
    pub(crate) limits: AircraftLimits,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MassProperties {
    pub(crate) maximum_takeoff_mass_kg: f64,
    pub(crate) operating_empty_mass_kg: f64,
    pub(crate) maximum_payload_mass_kg: f64,
    pub(crate) maximum_fuel_mass_kg: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Wing {
    pub(crate) area_m2: f64,
    pub(crate) span_m: f64,
    pub(crate) aspect_ratio: f64,
    pub(crate) sweep_quarter_chord_rad: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Aerodynamics {
    pub(crate) model: String,
    pub(crate) clean: AeroConfiguration,
    pub(crate) takeoff: AeroConfiguration,
    pub(crate) landing: AeroConfiguration,
}

impl Aerodynamics {
    pub(crate) fn configuration(&self, name: &str) -> Option<&AeroConfiguration> {
        match name {
            "clean" => Some(&self.clean),
            "takeoff" => Some(&self.takeoff),
            "landing" => Some(&self.landing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AeroConfiguration {
    pub(crate) cd0: f64,
    pub(crate) oswald_efficiency: f64,
    pub(crate) cl_max: f64,
    pub(crate) additional_cd: f64,
    pub(crate) mach_critical: Option<f64>,
    pub(crate) wave_drag: Option<WaveDrag>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Propulsion {
    pub(crate) profile: String,
    pub(crate) engine_count: u32,
    pub(crate) propeller_profile: Option<String>,
    pub(crate) sizing_factor: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AircraftLimits {
    pub(crate) maximum_operating_speed_m_s: Option<f64>,
    pub(crate) maximum_operating_mach: Option<f64>,
    pub(crate) maximum_operating_altitude_m: Option<f64>,
    pub(crate) maximum_load_factor: Option<f64>,
    pub(crate) minimum_load_factor: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Mission {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) payload_mass_kg: f64,
    pub(crate) segments: Vec<MissionSegment>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MissionSegment {
    pub(crate) id: String,
    pub(crate) kind: SegmentKind,
    pub(crate) duration_s: Option<f64>,
    pub(crate) distance_m: Option<f64>,
    pub(crate) target_altitude_m: Option<f64>,
    pub(crate) altitude_m: Option<f64>,
    pub(crate) indicated_airspeed_m_s: Option<f64>,
    pub(crate) true_airspeed_m_s: Option<f64>,
    pub(crate) mach: Option<f64>,
    pub(crate) power_fraction: Option<f64>,
    pub(crate) thrust_fraction: Option<f64>,
    pub(crate) fuel_fraction: Option<f64>,
    pub(crate) fuel_mass_kg: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SegmentKind {
    StartAndTaxi,
    FixedTime,
    FixedFuel,
    Takeoff,
    Climb,
    Cruise,
    Loiter,
    Descent,
    Landing,
    Reserve,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Requirements {
    pub(crate) id: String,
    pub(crate) items: Vec<Requirement>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Requirement {
    pub(crate) id: String,
    pub(crate) metric: String,
    pub(crate) operator: String,
    pub(crate) required: f64,
    pub(crate) unit: String,
    pub(crate) severity: String,
    pub(crate) weight: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum EngineProfile {
    Piston(PistonProfile),
    Turbofan(TurbofanProfile),
}

impl EngineProfile {
    pub(crate) fn model_id(&self) -> &str {
        match self {
            Self::Piston(profile) => &profile.model,
            Self::Turbofan(profile) => &profile.model,
        }
    }

    pub(crate) fn profile_id(&self) -> &str {
        match self {
            Self::Piston(profile) => &profile.id,
            Self::Turbofan(profile) => &profile.id,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PistonProfile {
    pub(crate) id: String,
    pub(crate) version: u32,
    pub(crate) model: String,
    pub(crate) source: String,
    pub(crate) confidence: String,
    pub(crate) rated_power_w: f64,
    pub(crate) rated_altitude_m: f64,
    pub(crate) rated_speed_rad_s: f64,
    pub(crate) dry_mass_kg: f64,
    pub(crate) lapse_exponent: f64,
    pub(crate) minimum_power_fraction: f64,
    pub(crate) bsfc_takeoff_kg_kwh: f64,
    pub(crate) bsfc_cruise_kg_kwh: f64,
    pub(crate) bsfc_economy_kg_kwh: f64,
    pub(crate) maximum_altitude_m: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PropellerProfile {
    pub(crate) id: String,
    pub(crate) version: u32,
    pub(crate) model: String,
    pub(crate) source: String,
    pub(crate) confidence: String,
    pub(crate) diameter_m: f64,
    pub(crate) blade_count: u32,
    pub(crate) static_efficiency: f64,
    pub(crate) cruise_efficiency: f64,
    pub(crate) maximum_efficiency: f64,
    pub(crate) climb_installation_factor: f64,
    pub(crate) tip_mach_limit: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TurbofanProfile {
    pub(crate) id: String,
    pub(crate) version: u32,
    pub(crate) model: String,
    pub(crate) source: String,
    pub(crate) confidence: String,
    pub(crate) sea_level_static_thrust_n: f64,
    pub(crate) dry_mass_kg: f64,
    pub(crate) bypass_ratio: f64,
    pub(crate) altitude_exponent: f64,
    pub(crate) mach_linear_coefficient: f64,
    pub(crate) minimum_thrust_fraction: f64,
    pub(crate) tsfc_takeoff_kg_n_hr: f64,
    pub(crate) tsfc_cruise_kg_n_hr: f64,
    pub(crate) cruise_reference_altitude_m: f64,
    pub(crate) cruise_reference_mach: f64,
    pub(crate) thrust_loss_fraction: f64,
    pub(crate) nacelle_drag_area_m2: f64,
    pub(crate) overall_length_m: Option<f64>,
    pub(crate) maximum_diameter_m: Option<f64>,
    pub(crate) maximum_mach: f64,
    pub(crate) maximum_altitude_m: f64,
}

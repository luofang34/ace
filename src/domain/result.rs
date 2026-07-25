use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::Diagnostic;
use crate::domain::quantity::QuantityOutput;
use crate::domain::schema::AssumptionEntry;
use crate::domain::validity::{MetricValidity, ModelValidityDomain};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResultProvenance {
    pub(crate) method: String,
    pub(crate) backend: String,
    pub(crate) assumptions: Vec<String>,
    pub(crate) validity_range: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) validity_domains: Vec<ModelValidityDomain>,
    pub(crate) units: BTreeMap<String, String>,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelMetadata {
    pub(crate) model_id: String,
    pub(crate) model_version: String,
    pub(crate) fidelity_level: u8,
    pub(crate) validity_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AtmosphereState {
    pub(crate) altitude_m: f64,
    pub(crate) temperature_k: f64,
    pub(crate) pressure_pa: f64,
    pub(crate) density_kg_m3: f64,
    pub(crate) speed_of_sound_m_s: f64,
    pub(crate) dynamic_viscosity_pa_s: f64,
    pub(crate) kinematic_viscosity_m2_s: f64,
    pub(crate) model: ModelMetadata,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AerodynamicState {
    pub(crate) dynamic_pressure_pa: f64,
    pub(crate) lift_coefficient: f64,
    pub(crate) drag_coefficient: f64,
    pub(crate) induced_drag_coefficient: f64,
    pub(crate) wave_drag_coefficient: f64,
    pub(crate) drag_n: f64,
    pub(crate) power_required_w: f64,
    pub(crate) lift_to_drag_ratio: f64,
    pub(crate) model: ModelMetadata,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PropulsionState {
    pub(crate) thrust_available_n: Option<f64>,
    pub(crate) shaft_power_available_w: Option<f64>,
    pub(crate) propulsive_power_available_w: Option<f64>,
    pub(crate) fuel_flow_kg_s: f64,
    pub(crate) model: ModelMetadata,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PointPerformanceResult {
    pub(crate) scenario_id: String,
    pub(crate) altitude_m: f64,
    pub(crate) true_airspeed_m_s: f64,
    pub(crate) mach: f64,
    pub(crate) mass_kg: f64,
    pub(crate) configuration: String,
    pub(crate) stall_speed_m_s: f64,
    pub(crate) aerodynamics: AerodynamicState,
    pub(crate) propulsion: PropulsionState,
    pub(crate) thrust_required_n: f64,
    pub(crate) power_required_w: f64,
    pub(crate) excess_thrust_n: f64,
    pub(crate) excess_power_w: f64,
    pub(crate) rate_of_climb_m_s: f64,
    pub(crate) climb_gradient: f64,
    pub(crate) best_glide_speed_m_s: f64,
    pub(crate) maximum_lift_to_drag_ratio: f64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) metric_validity: BTreeMap<String, MetricValidity>,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CruiseConditionPerformance {
    pub(crate) segment_id: String,
    pub(crate) altitude_m: f64,
    pub(crate) mass_kg: f64,
    pub(crate) declared_true_airspeed_m_s: f64,
    pub(crate) declared_mach: f64,
    #[serde(default)]
    pub(crate) achieved_true_airspeed_m_s: Option<f64>,
    #[serde(default)]
    pub(crate) achieved_mach: Option<f64>,
    pub(crate) excess_power_w: f64,
    pub(crate) feasible: bool,
    pub(crate) validity: MetricValidity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PerformanceSummary {
    pub(crate) stall_speed_clean_m_s: f64,
    pub(crate) stall_speed_landing_m_s: f64,
    pub(crate) best_glide_speed_m_s: f64,
    pub(crate) minimum_power_speed_m_s: f64,
    pub(crate) maximum_lift_to_drag_ratio: f64,
    pub(crate) maximum_level_speed_m_s: f64,
    pub(crate) service_ceiling_m: f64,
    pub(crate) absolute_ceiling_m: f64,
    pub(crate) cruise_mach: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) declared_cruise_mach: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) declared_cruise_true_airspeed_m_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) achieved_cruise_mach: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) achieved_cruise_true_airspeed_m_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) minimum_cruise_excess_power_w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cruise_feasible: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) cruise_conditions: Vec<CruiseConditionPerformance>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) metric_validity: BTreeMap<String, MetricValidity>,
    pub(crate) model: ModelMetadata,
    pub(crate) warnings: Vec<Diagnostic>,
}

impl PerformanceSummary {
    pub(crate) fn validity_for(&self, metric: &str) -> MetricValidity {
        self.metric_validity
            .get(metric)
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MissionSegmentResult {
    pub(crate) segment_id: String,
    pub(crate) start_mass_kg: f64,
    pub(crate) end_mass_kg: f64,
    pub(crate) fuel_burn_kg: f64,
    pub(crate) payload_removed_kg: f64,
    pub(crate) distance_m: f64,
    pub(crate) duration_s: f64,
    pub(crate) start_altitude_m: f64,
    pub(crate) end_altitude_m: f64,
    #[serde(skip)]
    pub(crate) operating_speed_m_s: Option<f64>,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MissionResult {
    pub(crate) scenario_id: String,
    pub(crate) completed: bool,
    pub(crate) total_distance: QuantityOutput,
    pub(crate) total_duration_s: f64,
    pub(crate) total_fuel_burn_kg: f64,
    pub(crate) reserve_fuel_remaining_kg: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) landing_fuel: Option<QuantityOutput>,
    pub(crate) initial_takeoff_mass_kg: f64,
    pub(crate) final_mass_kg: f64,
    pub(crate) final_payload_mass_kg: f64,
    pub(crate) failed_segment: Option<String>,
    #[serde(default)]
    pub(crate) fuel_exhausted: bool,
    pub(crate) fuel_capacity_violation: bool,
    pub(crate) takeoff_mass_violation: bool,
    pub(crate) segments: Vec<MissionSegmentResult>,
    pub(crate) assumptions: Vec<AssumptionEntry>,
    pub(crate) warnings: Vec<Diagnostic>,
    pub(crate) model: ModelMetadata,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequirementStatus {
    Pass,
    Fail,
    Indeterminate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequirementEvaluation {
    pub(crate) id: String,
    pub(crate) metric: String,
    pub(crate) actual: QuantityOutput,
    pub(crate) required: QuantityOutput,
    pub(crate) operator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) status: Option<RequirementStatus>,
    pub(crate) passed: Option<bool>,
    #[serde(default)]
    pub(crate) validity: MetricValidity,
    pub(crate) absolute_margin: f64,
    pub(crate) percentage_margin: Option<f64>,
    pub(crate) severity: String,
    pub(crate) warning_state: bool,
}

impl RequirementEvaluation {
    pub(crate) fn resolved_status(&self) -> RequirementStatus {
        self.status.unwrap_or(match self.passed {
            Some(true) => RequirementStatus::Pass,
            Some(false) | None => RequirementStatus::Fail,
        })
    }

    pub(crate) fn is_passed(&self) -> bool {
        self.resolved_status() == RequirementStatus::Pass
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StructuralScreen {
    pub(crate) passed: bool,
    pub(crate) structural_configuration: String,
    pub(crate) ultimate_load_factor: f64,
    pub(crate) wing_root_bending_moment: QuantityOutput,
    pub(crate) required_total_spar_cap_area: QuantityOutput,
    pub(crate) spar_cap_packaging_ratio: f64,
    pub(crate) horizontal_tail_volume: f64,
    pub(crate) vertical_tail_volume: f64,
    pub(crate) aspect_ratio: f64,
    pub(crate) estimated_usable_internal_volume: Option<QuantityOutput>,
    pub(crate) required_fuel_volume: Option<QuantityOutput>,
    pub(crate) fuel_volume_utilization_ratio: Option<f64>,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) provenance: ResultProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MissionPowerPoint {
    pub(crate) segment_id: String,
    pub(crate) altitude: QuantityOutput,
    pub(crate) true_airspeed: QuantityOutput,
    pub(crate) mass: QuantityOutput,
    pub(crate) throttle: f64,
    pub(crate) excess_power: QuantityOutput,
    pub(crate) required_reserve: QuantityOutput,
    pub(crate) passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MissionPowerScreen {
    pub(crate) passed: bool,
    pub(crate) minimum_excess_power: QuantityOutput,
    pub(crate) minimum_reserve_margin: QuantityOutput,
    pub(crate) required_reserve_fraction: f64,
    pub(crate) points: Vec<MissionPowerPoint>,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) provenance: ResultProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PayloadRangePoint {
    pub(crate) id: String,
    pub(crate) range: QuantityOutput,
    pub(crate) payload_kg: f64,
    pub(crate) fuel_kg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PayloadRangeResult {
    pub(crate) scenario_id: String,
    pub(crate) points: Vec<PayloadRangePoint>,
    pub(crate) simplified_cruise_assumption: bool,
    pub(crate) warnings: Vec<Diagnostic>,
    pub(crate) model: ModelMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ConstraintResult {
    pub(crate) scenario_id: String,
    pub(crate) x_axis: String,
    pub(crate) y_axis: String,
    pub(crate) wing_loading_n_m2: Vec<f64>,
    pub(crate) constraints: std::collections::BTreeMap<String, Vec<f64>>,
    pub(crate) feasible_region_mask: Vec<bool>,
    pub(crate) selected_wing_loading_n_m2: f64,
    pub(crate) selected_loading: f64,
    pub(crate) active_controlling_constraint: String,
    pub(crate) warnings: Vec<Diagnostic>,
    pub(crate) model: ModelMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SweepRow {
    pub(crate) variables: std::collections::BTreeMap<String, serde_json::Value>,
    pub(crate) metrics: std::collections::BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) metric_validity: BTreeMap<String, MetricValidity>,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SweepResult {
    pub(crate) scenario_id: String,
    pub(crate) rows: Vec<SweepRow>,
    pub(crate) deterministic_ordering: bool,
    pub(crate) warnings: Vec<Diagnostic>,
    pub(crate) provenance: ResultProvenance,
}

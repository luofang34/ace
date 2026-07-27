use std::collections::BTreeMap;

use crate::backends::contracts::GeometryOutput;
use crate::domain::diagnostic::AexResult;
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{
    MassPropertiesAnalysis, MissionPowerScreen, MissionResult, PayloadRangeResult,
    PerformanceSummary, StructuralScreen,
};
use crate::models::breguet::BreguetEstimate;
use crate::models::field_performance::FieldPerformanceEstimate;

pub(super) struct NativeMetricInputs<'a> {
    pub(super) geometry: &'a GeometryOutput,
    pub(super) performance: &'a PerformanceSummary,
    pub(super) mission: &'a MissionResult,
    pub(super) payload_range: &'a PayloadRangeResult,
    pub(super) breguet: &'a BreguetEstimate,
    pub(super) structural: &'a StructuralScreen,
    pub(super) mission_power: &'a MissionPowerScreen,
    pub(super) takeoff_field: &'a FieldPerformanceEstimate,
    pub(super) landing_field: &'a FieldPerformanceEstimate,
    pub(super) weight_kg: f64,
    pub(super) mass_properties: &'a MassPropertiesAnalysis,
}

pub(super) fn native_metrics(
    input: NativeMetricInputs<'_>,
) -> AexResult<BTreeMap<String, QuantityOutput>> {
    let mut metrics = BTreeMap::new();
    insert_aircraft_metrics(&mut metrics, &input)?;
    insert_mission_metrics(&mut metrics, &input);
    insert_structural_metrics(&mut metrics, &input);
    insert_mass_properties_metrics(&mut metrics, input.mass_properties);
    Ok(metrics)
}

fn insert_mass_properties_metrics(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    analysis: &MassPropertiesAnalysis,
) {
    metrics.insert(
        "mass_properties.minimum_center_of_gravity".to_owned(),
        analysis.minimum_center_of_gravity.clone(),
    );
    metrics.insert(
        "mass_properties.maximum_center_of_gravity".to_owned(),
        analysis.maximum_center_of_gravity.clone(),
    );
    metrics.insert(
        "mass_properties.closure_error".to_owned(),
        analysis.closure_error.clone(),
    );
    metrics.insert(
        "stability.neutral_point".to_owned(),
        analysis
            .neutral_point
            .clone()
            .unwrap_or_else(|| QuantityOutput::si(0.0, "m")),
    );
    insert(
        metrics,
        "stability.minimum_static_margin",
        analysis.minimum_static_margin.unwrap_or(0.0),
        "1",
    );
    insert(
        metrics,
        "stability.maximum_static_margin",
        analysis.maximum_static_margin.unwrap_or(0.0),
        "1",
    );
}

fn insert_aircraft_metrics(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    input: &NativeMetricInputs<'_>,
) -> AexResult<()> {
    insert(
        metrics,
        "weight.estimated_takeoff_mass",
        input.weight_kg,
        "kg",
    );
    insert(
        metrics,
        "geometry.wing_area",
        input.geometry.metrics.wing_area.value,
        "m^2",
    );
    insert(
        metrics,
        "aerodynamics.maximum_lift_to_drag_ratio",
        input.performance.maximum_lift_to_drag_ratio,
        "1",
    );
    insert_performance(metrics, input.performance, input)
}

fn insert_mission_metrics(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    input: &NativeMetricInputs<'_>,
) {
    metrics.insert(
        "mission.breguet_range".to_owned(),
        QuantityOutput::range(input.breguet.range_m),
    );
    insert(
        metrics,
        "mission.breguet_endurance",
        input.breguet.endurance_s,
        "s",
    );
    metrics.insert(
        "mission.simulated_range".to_owned(),
        input.mission.total_distance.clone(),
    );
    insert(
        metrics,
        "mission.completed",
        f64::from(u8::from(input.mission.completed)),
        "bool",
    );
    insert_payload_range(metrics, input.payload_range);
    insert(
        metrics,
        "mission.fuel_burn",
        input.mission.total_fuel_burn_kg,
        "kg",
    );
    if let Some(landing_fuel) = &input.mission.landing_fuel {
        metrics.insert("mission.landing_fuel".to_owned(), landing_fuel.clone());
    }
}

fn insert_structural_metrics(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    input: &NativeMetricInputs<'_>,
) {
    insert(
        metrics,
        "structures.wing_root_bending_moment",
        input.structural.wing_root_bending_moment.value,
        "N*m",
    );
    insert(
        metrics,
        "structures.spar_cap_packaging_ratio",
        input.structural.spar_cap_packaging_ratio,
        "1",
    );
    if let Some(volume) = &input.structural.estimated_usable_internal_volume {
        insert(
            metrics,
            "structures.estimated_usable_internal_volume",
            volume.value,
            "m^3",
        );
    }
    if let Some(ratio) = input.structural.fuel_volume_utilization_ratio {
        insert(
            metrics,
            "structures.fuel_volume_utilization_ratio",
            ratio,
            "1",
        );
    }
    insert(
        metrics,
        "mission.minimum_excess_power",
        input.mission_power.minimum_excess_power.value,
        "W",
    );
    insert(
        metrics,
        "mission.minimum_power_reserve_margin",
        input.mission_power.minimum_reserve_margin.value,
        "W",
    );
}

fn insert_payload_range(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    payload_range: &PayloadRangeResult,
) {
    for (point_id, metric_id) in [
        ("full_payload_mission", "performance.full_payload_range"),
        ("zero_payload_ferry", "performance.zero_payload_ferry_range"),
    ] {
        if let Some(point) = payload_range
            .points
            .iter()
            .find(|point| point.id == point_id)
        {
            metrics.insert(metric_id.to_owned(), point.range.clone());
        }
    }
}

fn insert_performance(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    performance: &PerformanceSummary,
    input: &NativeMetricInputs<'_>,
) -> AexResult<()> {
    insert(
        metrics,
        "performance.stall_speed_clean",
        performance.stall_speed_clean_m_s,
        "m/s",
    );
    insert(
        metrics,
        "performance.maximum_level_speed",
        performance.maximum_level_speed_m_s,
        "m/s",
    );
    insert(
        metrics,
        "performance.service_ceiling",
        performance.service_ceiling_m,
        "m",
    );
    insert(
        metrics,
        "performance.absolute_ceiling",
        performance.absolute_ceiling_m,
        "m",
    );
    insert_cruise_metrics(metrics, performance);
    insert(
        metrics,
        "performance.takeoff_field_length",
        input.takeoff_field.distance_m,
        "m",
    );
    insert(
        metrics,
        "performance.landing_field_length",
        input.landing_field.distance_m,
        "m",
    );
    Ok(())
}

fn insert_cruise_metrics(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    performance: &PerformanceSummary,
) {
    insert_optional(
        metrics,
        "performance.declared_cruise_mach",
        performance.declared_cruise_mach.or(performance.cruise_mach),
        "1",
    );
    insert_optional(
        metrics,
        "performance.declared_cruise_true_airspeed",
        performance.declared_cruise_true_airspeed_m_s,
        "m/s",
    );
    insert_optional(
        metrics,
        "performance.achieved_cruise_mach",
        performance.achieved_cruise_mach,
        "1",
    );
    insert_optional(
        metrics,
        "performance.achieved_cruise_true_airspeed",
        performance.achieved_cruise_true_airspeed_m_s,
        "m/s",
    );
    insert_optional(
        metrics,
        "performance.minimum_cruise_excess_power",
        performance.minimum_cruise_excess_power_w,
        "W",
    );
    insert_optional(
        metrics,
        "performance.cruise_feasible",
        performance
            .cruise_feasible
            .map(|feasible| f64::from(u8::from(feasible))),
        "bool",
    );
}

fn insert(metrics: &mut BTreeMap<String, QuantityOutput>, name: &str, value: f64, unit: &str) {
    metrics.insert(name.to_owned(), QuantityOutput::si(value, unit));
}

fn insert_optional(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    name: &str,
    value: Option<f64>,
    unit: &str,
) {
    if let Some(value) = value {
        insert(metrics, name, value, unit);
    }
}

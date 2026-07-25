use std::collections::BTreeMap;

use crate::backends::contracts::{
    AnalysisBackend, AnalysisOutput, AnalysisRequest, BackendDescriptor, GeometryBackend,
    GeometryMetrics, GeometryOutput, GeometryRequest, PolarPoint, ResultProvenance,
    StabilitySummary,
};
use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::schema::ResolvedScenario;
use crate::models::blended_wing::{BlendedWingPlanform, is_blended_wing_body};
use crate::models::breguet;
use crate::models::concept_geometry::ConceptGeometry;
use crate::models::field_performance::{estimate_landing_distance_m, estimate_takeoff_distance_m};
use crate::models::mission::MissionSimulator;
use crate::models::mission_power;
use crate::models::payload_range::PayloadRangeAnalyzer;
use crate::models::performance::PointAnalyzer;
use crate::models::structural_screen;
use crate::models::weight::{WeightClosureInput, solve_weight_closure};
use crate::services::requirements::evaluate_requirements;

mod descriptor;
mod topology;

use descriptor::native_descriptor;
pub(crate) use topology::native_topology_violations;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NativeBackend;

fn geometry_dimensions(
    wing_area: f64,
    wing_span: f64,
    concept: ConceptGeometry,
    planform: Option<BlendedWingPlanform>,
    has_horizontal_tail: bool,
    has_vertical_tail: bool,
) -> (f64, f64, f64, f64) {
    if let Some(planform) = planform {
        return (
            planform.mean_aerodynamic_chord_m(),
            0.0,
            0.0,
            2.2 * wing_area,
        );
    }
    let horizontal_tail = if has_horizontal_tail {
        concept.horizontal_tail_area_m2
    } else {
        0.0
    };
    let vertical_tail = if has_vertical_tail {
        concept.vertical_tail_area_m2
    } else {
        0.0
    };
    let wetted_area = 2.05 * (wing_area + horizontal_tail + vertical_tail)
        + 0.85 * std::f64::consts::PI * concept.fuselage_width_m * concept.fuselage_length_m;
    (
        wing_area / wing_span,
        horizontal_tail,
        vertical_tail,
        wetted_area,
    )
}

impl GeometryBackend for NativeBackend {
    fn descriptor(&self) -> BackendDescriptor {
        native_descriptor()
    }

    fn generate_geometry_blocking(
        &self,
        request: GeometryRequest<'_>,
    ) -> AexResult<GeometryOutput> {
        let aircraft = &request.scenario.aircraft;
        let wing_area = aircraft.wing.area_m2;
        let wing_span = aircraft.wing.span_m;
        let concept = ConceptGeometry::from_scenario(request.scenario);
        let blended = is_blended_wing_body(request.scenario);
        let planform = blended
            .then(|| BlendedWingPlanform::from_wing(&aircraft.wing))
            .transpose()?;
        let topology = &aircraft.topology;
        let dimensions = geometry_dimensions(
            wing_area,
            wing_span,
            concept,
            planform,
            topology.has_component_kind("horizontal_tail"),
            topology.has_component_kind("vertical_tail"),
        );
        let (mean_chord, horizontal_tail, vertical_tail, wetted_area) = dimensions;
        Ok(GeometryOutput {
            metrics: GeometryMetrics {
                wing_area: QuantityOutput::si(wing_area, "m^2"),
                wing_span: QuantityOutput::si(wing_span, "m"),
                mean_aerodynamic_chord: QuantityOutput::si(mean_chord, "m"),
                horizontal_tail_area: QuantityOutput::si(horizontal_tail, "m^2"),
                vertical_tail_area: QuantityOutput::si(vertical_tail, "m^2"),
                wetted_area: QuantityOutput::si(wetted_area, "m^2"),
                aspect_ratio: aircraft.wing.aspect_ratio,
                center_body_leading_edge_sweep: planform
                    .map(|item| QuantityOutput::si(item.center_edge_sweep_rad.to_degrees(), "deg")),
                center_body_trailing_edge_sweep: planform.map(|item| {
                    QuantityOutput::si(-item.center_edge_sweep_rad.to_degrees(), "deg")
                }),
                edge_alignment_error: planform.map(|item| {
                    QuantityOutput::si(item.edge_alignment_error_rad().to_degrees(), "deg")
                }),
                independent_planform_angle_count: planform
                    .map(BlendedWingPlanform::independent_planform_angle_count),
                estimated_usable_internal_volume: planform
                    .map(|item| QuantityOutput::si(item.estimated_usable_volume_m3(), "m^3")),
            },
            artifact_path: None,
            provenance: native_geometry_provenance(blended),
        })
    }
}

impl AnalysisBackend for NativeBackend {
    fn analyze_blocking(&self, request: AnalysisRequest<'_>) -> AexResult<AnalysisOutput> {
        let scenario = request.scenario;
        let performance = PointAnalyzer::new(scenario.clone()).summary()?;
        let mission = MissionSimulator::new(scenario.clone()).simulate()?;
        let payload_range = PayloadRangeAnalyzer::new(scenario.clone()).analyze()?;
        let structural_screen = structural_screen::evaluate(scenario)?;
        let mission_power_screen = mission_power::evaluate(scenario, &mission)?;
        let requirements =
            evaluate_requirements(scenario, &mission, &performance, Some(&payload_range));
        let breguet = breguet::estimate(
            scenario,
            performance.maximum_lift_to_drag_ratio,
            mission.total_fuel_burn_kg,
        )?;
        let weight = weight_estimate(scenario, mission.total_fuel_burn_kg)?;
        let metrics = native_metrics(NativeMetricInputs {
            scenario,
            geometry: request.geometry,
            performance: &performance,
            mission: &mission,
            payload_range: &payload_range,
            breguet: &breguet,
            structural: &structural_screen,
            mission_power: &mission_power_screen,
            weight_kg: weight,
        })?;
        let feasible = mission.completed
            && !mission.fuel_exhausted
            && !mission.fuel_capacity_violation
            && !mission.takeoff_mass_violation
            && structural_screen.passed
            && mission_power_screen.passed
            && requirements
                .iter()
                .filter(|item| item.severity == "hard")
                .all(|item| item.passed);
        let mut failed_constraints = failed_constraints(&requirements, &mission);
        failed_constraints.extend(structural_screen.failed_constraints.clone());
        failed_constraints.extend(mission_power_screen.failed_constraints.clone());
        let mut warnings = performance.warnings.clone();
        warnings.extend(mission.warnings.clone());
        warnings.extend(payload_range.warnings.clone());
        warnings.extend(breguet.warnings);
        warnings.extend(structural_screen.provenance.warnings.clone());
        warnings.extend(mission_power_screen.provenance.warnings.clone());
        warnings.push(Diagnostic::warning(
            "NATIVE_STABILITY_NOT_MODELED",
            "The native backend does not estimate stability derivatives.",
            "analysis.stability",
        ));
        Ok(AnalysisOutput {
            metrics,
            polar: native_polar(scenario),
            stability: StabilitySummary {
                pitching_moment_slope_per_deg: None,
                statically_stable: None,
                note: "No native stability-derivative model is registered.".to_owned(),
            },
            structural_screen: Some(structural_screen),
            mission_power_screen: Some(mission_power_screen),
            requirements,
            feasible: Some(feasible),
            failed_constraints,
            provenance: native_analysis_provenance(scenario, warnings, breguet.assumptions),
        })
    }
}

fn weight_estimate(scenario: &ResolvedScenario, fuel_kg: f64) -> AexResult<f64> {
    let mass = &scenario.aircraft.mass;
    let empty_fraction = mass.operating_empty_mass_kg / mass.maximum_takeoff_mass_kg;
    let fixed_mass = scenario.mission.payload_mass_kg + fuel_kg;
    let expected = fixed_mass / (1.0 - empty_fraction);
    let result = solve_weight_closure(WeightClosureInput {
        payload_kg: scenario.mission.payload_mass_kg,
        fuel_kg,
        empty_coefficient: empty_fraction,
        empty_exponent: 1.0,
        lower_mass_kg: fixed_mass.max(1.0),
        upper_mass_kg: (expected * 1.5).max(mass.maximum_takeoff_mass_kg * 2.0),
    })?;
    Ok(result.takeoff_mass_kg)
}

struct NativeMetricInputs<'a> {
    scenario: &'a ResolvedScenario,
    geometry: &'a GeometryOutput,
    performance: &'a crate::domain::result::PerformanceSummary,
    mission: &'a crate::domain::result::MissionResult,
    payload_range: &'a crate::domain::result::PayloadRangeResult,
    breguet: &'a breguet::BreguetEstimate,
    structural: &'a crate::domain::result::StructuralScreen,
    mission_power: &'a crate::domain::result::MissionPowerScreen,
    weight_kg: f64,
}

fn native_metrics(input: NativeMetricInputs<'_>) -> AexResult<BTreeMap<String, QuantityOutput>> {
    let mut metrics = BTreeMap::new();
    insert(
        &mut metrics,
        "weight.estimated_takeoff_mass",
        input.weight_kg,
        "kg",
    );
    insert(
        &mut metrics,
        "geometry.wing_area",
        input.geometry.metrics.wing_area.value,
        "m^2",
    );
    insert(
        &mut metrics,
        "aerodynamics.maximum_lift_to_drag_ratio",
        input.performance.maximum_lift_to_drag_ratio,
        "1",
    );
    insert_performance(&mut metrics, input.performance, input.scenario)?;
    metrics.insert(
        "mission.breguet_range".to_owned(),
        QuantityOutput::range(input.breguet.range_m),
    );
    insert(
        &mut metrics,
        "mission.breguet_endurance",
        input.breguet.endurance_s,
        "s",
    );
    metrics.insert(
        "mission.simulated_range".to_owned(),
        input.mission.total_distance.clone(),
    );
    insert_payload_range(&mut metrics, input.payload_range);
    insert(
        &mut metrics,
        "mission.fuel_burn",
        input.mission.total_fuel_burn_kg,
        "kg",
    );
    insert(
        &mut metrics,
        "structures.wing_root_bending_moment",
        input.structural.wing_root_bending_moment.value,
        "N*m",
    );
    insert(
        &mut metrics,
        "structures.spar_cap_packaging_ratio",
        input.structural.spar_cap_packaging_ratio,
        "1",
    );
    if let Some(volume) = &input.structural.estimated_usable_internal_volume {
        insert(
            &mut metrics,
            "structures.estimated_usable_internal_volume",
            volume.value,
            "m^3",
        );
    }
    if let Some(ratio) = input.structural.fuel_volume_utilization_ratio {
        insert(
            &mut metrics,
            "structures.fuel_volume_utilization_ratio",
            ratio,
            "1",
        );
    }
    insert(
        &mut metrics,
        "mission.minimum_excess_power",
        input.mission_power.minimum_excess_power.value,
        "W",
    );
    insert(
        &mut metrics,
        "mission.minimum_power_reserve_margin",
        input.mission_power.minimum_reserve_margin.value,
        "W",
    );
    Ok(metrics)
}

fn insert_payload_range(
    metrics: &mut BTreeMap<String, QuantityOutput>,
    payload_range: &crate::domain::result::PayloadRangeResult,
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
    performance: &crate::domain::result::PerformanceSummary,
    scenario: &ResolvedScenario,
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
        "performance.takeoff_field_length",
        estimate_takeoff_distance_m(scenario),
        "m",
    );
    insert(
        metrics,
        "performance.landing_field_length",
        estimate_landing_distance_m(scenario)?,
        "m",
    );
    Ok(())
}

fn insert(metrics: &mut BTreeMap<String, QuantityOutput>, name: &str, value: f64, unit: &str) {
    metrics.insert(name.to_owned(), QuantityOutput::si(value, unit));
}

fn failed_constraints(
    requirements: &[crate::domain::result::RequirementEvaluation],
    mission: &crate::domain::result::MissionResult,
) -> Vec<String> {
    let mut failed: Vec<String> = requirements
        .iter()
        .filter(|item| !item.passed)
        .map(|item| item.id.clone())
        .collect();
    if !mission.completed {
        failed.push("mission_completion".to_owned());
    }
    if mission.fuel_exhausted {
        failed.push("fuel_exhausted".to_owned());
    }
    if mission.fuel_capacity_violation {
        failed.push("fuel_capacity".to_owned());
    }
    if mission.takeoff_mass_violation {
        failed.push("takeoff_mass".to_owned());
    }
    failed
}

fn native_polar(scenario: &ResolvedScenario) -> Vec<PolarPoint> {
    let aero = &scenario.aircraft.aerodynamics.clean;
    let induced_factor =
        1.0 / (std::f64::consts::PI * aero.oswald_efficiency * scenario.aircraft.wing.aspect_ratio);
    (0..=8)
        .map(|index| {
            let lift_coefficient = -0.2 + f64::from(index) * 0.2;
            PolarPoint {
                angle_of_attack_deg: None,
                lift_coefficient,
                drag_coefficient: aero.cd0
                    + aero.additional_cd
                    + induced_factor * lift_coefficient.powi(2),
                pitching_moment_coefficient: None,
            }
        })
        .collect()
}

fn native_geometry_provenance(blended: bool) -> ResultProvenance {
    let (method, assumptions, validity_range) = if blended {
        (
            "area-closed two-panel BWB planform identities",
            vec![
                "one center-body edge angle enforces equal-and-opposite leading and trailing edges"
                    .to_owned(),
                "one outer quarter-chord sweep is independent".to_owned(),
                "wetted area is 2.2 times reference planform area".to_owned(),
            ],
            vec!["tailless blended-wing conceptual geometry".to_owned()],
        )
    } else {
        (
            "planform identities and empirical tail-volume ratios",
            vec![
                "horizontal tail area is 20-24% of wing area".to_owned(),
                "vertical tail area is 10-12% of wing area".to_owned(),
                "fuselage dimensions scale from wing span and area".to_owned(),
            ],
            vec!["conventional fixed-wing configurations".to_owned()],
        )
    };
    ResultProvenance {
        method: method.to_owned(),
        backend: "native".to_owned(),
        assumptions,
        validity_range,
        units: geometry_units(),
        warnings: vec![Diagnostic::limitation(
            "Native geometry dimensions and wetted area are conceptual estimates.",
        )],
    }
}

fn native_analysis_provenance(
    scenario: &ResolvedScenario,
    warnings: Vec<Diagnostic>,
    mut assumptions: Vec<String>,
) -> ResultProvenance {
    assumptions.extend([
        "parabolic drag polar".to_owned(),
        "ISA 1976 atmosphere".to_owned(),
        "quasi-steady mission segments".to_owned(),
        "sea-level dry paved runway for field-length estimates".to_owned(),
    ]);
    let configuration_range = if is_blended_wing_body(scenario) {
        "subsonic BWB represented by an equivalent parabolic polar"
    } else {
        "subsonic conventional fixed-wing aircraft"
    };
    ResultProvenance {
        method: "deterministic conceptual performance synthesis".to_owned(),
        backend: "native".to_owned(),
        assumptions,
        validity_range: vec![
            configuration_range.to_owned(),
            "fidelity level 0-1; not for certification".to_owned(),
        ],
        units: analysis_units(),
        warnings,
    }
}

fn geometry_units() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("area".to_owned(), "m^2".to_owned()),
        ("length".to_owned(), "m".to_owned()),
        ("aspect_ratio".to_owned(), "1".to_owned()),
    ])
}

fn analysis_units() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("mass".to_owned(), "kg".to_owned()),
        ("distance".to_owned(), "m".to_owned()),
        ("duration".to_owned(), "s".to_owned()),
        ("speed".to_owned(), "m/s".to_owned()),
        ("coefficient".to_owned(), "1".to_owned()),
    ])
}

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::backends::contracts::{
    AnalysisBackend, AnalysisOutput, AnalysisRequest, BackendDescriptor, GeometryBackend,
    GeometryMetrics, GeometryOutput, GeometryRequest, PolarPoint, ResultProvenance,
    StabilitySummary,
};
use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{
    MassPropertiesAnalysis, MissionPowerScreen, MissionResult, PayloadRangeResult,
    PerformanceSummary, RequirementEvaluation, StructuralScreen,
};
use crate::domain::schema::ResolvedScenario;
use crate::domain::validity::MetricValidity;
use crate::domain::warning::WarningCode;
use crate::models::aerodynamics::coefficient_evaluation_at_mach;
use crate::models::blended_wing::{BlendedWingPlanform, is_blended_wing_body};
use crate::models::breguet;
use crate::models::breguet::BreguetEstimate;
use crate::models::concept_geometry::ConceptGeometry;
use crate::models::field_performance::{
    FieldPerformanceEstimate, estimate_landing_distance, estimate_takeoff_distance,
};
use crate::models::mass_properties;
use crate::models::mission::MissionSimulator;
use crate::models::mission_power;
use crate::models::payload_range::PayloadRangeAnalyzer;
use crate::models::performance::PointAnalyzer;
use crate::models::structural_screen;
use crate::models::validity::scenario_domains;
use crate::models::weight::{WeightClosureInput, solve_weight_closure};
use crate::services::requirements::{
    evaluate_requirements, failed_hard_requirement_ids, hard_requirements_passed,
};

mod descriptor;
mod metrics;
mod topology;

use descriptor::native_descriptor;
use metrics::{NativeMetricInputs, native_metrics};
pub(crate) use topology::native_topology_violations;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NativeBackend;

struct NativeAnalysisState {
    mission: MissionResult,
    performance: PerformanceSummary,
    payload_range: PayloadRangeResult,
    structural_screen: StructuralScreen,
    mission_power_screen: MissionPowerScreen,
    mass_properties: MassPropertiesAnalysis,
    requirements: Vec<RequirementEvaluation>,
    breguet: BreguetEstimate,
    weight_kg: f64,
    takeoff_field: FieldPerformanceEstimate,
    landing_field: FieldPerformanceEstimate,
    polar: Vec<PolarPoint>,
    polar_warnings: Vec<Diagnostic>,
}

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
        let state = evaluate_native_state(request.scenario)?;
        build_native_output(request, state)
    }
}

fn evaluate_native_state(scenario: &ResolvedScenario) -> AexResult<NativeAnalysisState> {
    let mission = MissionSimulator::new(scenario.clone()).simulate()?;
    let performance = PointAnalyzer::new(scenario.clone()).summary(Some(&mission))?;
    let payload_range = PayloadRangeAnalyzer::new(scenario.clone()).analyze()?;
    let structural_screen = structural_screen::evaluate(scenario)?;
    let mission_power_screen = mission_power::evaluate(scenario, &mission)?;
    let mass_properties = mass_properties::evaluate(scenario, &mission)?;
    let requirements =
        evaluate_requirements(scenario, &mission, &performance, Some(&payload_range))?;
    let breguet = breguet::estimate(
        scenario,
        performance.maximum_lift_to_drag_ratio,
        mission.total_fuel_burn_kg,
    )?;
    let weight_kg = weight_estimate(scenario, mission.total_fuel_burn_kg)?;
    let takeoff_field = estimate_takeoff_distance(scenario)?;
    let landing_field = estimate_landing_distance(scenario)?;
    let (polar, polar_warnings) = native_polar(scenario)?;
    Ok(NativeAnalysisState {
        mission,
        performance,
        payload_range,
        structural_screen,
        mission_power_screen,
        mass_properties,
        requirements,
        breguet,
        weight_kg,
        takeoff_field,
        landing_field,
        polar,
        polar_warnings,
    })
}

fn build_native_output(
    request: AnalysisRequest<'_>,
    state: NativeAnalysisState,
) -> AexResult<AnalysisOutput> {
    let metrics = native_state_metrics(request.geometry, &state)?;
    let metric_validity = native_metric_validity(&state);
    let feasible = native_feasible(request.scenario, &state);
    let failed_constraints = native_failed_constraints(request.scenario, &state);
    let warnings = native_warnings(&state);
    let stability = native_stability_summary(&state.mass_properties);
    let provenance = native_analysis_provenance(
        request.scenario,
        warnings,
        state.breguet.assumptions.clone(),
    )?;
    Ok(AnalysisOutput {
        metrics,
        metric_validity,
        polar: state.polar,
        stability,
        structural_screen: Some(state.structural_screen),
        mission_power_screen: Some(state.mission_power_screen),
        mass_properties: Some(state.mass_properties),
        requirements: state.requirements,
        feasible: Some(feasible),
        failed_constraints,
        provenance,
    })
}

fn native_state_metrics(
    geometry: &GeometryOutput,
    state: &NativeAnalysisState,
) -> AexResult<BTreeMap<String, QuantityOutput>> {
    native_metrics(NativeMetricInputs {
        geometry,
        performance: &state.performance,
        mission: &state.mission,
        payload_range: &state.payload_range,
        breguet: &state.breguet,
        structural: &state.structural_screen,
        mission_power: &state.mission_power_screen,
        takeoff_field: &state.takeoff_field,
        landing_field: &state.landing_field,
        weight_kg: state.weight_kg,
        mass_properties: &state.mass_properties,
    })
}

fn native_metric_validity(state: &NativeAnalysisState) -> BTreeMap<String, MetricValidity> {
    let mut validity = state.performance.metric_validity.clone();
    let stability = if state.mass_properties.stability_supported {
        MetricValidity::default()
    } else {
        MetricValidity::unsupported()
    };
    for metric in [
        "stability.neutral_point",
        "stability.minimum_static_margin",
        "stability.maximum_static_margin",
    ] {
        validity.insert(metric.to_owned(), stability.clone());
    }
    validity
}

fn native_feasible(scenario: &ResolvedScenario, state: &NativeAnalysisState) -> bool {
    hard_requirements_passed(
        state.mission.completed,
        &scenario.requirements.items,
        &state.requirements,
    ) && !state.mission.fuel_exhausted
        && !state.mission.fuel_capacity_violation
        && !state.mission.takeoff_mass_violation
        && state.performance.cruise_feasible.unwrap_or(true)
        && state.structural_screen.passed
        && state.mission_power_screen.passed
        && state.mass_properties.failed_constraints.is_empty()
}

fn native_failed_constraints(
    scenario: &ResolvedScenario,
    state: &NativeAnalysisState,
) -> Vec<String> {
    let mut failed = failed_constraints(
        &scenario.requirements.items,
        &state.requirements,
        &state.mission,
    );
    failed.extend(state.structural_screen.failed_constraints.clone());
    failed.extend(state.mission_power_screen.failed_constraints.clone());
    failed.extend(state.mass_properties.failed_constraints.clone());
    if state.performance.cruise_feasible == Some(false) {
        failed.push("cruise_capability".to_owned());
    }
    failed
}

fn native_warnings(state: &NativeAnalysisState) -> Vec<Diagnostic> {
    let mut warnings = state.performance.warnings.clone();
    warnings.extend(state.mission.warnings.clone());
    warnings.extend(state.payload_range.warnings.clone());
    warnings.extend(state.breguet.warnings.clone());
    warnings.extend(state.structural_screen.provenance.warnings.clone());
    warnings.extend(state.mission_power_screen.provenance.warnings.clone());
    warnings.extend(state.mass_properties.provenance.warnings.clone());
    extend_unique_warnings(&mut warnings, state.takeoff_field.warnings.clone());
    extend_unique_warnings(&mut warnings, state.landing_field.warnings.clone());
    extend_unique_warnings(&mut warnings, state.polar_warnings.clone());
    if !state.mass_properties.stability_supported {
        warnings.push(Diagnostic::warning(
            WarningCode::NativeStabilityNotModeled,
            "The native backend cannot estimate tailless neutral-point stability.",
            "analysis.stability",
        ));
    }
    warnings
}

fn native_stability_summary(mass: &MassPropertiesAnalysis) -> StabilitySummary {
    let (statically_stable, note) = if mass.stability_supported {
        (
            mass.minimum_static_margin.map(|margin| margin > 0.0),
            "Native neutral point uses the resolved horizontal-tail volume.",
        )
    } else {
        (
            None,
            "Native tailless neutral-point stability is unsupported.",
        )
    };
    StabilitySummary {
        pitching_moment_slope_per_deg: None,
        statically_stable,
        neutral_point: mass.neutral_point.clone(),
        minimum_static_margin: mass.minimum_static_margin,
        note: note.to_owned(),
    }
}

fn extend_unique_warnings(target: &mut Vec<Diagnostic>, warnings: Vec<Diagnostic>) {
    for warning in warnings {
        if !target
            .iter()
            .any(|existing| existing.code == warning.code && existing.path == warning.path)
        {
            target.push(warning);
        }
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

fn failed_constraints(
    declared: &[crate::domain::schema::Requirement],
    requirements: &[crate::domain::result::RequirementEvaluation],
    mission: &crate::domain::result::MissionResult,
) -> Vec<String> {
    let mut failed: Vec<String> = requirements
        .iter()
        .filter(|item| !item.is_passed())
        .map(|item| item.id.clone())
        .collect();
    for id in failed_hard_requirement_ids(declared, requirements) {
        if !failed.iter().any(|failed_id| failed_id == &id) {
            failed.push(id);
        }
    }
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

fn native_polar(scenario: &ResolvedScenario) -> AexResult<(Vec<PolarPoint>, Vec<Diagnostic>)> {
    let aero = &scenario.aircraft.aerodynamics.clean;
    let evaluation = coefficient_evaluation_at_mach(&scenario.aircraft, "clean", 0.0)?;
    let polar = evaluation.coefficients;
    let points = (0..=8)
        .map(|index| {
            let lift_coefficient = -0.2 + f64::from(index) * 0.2;
            PolarPoint {
                angle_of_attack_deg: None,
                lift_coefficient,
                drag_coefficient: polar.cd0
                    + aero.additional_cd
                    + polar.induced_drag_factor * lift_coefficient.powi(2),
                pitching_moment_coefficient: None,
            }
        })
        .collect();
    Ok((points, evaluation.warnings))
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
            "planform identities and resolved conventional geometry",
            vec![
                "explicit fuselage and tail geometry takes precedence".to_owned(),
                "missing conventional geometry uses versioned statistical correlations".to_owned(),
                "wetted area uses the same resolved geometry supplied to geometry backends"
                    .to_owned(),
            ],
            vec!["conventional fixed-wing configurations".to_owned()],
        )
    };
    ResultProvenance {
        method: method.to_owned(),
        backend: "native".to_owned(),
        assumptions,
        validity_range,
        validity_domains: Vec::new(),
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
) -> AexResult<ResultProvenance> {
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
    Ok(ResultProvenance {
        method: "deterministic conceptual performance synthesis".to_owned(),
        backend: "native".to_owned(),
        assumptions,
        validity_range: vec![
            configuration_range.to_owned(),
            "fidelity level 0-1; not for certification".to_owned(),
        ],
        validity_domains: scenario_domains(scenario)?,
        units: analysis_units(),
        warnings,
    })
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

use std::path::Path;

use crate::backends::contracts::GeometryOutput;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::{EngineProfile, ResolvedScenario, Wing};
use crate::models::concept_geometry::ConceptGeometry;

use super::script_string;

const BLENDED_WING_BODY_TEMPLATE: &str = include_str!("scripts/blended_wing_body.vspscript");
const CONVENTIONAL_TEMPLATE: &str = include_str!("scripts/conventional.vspscript");

#[derive(Debug, Clone, Copy)]
struct BlendedWingPlanform {
    inner_span_m: f64,
    outer_span_m: f64,
    root_chord_m: f64,
    middle_chord_m: f64,
    tip_chord_m: f64,
    center_quarter_sweep_deg: f64,
}

#[derive(Debug, Clone, Copy)]
struct PlanformPanel {
    span_m: f64,
    root_chord_m: f64,
    tip_chord_m: f64,
    root_quarter_x_m: f64,
    tip_quarter_x_m: f64,
}

impl PlanformPanel {
    fn area_m2(self) -> f64 {
        0.5 * self.span_m * (self.root_chord_m + self.tip_chord_m)
    }

    fn chord_square_integral_m3(self) -> f64 {
        self.span_m
            * (self.root_chord_m.powi(2)
                + self.root_chord_m * self.tip_chord_m
                + self.tip_chord_m.powi(2))
            / 3.0
    }

    fn chord_quarter_x_integral_m3(self) -> f64 {
        self.span_m
            * (2.0 * self.root_chord_m * self.root_quarter_x_m
                + self.root_chord_m * self.tip_quarter_x_m
                + self.tip_chord_m * self.root_quarter_x_m
                + 2.0 * self.tip_chord_m * self.tip_quarter_x_m)
            / 6.0
    }
}

pub(super) fn geometry_script(
    scenario: &ResolvedScenario,
    native: &GeometryOutput,
    artifact: &Path,
) -> AexResult<String> {
    if is_blended_wing_body(scenario) {
        return blended_wing_body_script(scenario, artifact);
    }
    conventional_geometry_script(scenario, native, artifact)
}

pub(super) fn is_blended_wing_body(scenario: &ResolvedScenario) -> bool {
    let configuration = scenario.aircraft.configuration.to_ascii_lowercase();
    configuration.contains("blended_wing")
        || configuration.contains("flying_wing")
        || configuration.contains("tailless")
}

pub(super) fn blended_wing_center_of_gravity_x(scenario: &ResolvedScenario) -> AexResult<f64> {
    let wing = &scenario.aircraft.wing;
    let planform = blended_wing_planform(wing)?;
    let inner_root_quarter_x = 0.25 * planform.root_chord_m;
    let inner_tip_quarter_x = inner_root_quarter_x
        + planform.inner_span_m * planform.center_quarter_sweep_deg.to_radians().tan();
    let outer_tip_quarter_x =
        inner_tip_quarter_x + planform.outer_span_m * wing.sweep_quarter_chord_rad.tan();
    let panels = [
        PlanformPanel {
            span_m: planform.inner_span_m,
            root_chord_m: planform.root_chord_m,
            tip_chord_m: planform.middle_chord_m,
            root_quarter_x_m: inner_root_quarter_x,
            tip_quarter_x_m: inner_tip_quarter_x,
        },
        PlanformPanel {
            span_m: planform.outer_span_m,
            root_chord_m: planform.middle_chord_m,
            tip_chord_m: planform.tip_chord_m,
            root_quarter_x_m: inner_tip_quarter_x,
            tip_quarter_x_m: outer_tip_quarter_x,
        },
    ];
    let half_area = panels
        .iter()
        .copied()
        .map(PlanformPanel::area_m2)
        .sum::<f64>();
    let mean_aerodynamic_chord = panels
        .iter()
        .copied()
        .map(PlanformPanel::chord_square_integral_m3)
        .sum::<f64>()
        / half_area;
    let quarter_chord_x = panels
        .iter()
        .copied()
        .map(PlanformPanel::chord_quarter_x_integral_m3)
        .sum::<f64>()
        / half_area;
    Ok(quarter_chord_x + 0.05 * mean_aerodynamic_chord)
}

fn blended_wing_body_script(scenario: &ResolvedScenario, artifact: &Path) -> AexResult<String> {
    let wing = &scenario.aircraft.wing;
    let planform = blended_wing_planform(wing)?;
    let (engine_length, engine_diameter) = turbofan_envelope(scenario);
    let values = [
        ("__INNER_SPAN__", decimal(planform.inner_span_m)),
        ("__OUTER_SPAN__", decimal(planform.outer_span_m)),
        ("__ROOT_CHORD__", decimal(planform.root_chord_m)),
        ("__MIDDLE_CHORD__", decimal(planform.middle_chord_m)),
        ("__TIP_CHORD__", decimal(planform.tip_chord_m)),
        (
            "__CENTER_SWEEP_DEG__",
            decimal(planform.center_quarter_sweep_deg),
        ),
        (
            "__SWEEP_DEG__",
            decimal(wing.sweep_quarter_chord_rad.to_degrees()),
        ),
        ("__ENGINE_LENGTH__", decimal(engine_length)),
        (
            "__ENGINE_FINE_RATIO__",
            decimal(engine_length / engine_diameter),
        ),
        ("__ENGINE_X__", decimal(planform.root_chord_m * 0.58)),
        ("__ARTIFACT__", script_string(artifact)?),
    ];
    Ok(render_template(BLENDED_WING_BODY_TEMPLATE, &values))
}

fn blended_wing_planform(wing: &Wing) -> AexResult<BlendedWingPlanform> {
    let semispan = wing.span_m * 0.5;
    let inner_span = semispan * 0.23;
    let outer_span = semispan - inner_span;
    let tip_ratio = 0.16;
    let (root_chord, middle_chord, center_sweep) = match wing.center_body_edge_sweep_rad {
        Some(edge_sweep) => edge_swept_center(wing, inner_span, outer_span, edge_sweep)?,
        None => default_center(wing, inner_span, outer_span, tip_ratio),
    };
    Ok(BlendedWingPlanform {
        inner_span_m: inner_span,
        outer_span_m: outer_span,
        root_chord_m: root_chord,
        middle_chord_m: middle_chord,
        tip_chord_m: middle_chord * tip_ratio,
        center_quarter_sweep_deg: center_sweep.to_degrees(),
    })
}

fn edge_swept_center(
    wing: &Wing,
    inner_span: f64,
    outer_span: f64,
    edge_sweep: f64,
) -> AexResult<(f64, f64, f64)> {
    if !edge_sweep.is_finite() || edge_sweep <= 0.0 || edge_sweep >= 90_f64.to_radians() {
        return Err(AexError::validation(
            "INVALID_CENTER_BODY_EDGE_SWEEP",
            "aircraft.geometry.wing.center_body_edge_sweep",
            "edge sweep must be finite and between 0 and 90 degrees",
        ));
    }
    let chord_delta = 2.0 * inner_span * edge_sweep.tan();
    let denominator = 2.0 * inner_span + outer_span * 1.16;
    let middle_chord = (wing.area_m2 - inner_span * chord_delta) / denominator;
    if middle_chord <= 0.0 {
        return Err(AexError::validation(
            "CENTER_BODY_PLANFORM_DOES_NOT_CLOSE",
            "aircraft.geometry.wing.center_body_edge_sweep",
            "requested edge sweep consumes the available wing area",
        ));
    }
    let root_chord = middle_chord + chord_delta;
    let quarter_sweep = (0.5 * edge_sweep.tan()).atan();
    Ok((root_chord, middle_chord, quarter_sweep))
}

fn default_center(
    wing: &Wing,
    inner_span: f64,
    outer_span: f64,
    tip_ratio: f64,
) -> (f64, f64, f64) {
    let middle_ratio = 0.55;
    let denominator =
        inner_span * (1.0 + middle_ratio) + outer_span * middle_ratio * (1.0 + tip_ratio);
    let root_chord = wing.area_m2 / denominator;
    let middle_chord = root_chord * middle_ratio;
    let quarter_sweep = (0.25 * (root_chord - middle_chord) / inner_span).atan();
    (root_chord, middle_chord, quarter_sweep)
}

fn turbofan_envelope(scenario: &ResolvedScenario) -> (f64, f64) {
    match &scenario.engine {
        EngineProfile::Turbofan(profile) => (
            profile.overall_length_m.unwrap_or(1.888),
            profile.maximum_diameter_m.unwrap_or(1.888 / 3.0),
        ),
        EngineProfile::Piston(_) => (1.888, 1.888 / 3.0),
    }
}

fn conventional_geometry_script(
    scenario: &ResolvedScenario,
    native: &GeometryOutput,
    artifact: &Path,
) -> AexResult<String> {
    let wing = &scenario.aircraft.wing;
    let concept = ConceptGeometry::from_scenario(scenario);
    let transport = scenario.aircraft.category.contains("transport");
    let values = [
        ("__FUSELAGE_LENGTH__", decimal(concept.fuselage_length_m)),
        ("__NOSE_WIDTH__", decimal(concept.fuselage_width_m * 0.72)),
        ("__NOSE_HEIGHT__", decimal(concept.fuselage_height_m * 0.78)),
        ("__FUSELAGE_WIDTH__", decimal(concept.fuselage_width_m)),
        ("__FUSELAGE_HEIGHT__", decimal(concept.fuselage_height_m)),
        ("__TAIL_WIDTH__", decimal(concept.fuselage_width_m * 0.42)),
        ("__TAIL_HEIGHT__", decimal(concept.fuselage_height_m * 0.52)),
        ("__WING_AREA__", decimal(wing.area_m2)),
        ("__ASPECT_RATIO__", decimal(wing.aspect_ratio)),
        ("__TAPER_RATIO__", decimal(concept.taper_ratio)),
        (
            "__SWEEP_DEG__",
            decimal(wing.sweep_quarter_chord_rad.to_degrees()),
        ),
        (
            "__DIHEDRAL_DEG__",
            decimal(if transport { 5.0 } else { 1.5 }),
        ),
        (
            "__TWIST_DEG__",
            decimal(if transport { -1.5 } else { -2.0 }),
        ),
        (
            "__WING_CAMBER__",
            decimal(if transport { 0.015 } else { 0.02 }),
        ),
        ("__WING_X__", decimal(concept.wing_x_m)),
        ("__WING_Z__", decimal(concept.wing_z_m)),
        (
            "__HORIZONTAL_TAIL_AREA__",
            decimal(native.metrics.horizontal_tail_area.value),
        ),
        (
            "__VERTICAL_TAIL_AREA__",
            decimal(native.metrics.vertical_tail_area.value),
        ),
        ("__TAIL_X__", decimal(concept.tail_x_m)),
        ("__TAIL_Z__", decimal(concept.tail_z_m)),
        ("__ARTIFACT__", script_string(artifact)?),
    ];
    Ok(render_template(CONVENTIONAL_TEMPLATE, &values))
}

fn decimal(value: f64) -> String {
    format!("{value:.12}")
}

fn render_template(template: &str, values: &[(&str, String)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |script, (key, value)| {
            script.replace(key, value)
        })
}

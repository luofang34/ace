use std::path::Path;

use crate::backends::contracts::GeometryOutput;
use crate::domain::diagnostic::AexResult;
use crate::domain::schema::ResolvedScenario;
use crate::models::concept_geometry::ConceptGeometry;

use super::script_string;

const BLENDED_WING_BODY_TEMPLATE: &str = include_str!("scripts/blended_wing_body.vspscript");
const CONVENTIONAL_TEMPLATE: &str = include_str!("scripts/conventional.vspscript");

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

fn blended_wing_body_script(scenario: &ResolvedScenario, artifact: &Path) -> AexResult<String> {
    let wing = &scenario.aircraft.wing;
    let semispan = wing.span_m * 0.5;
    let inner_span = semispan * 0.23;
    let outer_span = semispan - inner_span;
    let middle_ratio = 0.55;
    let tip_ratio = 0.16;
    let area_denominator =
        inner_span * (1.0 + middle_ratio) + outer_span * middle_ratio * (1.0 + tip_ratio);
    let root_chord = wing.area_m2 / area_denominator;
    let middle_chord = root_chord * middle_ratio;
    let values = [
        ("__INNER_SPAN__", decimal(inner_span)),
        ("__OUTER_SPAN__", decimal(outer_span)),
        ("__ROOT_CHORD__", decimal(root_chord)),
        ("__MIDDLE_CHORD__", decimal(middle_chord)),
        ("__TIP_CHORD__", decimal(middle_chord * tip_ratio)),
        (
            "__SWEEP_DEG__",
            decimal(wing.sweep_quarter_chord_rad.to_degrees()),
        ),
        ("__ENGINE_X__", decimal(root_chord * 0.58)),
        ("__ARTIFACT__", script_string(artifact)?),
    ];
    Ok(render_template(BLENDED_WING_BODY_TEMPLATE, &values))
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

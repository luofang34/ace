use std::path::Path;

use crate::backends::contracts::GeometryOutput;
use crate::domain::diagnostic::AexResult;
use crate::domain::schema::ResolvedScenario;
use crate::models::blended_wing::{BlendedWingPlanform, is_blended_wing_body};
use crate::models::concept_geometry::ConceptGeometry;

use super::script_string;

mod layout;
#[cfg(test)]
mod preview;

use layout::{conventional_propulsion_layout, engine_envelope};

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

pub(super) fn blended_wing_center_of_gravity_x(scenario: &ResolvedScenario) -> AexResult<f64> {
    Ok(BlendedWingPlanform::from_wing(&scenario.aircraft.wing)?.center_of_gravity_x_m())
}

fn blended_wing_body_script(scenario: &ResolvedScenario, artifact: &Path) -> AexResult<String> {
    let wing = &scenario.aircraft.wing;
    let planform = BlendedWingPlanform::from_wing(wing)?;
    let engine = engine_envelope(scenario);
    let values = [
        ("__INNER_SPAN__", decimal(planform.inner_span_m)),
        ("__OUTER_SPAN__", decimal(planform.outer_span_m)),
        ("__ROOT_CHORD__", decimal(planform.root_chord_m)),
        ("__MIDDLE_CHORD__", decimal(planform.middle_chord_m)),
        ("__TIP_CHORD__", decimal(planform.tip_chord_m)),
        (
            "__CENTER_SWEEP_DEG__",
            decimal(planform.center_quarter_sweep_rad.to_degrees()),
        ),
        (
            "__SWEEP_DEG__",
            decimal(wing.sweep_quarter_chord_rad.to_degrees()),
        ),
        ("__ENGINE_LENGTH__", decimal(engine.length_m)),
        (
            "__ENGINE_COUNT__",
            scenario.aircraft.propulsion.engine_count.to_string(),
        ),
        ("__ENGINE_Y__", decimal(wing.span_m * 0.12)),
        (
            "__ENGINE_FINE_RATIO__",
            decimal(engine.length_m / engine.diameter_m),
        ),
        ("__ENGINE_X__", decimal(planform.root_chord_m * 0.58)),
        ("__ARTIFACT__", script_string(artifact)?),
    ];
    Ok(render_template(BLENDED_WING_BODY_TEMPLATE, &values))
}

fn conventional_geometry_script(
    scenario: &ResolvedScenario,
    native: &GeometryOutput,
    artifact: &Path,
) -> AexResult<String> {
    let concept = ConceptGeometry::from_scenario(scenario);
    let mut values = conventional_airframe_values(scenario, native, concept);
    values.extend(conventional_propulsion_values(scenario, concept));
    values.push(("__ARTIFACT__", script_string(artifact)?));
    Ok(render_template(CONVENTIONAL_TEMPLATE, &values))
}

fn conventional_airframe_values(
    scenario: &ResolvedScenario,
    native: &GeometryOutput,
    concept: ConceptGeometry,
) -> Vec<(&'static str, String)> {
    let wing = &scenario.aircraft.wing;
    let transport = scenario.aircraft.category.contains("transport");
    vec![
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
    ]
}

fn conventional_propulsion_values(
    scenario: &ResolvedScenario,
    concept: ConceptGeometry,
) -> Vec<(&'static str, String)> {
    let layout = conventional_propulsion_layout(scenario, concept);
    let propeller = layout.propeller;
    vec![
        ("__ENGINE_LENGTH__", decimal(layout.engine.length_m)),
        (
            "__ENGINE_FINE_RATIO__",
            decimal(layout.engine.length_m / layout.engine.diameter_m),
        ),
        ("__ENGINE_COUNT__", layout.engine_count.to_string()),
        ("__ENGINE_Y__", decimal(layout.engine_y_m)),
        ("__ENGINE_X__", decimal(layout.engine_x_m)),
        ("__ENGINE_Z__", decimal(layout.engine_z_m)),
        (
            "__HAS_PROPELLER__",
            u8::from(propeller.is_some()).to_string(),
        ),
        (
            "__PROPELLER_DIAMETER__",
            decimal(propeller.map_or(1.9, |placement| placement.diameter_m)),
        ),
        (
            "__PROPELLER_BLADE_COUNT__",
            propeller
                .map_or(2, |placement| placement.blade_count)
                .to_string(),
        ),
        (
            "__PROPELLER_X__",
            decimal(propeller.map_or(layout.engine_x_m, |placement| placement.x_m)),
        ),
        (
            "__PROPELLER_Z__",
            decimal(propeller.map_or(layout.engine_z_m, |placement| placement.z_m)),
        ),
    ]
}

#[cfg(test)]
pub(super) fn visual_snapshot(scenario: &ResolvedScenario) -> String {
    preview::visual_snapshot(scenario)
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

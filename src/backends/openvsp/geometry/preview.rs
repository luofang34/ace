use crate::domain::schema::ResolvedScenario;
use crate::models::concept_geometry::ConceptGeometry;

use super::layout::{ConventionalPropulsionLayout, conventional_propulsion_layout};

const CANVAS_WIDTH: f64 = 800.0;
const CANVAS_HEIGHT: f64 = 360.0;

#[derive(Debug, Clone, Copy)]
struct Projection {
    x_origin: f64,
    y_origin: f64,
    x_scale: f64,
    y_scale: f64,
}

impl Projection {
    fn x(self, value: f64) -> f64 {
        self.x_origin + value * self.x_scale
    }

    fn y(self, value: f64) -> f64 {
        self.y_origin - value * self.y_scale
    }
}

pub(super) fn visual_snapshot(scenario: &ResolvedScenario) -> String {
    let concept = ConceptGeometry::from_scenario(scenario);
    let layout = conventional_propulsion_layout(scenario, concept);
    let represented_length_m = represented_length_m(scenario, concept, layout);
    let projection = Projection {
        x_origin: 50.0,
        y_origin: CANVAS_HEIGHT * 0.5,
        x_scale: 700.0 / represented_length_m,
        y_scale: 280.0 / scenario.aircraft.wing.span_m,
    };
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {CANVAS_WIDTH:.0} {CANVAS_HEIGHT:.0}\">\n  <title>{} OpenVSP conventional layout</title>\n",
        scenario.id
    );
    svg.push_str("  <g fill=\"none\" stroke=\"#172554\" stroke-width=\"2\">\n");
    svg.push_str(&airframe_svg(scenario, concept, projection));
    svg.push_str(&propulsion_svg(layout, projection));
    svg.push_str("  </g>\n</svg>\n");
    svg
}

fn airframe_svg(
    scenario: &ResolvedScenario,
    concept: ConceptGeometry,
    projection: Projection,
) -> String {
    let wing = &scenario.aircraft.wing;
    let (root_chord, tip_chord, semispan, tip_leading_x) =
        wing_outline_dimensions(scenario, concept);
    let fuselage = scenario
        .aircraft
        .geometry
        .fuselage
        .as_ref()
        .map_or_else(String::new, |_| {
            rectangle(
                "fuselage",
                0.0,
                0.0,
                concept.fuselage_length_m,
                concept.fuselage_width_m,
                projection,
            )
        });
    let wing = format!(
        "    <polygon id=\"wing\" data-area-m2=\"{:.3}\" points=\"{}\"/>\n",
        wing.area_m2,
        wing_points(
            concept.wing_x_m,
            root_chord,
            tip_leading_x,
            tip_chord,
            semispan,
            projection
        )
    );
    let tail = tail_svg(scenario, concept, projection);
    format!("{fuselage}{wing}{tail}")
}

fn wing_outline_dimensions(
    scenario: &ResolvedScenario,
    concept: ConceptGeometry,
) -> (f64, f64, f64, f64) {
    let wing = &scenario.aircraft.wing;
    let root_chord = 2.0 * wing.area_m2 / (wing.span_m * (1.0 + concept.taper_ratio));
    let tip_chord = root_chord * concept.taper_ratio;
    let semispan = wing.span_m * 0.5;
    let tip_leading_x = concept.wing_x_m
        + semispan * wing.sweep_quarter_chord_rad.tan()
        + 0.25 * (root_chord - tip_chord);
    (root_chord, tip_chord, semispan, tip_leading_x)
}

fn represented_length_m(
    scenario: &ResolvedScenario,
    concept: ConceptGeometry,
    layout: ConventionalPropulsionLayout,
) -> f64 {
    let (root_chord, tip_chord, _, tip_leading_x) = wing_outline_dimensions(scenario, concept);
    let mut maximum_x = (concept.wing_x_m + root_chord).max(tip_leading_x + tip_chord);
    if scenario.aircraft.geometry.fuselage.is_some() {
        maximum_x = maximum_x.max(concept.fuselage_length_m);
    }
    if scenario.aircraft.geometry.horizontal_tail.is_some() {
        let horizontal_span = (4.0 * concept.horizontal_tail_area_m2).sqrt();
        let horizontal_chord = concept.horizontal_tail_area_m2 / horizontal_span;
        maximum_x = maximum_x.max(concept.horizontal_tail_x_m + horizontal_chord);
    }
    if scenario.aircraft.geometry.vertical_tail.is_some() {
        maximum_x = maximum_x.max(vertical_tail_end_x(scenario, concept));
    }
    maximum_x.max(layout.engine_x_m + layout.engine.length_m)
}

fn wing_points(
    root_x: f64,
    root_chord: f64,
    tip_x: f64,
    tip_chord: f64,
    semispan: f64,
    projection: Projection,
) -> String {
    [
        (root_x, 0.0),
        (tip_x, semispan),
        (tip_x + tip_chord, semispan),
        (root_x + root_chord, 0.0),
        (tip_x + tip_chord, -semispan),
        (tip_x, -semispan),
    ]
    .map(|(x, y)| format!("{:.2},{:.2}", projection.x(x), projection.y(y)))
    .join(" ")
}

fn tail_svg(
    scenario: &ResolvedScenario,
    concept: ConceptGeometry,
    projection: Projection,
) -> String {
    let horizontal = scenario
        .aircraft
        .geometry
        .horizontal_tail
        .as_ref()
        .map_or_else(String::new, |_| {
            let span = (4.0 * concept.horizontal_tail_area_m2).sqrt();
            rectangle(
                "horizontal-tail",
                concept.horizontal_tail_x_m,
                0.0,
                concept.horizontal_tail_area_m2 / span,
                span,
                projection,
            )
        });
    let vertical = scenario
        .aircraft
        .geometry
        .vertical_tail
        .as_ref()
        .map_or_else(String::new, |_| {
            format!(
                "    <line id=\"vertical-tail\" data-area-m2=\"{:.3}\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/>\n",
                concept.vertical_tail_area_m2,
                projection.x(concept.vertical_tail_x_m),
                projection.y(0.0),
                projection.x(vertical_tail_end_x(scenario, concept)),
                projection.y(0.0)
            )
        });
    format!("{horizontal}{vertical}")
}

fn vertical_tail_end_x(scenario: &ResolvedScenario, concept: ConceptGeometry) -> f64 {
    if scenario.aircraft.geometry.fuselage.is_some() {
        concept.fuselage_length_m
    } else {
        concept.vertical_tail_x_m + (concept.vertical_tail_area_m2 / 1.8).sqrt()
    }
}

fn propulsion_svg(layout: ConventionalPropulsionLayout, projection: Projection) -> String {
    let engine_y_locations = if layout.engine_count == 1 {
        vec![0.0]
    } else {
        vec![-layout.engine_y_m, layout.engine_y_m]
    };
    let mut svg = String::new();
    for (index, y_m) in engine_y_locations.into_iter().enumerate() {
        svg.push_str(&rectangle(
            &format!("engine-{}", index + 1),
            layout.engine_x_m,
            y_m,
            layout.engine.length_m,
            layout.engine.diameter_m,
            projection,
        ));
    }
    if let Some(propeller) = layout.propeller {
        svg.push_str(&format!(
            "    <line id=\"propeller\" data-blades=\"{}\" data-diameter-m=\"{:.3}\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"#b91c1c\"/>\n",
            propeller.blade_count,
            propeller.diameter_m,
            projection.x(propeller.x_m),
            projection.y(propeller.diameter_m * 0.5),
            projection.x(propeller.x_m),
            projection.y(-propeller.diameter_m * 0.5)
        ));
    }
    svg
}

fn rectangle(
    id: &str,
    x_m: f64,
    y_m: f64,
    length_m: f64,
    width_m: f64,
    projection: Projection,
) -> String {
    format!(
        "    <rect id=\"{id}\" data-length-m=\"{length_m:.3}\" data-width-m=\"{width_m:.3}\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"3\"/>\n",
        projection.x(x_m),
        projection.y(y_m + width_m * 0.5),
        length_m * projection.x_scale,
        width_m * projection.y_scale
    )
}

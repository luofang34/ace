use super::panel;
use crate::{DrawingError, DrawingResult, Scene, Stroke};
use ace_physics::{ConceptGeometry, ConceptObject, GeometryReport};
use ace_sheet::Sheet;

mod projection;
mod sections;
use projection::{hull, project};

const SCALE: f64 = 1000.0 / 300.0;
const X: f64 = 130.0;
const PLAN: f64 = 135.0;
const SIDE: f64 = 231.0;
const FRONT: f64 = 265.0;

pub(super) fn draw(scene: &mut Scene, sheet: &Sheet) -> DrawingResult<()> {
    scene.text(
        [98.0, 34.0],
        "BODY SECTION A(x) / m2 - no wave-drag inference",
        "layout",
    );
    scene.text([98.0, 66.0], "PLAN | E1 conceptual surfaces", "layout");
    scene.text(
        [98.0, 209.0],
        "SIDE | same station datum and physical scale",
        "layout",
    );
    scene.text([98.0, 250.0], "FRONT | same scale", "layout");
    let Some(panel) = panel(sheet, "vehicle.concept_geometry", None) else {
        scene.text([120.0, 130.0], "Native geometry NOT CALCULATED", "layout");
        return Ok(());
    };
    let run = sheet
        .snapshot
        .runs
        .iter()
        .find(|r| r.id == panel.run_id)
        .ok_or_else(|| DrawingError::Invalid("missing geometry run".into()))?;
    let artifact = run
        .artifacts
        .iter()
        .find(|a| a.schema == "ace.geometry.concept/1")
        .ok_or_else(|| {
            DrawingError::Invalid("missing verified concept geometry artifact".into())
        })?;
    let text = artifact.text.as_ref().ok_or_else(|| {
        DrawingError::Invalid("geometry requires embedded replayable content".into())
    })?;
    artifact.verify(text.as_bytes())?;
    let geometry: ConceptGeometry = serde_json::from_str(text)?;
    let reference = format!("analysis:{}", panel.analysis);
    let envelopes = super::panel(sheet, "vehicle.geometry_envelopes", None)
        .map(|p| serde_json::from_value::<GeometryReport>(p.details.clone()))
        .transpose()?;
    stations(scene, &geometry, &reference);
    for (name, object) in &geometry.objects {
        draw_object(scene, name, object);
    }
    if let Some(report) = &envelopes {
        projection::occupancy(scene, sheet, report);
    }
    dimensions(scene, &geometry);
    sections::draw(scene, sheet, &geometry, envelopes.as_ref())?;
    scene.text(
        [98.0, 200.0],
        "Solid: estimated contour | dashed: occupancy envelope",
        "layout",
    );
    Ok(())
}

fn draw_object(scene: &mut Scene, name: &str, object: &ConceptObject) {
    let evidence = format!("geometry:{name}");
    for (view, origin) in [(0, [X, PLAN]), (1, [X, SIDE]), (2, [177.0, FRONT])] {
        for part in &object.parts {
            let points = hull(
                part.vertices
                    .iter()
                    .map(|p| project(*p, view, origin, SCALE))
                    .collect(),
            );
            scene.path(
                &format!("{name}:view{view}"),
                points,
                true,
                Stroke::Outline,
                &evidence,
            );
        }
    }
}

fn stations(scene: &mut Scene, geometry: &ConceptGeometry, reference: &str) {
    let Some(body) = geometry.objects.get("fuselage") else {
        return;
    };
    let length = body.maximum_m[0] - body.minimum_m[0];
    for i in 0..=(length / 5.0).floor() as u32 {
        let x = i as f64 * 5.0;
        let px = X + SCALE * x;
        scene.line([px, 58.0], [px, 240.0], Stroke::Datum, reference);
        scene.text([px - 1.0, 62.0], format!("{x:.0}"), reference);
    }
    scene.line(
        [X, PLAN],
        [X + length * SCALE, PLAN],
        Stroke::Datum,
        reference,
    );
    scene.line(
        [X, SIDE],
        [X + length * SCALE, SIDE],
        Stroke::Datum,
        reference,
    );
    let samples: Vec<_> = (0..=100)
        .map(|i| {
            let x = body.minimum_m[0] + length * i as f64 / 100.0;
            [x, ace_physics::concept_section_area(body, x)]
        })
        .collect();
    let max = samples.iter().map(|p| p[1]).fold(0.0, f64::max).max(1.0);
    scene.text([98.0, 43.0], format!("max {max:.2}"), reference);
    scene.path(
        "body-area",
        samples
            .iter()
            .map(|p| [X + p[0] * SCALE, 56.0 - p[1] / max * 18.0])
            .collect(),
        false,
        Stroke::Data,
        reference,
    );
}

fn dimensions(scene: &mut Scene, geometry: &ConceptGeometry) {
    if let Some(body) = geometry.objects.get("fuselage") {
        let a = X + body.minimum_m[0] * SCALE;
        let b = X + body.maximum_m[0] * SCALE;
        projection::dimension(
            scene,
            [a, 244.0],
            [b, 244.0],
            format!("L {:.2} m", body.maximum_m[0] - body.minimum_m[0]),
            "geometry:fuselage",
        );
    }
    if let Some(wing) = geometry.objects.get("wing") {
        let low = PLAN - wing.maximum_m[1] * SCALE;
        let high = PLAN - wing.minimum_m[1] * SCALE;
        projection::dimension(
            scene,
            [307.0, low],
            [307.0, high],
            format!("b {:.2} m", wing.maximum_m[1] - wing.minimum_m[1]),
            "geometry:wing",
        );
    }
}

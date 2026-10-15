use crate::{DrawingResult, Scene, Stroke};
use ace_physics::{ConceptGeometry, GeometryReport};
use ace_sheet::Sheet;

pub(super) fn draw(
    scene: &mut Scene,
    sheet: &Sheet,
    geometry: &ConceptGeometry,
    envelopes: Option<&GeometryReport>,
) -> DrawingResult<()> {
    scene.text([258.0, 252.0], "CABIN SECTION | 1:200 (A3)", "layout");
    if let Some(body) = geometry.objects.get("fuselage") {
        let station = sheet
            .snapshot
            .values
            .get("drawing.cabin_station")
            .filter(|v| v.usable())
            .and_then(|v| v.quantity)
            .and_then(|q| q.to(ace_model::Unit::Meter).ok())
            .map(|q| q.value);
        if let Some(x) = station {
            let reference = "geometry:fuselage";
            for segment in ace_physics::concept_section(body, x) {
                scene.path(
                    "body-section",
                    segment
                        .into_iter()
                        .map(|p| [283.0 + p[0] * 5.0, 267.0 - p[1] * 5.0])
                        .collect(),
                    false,
                    Stroke::Outline,
                    reference,
                );
            }
            scene.text(
                [258.0, 280.0],
                format!("x = {x:.2} m; occupancy dashed"),
                "parameter:drawing.cabin_station",
            );
            if let Some(report) = envelopes {
                cabin(scene, report, x);
            }
        } else {
            scene.text([258.0, 265.0], "Section station NOT CALCULATED", "layout");
        }
    }
    if let Some(report) = envelopes {
        packaging(scene, report);
    }
    Ok(())
}

fn cabin(scene: &mut Scene, report: &GeometryReport, station: f64) {
    for id in ["cabin", "cargo"] {
        if let Some(e) = report.envelopes.get(id)
            && station >= e.minimum_m[0]
            && station <= e.maximum_m[0]
        {
            let points = vec![
                [e.minimum_m[1], e.minimum_m[2]],
                [e.maximum_m[1], e.minimum_m[2]],
                [e.maximum_m[1], e.maximum_m[2]],
                [e.minimum_m[1], e.maximum_m[2]],
            ];
            scene.path(
                id,
                points
                    .into_iter()
                    .map(|p| [283.0 + p[0] * 5.0, 267.0 - p[1] * 5.0])
                    .collect(),
                true,
                Stroke::Envelope,
                &format!("geometry:{id}"),
            );
        }
    }
}

fn packaging(scene: &mut Scene, report: &GeometryReport) {
    scene.text([266.0, 178.0], "TANK / SPAR | 1:100 (A3)", "layout");
    let Some(tank) = report.envelopes.get("tank") else {
        return;
    };
    let x0 = tank.maximum_m[0] - 1.0;
    for id in ["tank", "rear_spar"] {
        if let Some(e) = report.envelopes.get(id) {
            let pts = vec![
                [e.minimum_m[0], e.minimum_m[2]],
                [e.maximum_m[0], e.minimum_m[2]],
                [e.maximum_m[0], e.maximum_m[2]],
                [e.minimum_m[0], e.maximum_m[2]],
            ];
            scene.path(
                id,
                pts.into_iter()
                    .map(|p| [270.0 + (p[0].max(x0) - x0) * 10.0, 179.0 - p[1] * 10.0])
                    .collect(),
                true,
                Stroke::Envelope,
                &format!("geometry:{id}"),
            );
        }
    }
    if let Some(check) = report
        .clearances
        .values()
        .find(|c| c.first == "tank" || c.second == "tank")
    {
        scene.text(
            [266.0, 202.0],
            format!("End detail; AABB gap {:.3} m", check.clearance_m),
            "geometry:tank",
        );
    }
}

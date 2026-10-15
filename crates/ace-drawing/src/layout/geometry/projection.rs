use super::{FRONT, PLAN, SCALE, SIDE, X};
use crate::{Scene, Stroke};
use ace_physics::GeometryReport;
use ace_sheet::Sheet;

pub(super) fn project(p: [f64; 3], view: usize, origin: [f64; 2], scale: f64) -> [f64; 2] {
    let pair = match view {
        0 => [p[0], p[1]],
        1 => [p[0], p[2]],
        _ => [p[1], p[2]],
    };
    [origin[0] + pair[0] * scale, origin[1] - pair[1] * scale]
}

pub(super) fn hull(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    for p in &points {
        append(&mut lower, *p);
    }
    for p in points.iter().rev() {
        append(&mut upper, *p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn append(chain: &mut Vec<[f64; 2]>, p: [f64; 2]) {
    while chain.len() >= 2 {
        let a = chain[chain.len() - 2];
        let b = chain[chain.len() - 1];
        if (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) > 1e-10 {
            break;
        }
        chain.pop();
    }
    chain.push(p);
}

pub(super) fn occupancy(scene: &mut Scene, sheet: &Sheet, report: &GeometryReport) {
    for (id, envelope) in &report.envelopes {
        if sheet
            .model
            .geometry
            .get(id)
            .is_some_and(|g| g.shape.is_some())
        {
            continue;
        }
        let reference = format!("geometry:{id}");
        for (view, origin) in [(0, [X, PLAN]), (1, [X, SIDE]), (2, [177.0, FRONT])] {
            let vertices = (0..8).map(|i| {
                std::array::from_fn(|axis| {
                    if i & (1 << axis) == 0 {
                        envelope.minimum_m[axis]
                    } else {
                        envelope.maximum_m[axis]
                    }
                })
            });
            let points = hull(vertices.map(|p| project(p, view, origin, SCALE)).collect());
            scene.path(
                &format!("{id}:envelope{view}"),
                points,
                true,
                Stroke::Envelope,
                &reference,
            );
        }
    }
    for (row, id) in ["cabin", "cargo", "tank", "gear_bay"].iter().enumerate() {
        if let Some(object) = report.envelopes.get(*id) {
            let center = std::array::from_fn(|i| (object.minimum_m[i] + object.maximum_m[i]) * 0.5);
            let point = project(center, 0, [X, PLAN], SCALE);
            let y = 87.0 + row as f64 * 9.0;
            let reference = format!("geometry:{id}");
            scene.text([98.0, y], id.to_uppercase(), &reference);
            scene.path(
                "leader",
                vec![[116.0, y - 1.0], [124.0, y - 1.0], point],
                false,
                Stroke::Envelope,
                &reference,
            );
        }
    }
}

pub(super) fn dimension(
    scene: &mut Scene,
    a: [f64; 2],
    b: [f64; 2],
    label: String,
    evidence: &str,
) {
    scene.line(a, b, Stroke::Annotation, evidence);
    for p in [a, b] {
        scene.line(
            [p[0] - 1.0, p[1] + 1.0],
            [p[0] + 1.0, p[1] - 1.0],
            Stroke::Annotation,
            evidence,
        );
    }
    let at = if (a[0] - b[0]).abs() < 0.1 {
        [a[0] - 25.0, (a[1] + b[1]) * 0.5 - 2.0]
    } else {
        [(a[0] + b[0]) * 0.5 - 8.0, a[1] - 2.0]
    };
    scene.text(at, label, evidence);
}

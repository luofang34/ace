use crate::{Scene, Stroke};

pub(crate) fn project(p: [f64; 3], view: usize, origin: [f64; 2], scale: f64) -> [f64; 2] {
    let pair = match view {
        0 => [p[0], p[1]],
        1 => [p[0], p[2]],
        _ => [p[1], p[2]],
    };
    [origin[0] + pair[0] * scale, origin[1] - pair[1] * scale]
}

pub(crate) fn hull(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
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

pub(crate) fn dimension(
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

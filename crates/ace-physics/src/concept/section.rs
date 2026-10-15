use super::ConceptObject;

pub(super) fn intersect(object: &ConceptObject, station: f64) -> Vec<[[f64; 2]; 2]> {
    let mut segments = Vec::new();
    for part in &object.parts {
        for face in &part.triangles {
            let mut points = Vec::new();
            for (ia, ib) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                let (Some(a), Some(b)) = (part.vertices.get(ia), part.vertices.get(ib)) else {
                    continue;
                };
                let da = a[0] - station;
                let db = b[0] - station;
                if (da >= 0.0 && db <= 0.0 || da <= 0.0 && db >= 0.0) && (db - da).abs() > 1e-12 {
                    let t = -da / (db - da);
                    let p = [a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])];
                    if !points
                        .iter()
                        .any(|q: &[f64; 2]| (p[0] - q[0]).abs() + (p[1] - q[1]).abs() < 1e-9)
                    {
                        points.push(p);
                    }
                }
            }
            if let [a, b] = points.as_slice() {
                segments.push([*a, *b]);
            }
        }
    }
    segments
}

pub(super) fn area(object: &ConceptObject, station: f64) -> f64 {
    let segments = intersect(object, station);
    if segments.is_empty() {
        return 0.0;
    }
    let mut points: Vec<_> = segments.iter().flatten().copied().collect();
    let center: [f64; 2] =
        std::array::from_fn(|i| points.iter().map(|p| p[i]).sum::<f64>() / points.len() as f64);
    points.sort_by(|a, b| {
        (a[1] - center[1])
            .atan2(a[0] - center[0])
            .total_cmp(&(b[1] - center[1]).atan2(b[0] - center[0]))
    });
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>()
        .abs()
        * 0.5
}

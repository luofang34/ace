use super::{DrawingGeometry, Frame, stroke};
use crate::layout::{arrangement::Page, imported};
use crate::{DrawingInput, DrawingResult, PathRole, Scene, Stroke};
use ace_model::ConceptShape;

pub(crate) fn body_id<'a>(input: &'a DrawingInput, geometry: &DrawingGeometry) -> Option<&'a str> {
    input
        .model
        .geometry
        .iter()
        .filter(|(_, g)| matches!(g.shape.as_deref(), Some(ConceptShape::Body { .. })))
        .filter_map(|(id, _)| {
            let body = geometry.native.as_ref()?.objects.get(id)?;
            Some((id.as_str(), body.maximum_m[0] - body.minimum_m[0]))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

struct Cut {
    id: String,
    station: f64,
    objects: Vec<String>,
    evidence: String,
}

fn cuts(input: &DrawingInput, geometry: &DrawingGeometry) -> DrawingResult<Vec<Cut>> {
    if let Some(data) = &input.data
        && !data.geometry.sections.is_empty()
    {
        return data
            .geometry
            .sections
            .iter()
            .map(|cut| {
                Ok(Cut {
                    id: cut.id.clone(),
                    station: imported::coordinate(&cut.station, input)?,
                    objects: cut.objects.clone(),
                    evidence: format!("cut:{}", cut.id),
                })
            })
            .collect();
    }
    let body = body_id(input, geometry).and_then(|id| geometry.native.as_ref()?.objects.get(id));
    Ok(body
        .map(|body| {
            [0.25, 0.5, 0.75]
                .into_iter()
                .zip(["A", "B", "C"])
                .map(|(t, id)| Cut {
                    id: id.into(),
                    station: body.minimum_m[0] + t * (body.maximum_m[0] - body.minimum_m[0]),
                    objects: Vec::new(),
                    evidence: "geometry:extent".into(),
                })
                .collect()
        })
        .unwrap_or_default())
}

pub(super) fn draw(
    scene: &mut Scene,
    input: &DrawingInput,
    geometry: &DrawingGeometry,
    page: &Page,
    frame: &Frame,
) -> DrawingResult<()> {
    let cuts = cuts(input, geometry)?;
    let [x, y, w, h] = page.views.sections;
    if cuts.is_empty() {
        scene.text([x, y + 10.], "Sections N/C", "layout");
        return Ok(());
    }
    let count = cuts.len().min(3);
    for (i, cut) in cuts.iter().take(3).enumerate() {
        let px = frame.x + cut.station * frame.scale;
        scene.line(
            [px, frame.side - 5.],
            [px, frame.side + 5.],
            Stroke::Structure,
            &cut.evidence,
        );
        scene.text([px + 1., frame.side - 5.], &cut.id, &cut.evidence);
        scene.line(
            [px, frame.plan - 8.],
            [px, frame.plan + 8.],
            Stroke::Structure,
            &cut.evidence,
        );
        scene.text([px + 1., frame.plan - 9.], &cut.id, &cut.evidence);
        let region = [x + i as f64 * w / count as f64, y, w / count as f64 - 3., h];
        detail(scene, input, geometry, cut, region)?;
    }
    if cuts.len() > 3 {
        scene.text(
            [x, y + h + 3.],
            format!("3/{} cuts; full register retained", cuts.len()),
            "layout",
        );
    }
    Ok(())
}

fn detail(
    scene: &mut Scene,
    input: &DrawingInput,
    geometry: &DrawingGeometry,
    cut: &Cut,
    region: [f64; 4],
) -> DrawingResult<()> {
    let [x, y, w, h] = region;
    let segments = segments(input, geometry, cut);
    let mut bounds = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for (seg, _, _) in &segments {
        for p in seg {
            bounds[0] = bounds[0].min(p[0]);
            bounds[1] = bounds[1].max(p[0]);
            bounds[2] = bounds[2].min(p[1]);
            bounds[3] = bounds[3].max(p[1]);
        }
    }
    scene.label(
        [x, y],
        format!("{}-{}", cut.id, cut.id),
        2.8,
        true,
        Stroke::Outline,
        &cut.evidence,
    );
    scene.text(
        [x, y + h],
        format!("FS {:.2} m", cut.station),
        &cut.evidence,
    );
    if segments.is_empty() {
        scene.text([x, y + 10.], "N/C", &cut.evidence);
        return Ok(());
    }
    let fit =
        (w / (bounds[1] - bounds[0]).max(0.1)).min((h - 9.) / (bounds[3] - bounds[2]).max(0.1));
    let denominator = (1000. / fit / 5.).ceil() * 5.;
    let scale = 1000. / denominator;
    let point = |p: [f64; 2]| {
        [
            x + w * 0.5 + (p[0] - (bounds[0] + bounds[1]) * 0.5) * scale,
            y + 4. + (h - 9.) * 0.5 - (p[1] - (bounds[2] + bounds[3]) * 0.5) * scale,
        ]
    };
    scene.text([x + 10., y], format!("1:{denominator:.1}"), &cut.evidence);
    draw_segments(scene, segments, point);
    for path in &geometry.paths {
        if (cut.objects.is_empty() || cut.objects.contains(&path.id))
            && path.role == PathRole::SectionMaterial
            && path
                .points
                .iter()
                .all(|p| (p[0] - cut.station).abs() < 1e-7)
        {
            let polygon: Vec<_> = path.points.iter().map(|p| point([p[1], p[2]])).collect();
            hatch(scene, &polygon, &format!("path:{}", path.id));
        }
    }
    Ok(())
}

type Segment = ([[f64; 2]; 2], Stroke, String);

fn segments(input: &DrawingInput, geometry: &DrawingGeometry, cut: &Cut) -> Vec<Segment> {
    let body = body_id(input, geometry);
    let mut segments = Vec::new();
    for (id, object) in geometry.native.iter().flat_map(|g| &g.objects) {
        let selected = if cut.objects.is_empty() {
            Some(id.as_str()) == body
        } else {
            cut.objects.contains(id)
        };
        if selected {
            for segment in ace_physics::concept_section(object, cut.station) {
                segments.push((segment, Stroke::Outline, format!("geometry:{id}")));
            }
        }
    }
    for path in &geometry.paths {
        if !cut.objects.is_empty() && !cut.objects.contains(&path.id) {
            continue;
        }
        let pairs = path.points.iter().zip(path.points.iter().cycle().skip(1));
        for (a, b) in pairs.take(path.points.len().saturating_sub(usize::from(!path.closed))) {
            if let Some(segment) = intersect(*a, *b, cut.station) {
                segments.push((segment, stroke(path.role), format!("path:{}", path.id)));
            }
        }
    }
    occupancy_segments(input, geometry, cut, &mut segments);
    segments
}

fn hatch(scene: &mut Scene, points: &[[f64; 2]], evidence: &str) {
    let low = points
        .iter()
        .map(|p| p[1] - p[0])
        .fold(f64::INFINITY, f64::min);
    let high = points
        .iter()
        .map(|p| p[1] - p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    for i in 1..10 {
        let v = low + (high - low) * f64::from(i) / 10.;
        let mut hits = Vec::new();
        for (a, b) in points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
        {
            let va = a[1] - a[0];
            let vb = b[1] - b[0];
            if (va <= v && vb > v) || (vb <= v && va > v) {
                hits.push(a[0] + (v - va) / (vb - va) * (b[0] - a[0]));
            }
        }
        hits.sort_by(f64::total_cmp);
        for pair in hits.as_chunks::<2>().0 {
            scene.line(
                [pair[0], pair[0] + v],
                [pair[1], pair[1] + v],
                Stroke::Hatch,
                evidence,
            );
        }
    }
}

fn intersect(a: [f64; 3], b: [f64; 3], station: f64) -> Option<[[f64; 2]; 2]> {
    let da = a[0] - station;
    let db = b[0] - station;
    if da.abs() < 1e-7 && db.abs() < 1e-7 {
        return Some([[a[1], a[2]], [b[1], b[2]]]);
    }
    if da * db > 0. || (da - db).abs() < 1e-12 {
        return None;
    }
    let t = da / (da - db);
    let hit = [a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])];
    Some([hit, hit])
}

fn draw_segments(scene: &mut Scene, segments: Vec<Segment>, point: impl Fn([f64; 2]) -> [f64; 2]) {
    for (segment, style, evidence) in segments {
        if segment[0] == segment[1] {
            let [x, y] = point(segment[0]);
            scene.path(
                "section-intersection-marker",
                vec![[x - 0.5, y], [x + 0.5, y]],
                false,
                style,
                &evidence,
            );
            scene.path(
                "section-intersection-marker",
                vec![[x, y - 0.5], [x, y + 0.5]],
                false,
                style,
                &evidence,
            );
        } else {
            scene.path(
                "section-cut",
                segment.into_iter().map(&point).collect(),
                false,
                style,
                &evidence,
            );
        }
    }
}

fn occupancy_segments(
    input: &DrawingInput,
    geometry: &DrawingGeometry,
    cut: &Cut,
    segments: &mut Vec<Segment>,
) {
    for (id, envelope) in geometry.envelopes.iter().flat_map(|r| &r.envelopes) {
        if input
            .model
            .geometry
            .get(id)
            .is_some_and(|g| g.shape.is_some())
            || (!cut.objects.is_empty() && !cut.objects.contains(id))
            || cut.station < envelope.minimum_m[0]
            || cut.station > envelope.maximum_m[0]
        {
            continue;
        }
        let lo = envelope.minimum_m;
        let hi = envelope.maximum_m;
        let corners = [
            [lo[1], lo[2]],
            [hi[1], lo[2]],
            [hi[1], hi[2]],
            [lo[1], hi[2]],
        ];
        for (a, b) in corners.iter().zip(corners.iter().cycle().skip(1)).take(4) {
            segments.push(([*a, *b], Stroke::Envelope, format!("geometry:{id}")));
        }
    }
}

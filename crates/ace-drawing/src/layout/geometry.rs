use super::{arrangement::Page, imported, panel};
use crate::{DrawingError, DrawingInput, DrawingResult, PathRole, Scene, Stroke};
use ace_physics::{ConceptGeometry, GeometryReport};
use ace_sheet::Sheet;

pub(crate) mod projection;
pub(super) mod sections;
use projection::{hull, project};

pub(super) struct ResolvedPath {
    pub id: String,
    pub role: PathRole,
    pub points: Vec<[f64; 3]>,
    pub closed: bool,
}

pub(super) struct DrawingGeometry {
    pub native: Option<ConceptGeometry>,
    pub paths: Vec<ResolvedPath>,
    pub envelopes: Option<GeometryReport>,
    pub bounds: Option<[[f64; 3]; 2]>,
}

pub(super) struct Frame {
    pub scale: f64,
    pub x: f64,
    pub plan: f64,
    pub side: f64,
    pub front: [f64; 2],
}

impl DrawingGeometry {
    pub(super) fn read(sheet: &Sheet, input: &DrawingInput) -> DrawingResult<Self> {
        let native = native(sheet)?;
        let paths = input
            .data
            .as_ref()
            .map(|d| d.geometry.paths.iter())
            .into_iter()
            .flatten()
            .map(|path| {
                Ok(ResolvedPath {
                    id: path.id.clone(),
                    role: path.role,
                    points: path
                        .points
                        .iter()
                        .map(|p| imported::position(p, input))
                        .collect::<DrawingResult<_>>()?,
                    closed: path.closed,
                })
            })
            .collect::<DrawingResult<Vec<_>>>()?;
        let envelopes = panel(sheet, "vehicle.geometry_envelopes", None)
            .map(|p| serde_json::from_value(p.details.clone()))
            .transpose()?;
        let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
        for point in native
            .iter()
            .flat_map(|g| g.objects.values())
            .flat_map(|o| o.parts.iter())
            .flat_map(|p| p.vertices.iter())
            .chain(paths.iter().flat_map(|p| p.points.iter()))
        {
            for axis in 0..3 {
                bounds[0][axis] = bounds[0][axis].min(point[axis]);
                bounds[1][axis] = bounds[1][axis].max(point[axis]);
            }
        }
        Ok(Self {
            native,
            paths,
            envelopes,
            bounds: bounds
                .iter()
                .flatten()
                .all(|v| v.is_finite())
                .then_some(bounds),
        })
    }

    pub(super) fn frame(&self, page: &Page) -> Option<Frame> {
        let [lo, hi] = self.bounds?;
        let length = (hi[0] - lo[0]).max(1.);
        let span = (hi[1] - lo[1]).max(1.);
        let fit = ((page.views.longitudinal[1] - 20.) / length)
            .min(page.views.plan[1] / span)
            .min(page.views.front[2] / span)
            .min(28. / (hi[2] - lo[2]).max(1.));
        let denominator = (1000. / fit / 25.).ceil() * 25.;
        let scale = 1000. / denominator;
        Some(Frame {
            scale,
            x: page.views.longitudinal[0] + page.views.longitudinal[1] * 0.5
                - (lo[0] + hi[0]) * scale * 0.5,
            plan: page.views.plan[0] + (lo[1] + hi[1]) * scale * 0.5,
            side: page.views.side + (lo[2] + hi[2]) * scale * 0.5,
            front: [
                page.views.front[0] - (lo[1] + hi[1]) * scale * 0.5,
                page.views.front[1] + (lo[2] + hi[2]) * scale * 0.5,
            ],
        })
    }
}

fn native(sheet: &Sheet) -> DrawingResult<Option<ConceptGeometry>> {
    let Some(panel) = panel(sheet, "vehicle.concept_geometry", None) else {
        return Ok(None);
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
        .ok_or_else(|| DrawingError::Invalid("missing concept geometry artifact".into()))?;
    let text = artifact
        .text
        .as_ref()
        .ok_or_else(|| DrawingError::Invalid("geometry requires embedded content".into()))?;
    artifact.verify(text.as_bytes())?;
    Ok(Some(serde_json::from_str(text)?))
}

pub(super) fn draw(
    scene: &mut Scene,
    sheet: &Sheet,
    input: &DrawingInput,
    geometry: &DrawingGeometry,
    page: &Page,
) -> DrawingResult<()> {
    let Some(frame) = geometry.frame(page) else {
        scene.text(
            [page.views.longitudinal[0], page.views.plan[0]],
            "Geometry N/C - no supplied surface or linework",
            "layout",
        );
        return Ok(());
    };
    scene.scale_denominator = 1000. / frame.scale;
    orthographic(scene, sheet, geometry, &frame);
    datums(scene, geometry, page, &frame);
    super::charts::area(scene, input, geometry, page, &frame)?;
    sections::draw(scene, input, geometry, page, &frame)?;
    callouts(scene, input, page, &frame)?;
    scene.text(
        [page.views.longitudinal[0], page.views.plan[0] - 10.],
        "PLAN / vehicle datum",
        "layout",
    );
    scene.text(
        [page.views.longitudinal[0], page.views.side],
        "SIDE / common station axis",
        "layout",
    );
    scene.text(
        [
            page.views.front[0] - page.views.front[2] * 0.5,
            page.views.front[1] + 15.,
        ],
        "FRONT / same physical scale",
        "layout",
    );
    if !geometry
        .paths
        .iter()
        .any(|p| matches!(p.role, PathRole::Structure | PathRole::SectionMaterial))
    {
        scene.text(
            [
                page.views.longitudinal[0],
                page.views.plan[0] + page.views.plan[1] * 0.5 + 6.,
            ],
            "Structural members N/C - occupancy is not verified structure",
            "layout",
        );
    }
    Ok(())
}

pub(super) fn orthographic(
    scene: &mut Scene,
    sheet: &Sheet,
    geometry: &DrawingGeometry,
    frame: &Frame,
) {
    let origins = [
        (0, [frame.x, frame.plan]),
        (1, [frame.x, frame.side]),
        (2, frame.front),
    ];
    for (id, object) in geometry.native.iter().flat_map(|g| &g.objects) {
        for (view, origin) in origins {
            for part in &object.parts {
                scene.path(
                    &format!("{id}:view{view}"),
                    hull(
                        part.vertices
                            .iter()
                            .map(|p| project(*p, view, origin, frame.scale))
                            .collect(),
                    ),
                    true,
                    Stroke::Outline,
                    &format!("geometry:{id}"),
                );
            }
        }
    }
    for path in &geometry.paths {
        for (view, origin) in origins {
            scene.path(
                &format!("{}:view{view}", path.id),
                path.points
                    .iter()
                    .map(|p| project(*p, view, origin, frame.scale))
                    .collect(),
                path.closed,
                stroke(path.role),
                &format!("path:{}", path.id),
            );
        }
    }
    occupancy(scene, sheet, geometry, &origins, frame);
}

fn stroke(role: PathRole) -> Stroke {
    match role {
        PathRole::Outline => Stroke::Outline,
        PathRole::Structure | PathRole::SectionMaterial => Stroke::Structure,
        PathRole::Occupancy => Stroke::Envelope,
        PathRole::Hidden => Stroke::Hidden,
    }
}

fn occupancy(
    scene: &mut Scene,
    sheet: &Sheet,
    geometry: &DrawingGeometry,
    origins: &[(usize, [f64; 2]); 3],
    frame: &Frame,
) {
    for (id, envelope) in geometry.envelopes.iter().flat_map(|r| &r.envelopes) {
        if sheet
            .model
            .geometry
            .get(id)
            .is_some_and(|g| g.shape.is_some())
        {
            continue;
        }
        for (view, origin) in origins {
            let points = (0..8).map(|i| {
                std::array::from_fn(|axis| {
                    if i & (1 << axis) == 0 {
                        envelope.minimum_m[axis]
                    } else {
                        envelope.maximum_m[axis]
                    }
                })
            });
            scene.path(
                &format!("{id}:envelope{view}"),
                hull(
                    points
                        .map(|p| project(p, *view, *origin, frame.scale))
                        .collect(),
                ),
                true,
                Stroke::Envelope,
                &format!("geometry:{id}"),
            );
        }
    }
}

fn datums(scene: &mut Scene, geometry: &DrawingGeometry, page: &Page, frame: &Frame) {
    let Some([lo, hi]) = geometry.bounds else {
        return;
    };
    let first = frame.x + lo[0] * frame.scale;
    let last = frame.x + hi[0] * frame.scale;
    for y in [frame.plan, frame.side] {
        scene.line(
            [first - 3., y],
            [last + 3., y],
            Stroke::Datum,
            "geometry:extent",
        );
    }
    for i in 0..=4 {
        let station = lo[0] + (hi[0] - lo[0]) * f64::from(i) / 4.;
        let x = frame.x + station * frame.scale;
        scene.line(
            [x, page.views.area[0] + page.views.area[1]],
            [x, page.views.plan[0] + page.views.plan[1] * 0.5],
            Stroke::Datum,
            "geometry:extent",
        );
        scene.text(
            [x - 4., page.views.side - 14.],
            format!("FS {station:.1}"),
            "geometry:extent",
        );
    }
    projection::dimension(
        scene,
        [first, frame.side + 10.],
        [last, frame.side + 10.],
        format!("L {:.2} m", hi[0] - lo[0]),
        "geometry:extent",
    );
    let y0 = frame.plan - hi[1] * frame.scale;
    let y1 = frame.plan - lo[1] * frame.scale;
    projection::dimension(
        scene,
        [last + 6., y0],
        [last + 6., y1],
        format!("b {:.2} m", hi[1] - lo[1]),
        "geometry:extent",
    );
}

fn callouts(
    scene: &mut Scene,
    input: &DrawingInput,
    page: &Page,
    frame: &Frame,
) -> DrawingResult<()> {
    let Some(data) = &input.data else {
        return Ok(());
    };
    for (i, callout) in data.results.callouts.iter().take(3).enumerate() {
        let anchor = imported::position(&callout.anchor, input)?;
        let point = project(anchor, 0, [frame.x, frame.plan], frame.scale);
        let key = format!("callout:{}", callout.id);
        let at = [
            page.views.longitudinal[0] + 2.,
            page.views.plan[0] - page.views.plan[1] * 0.5 + 10. + i as f64 * 24.,
        ];
        scene.path(
            "result-leader",
            vec![point, [at[0] + 48., at[1] - 1.], [at[0], at[1] - 1.]],
            false,
            Stroke::Annotation,
            &key,
        );
        scene.text(
            at,
            super::short(&format!("{} / {}", callout.id, callout.result.label), 36),
            &key,
        );
        let value = imported::value(&callout.result, input)?;
        let (value, margin) = value.split_once("; ").unwrap_or((&value, ""));
        scene.text([at[0], at[1] + 4.], super::short(value, 40), &key);
        scene.text([at[0], at[1] + 8.], super::short(margin, 40), &key);
        if let Some(equation) = &callout.result.equation {
            scene.text([at[0], at[1] + 12.], super::short(equation, 40), &key);
        }
        scene.text(
            [at[0], at[1] + 16.],
            super::short(&imported::source(input, &callout.result.evidence), 45),
            &key,
        );
    }
    if !data.results.callouts.is_empty() {
        scene.text(
            [
                page.views.longitudinal[0],
                page.views.plan[0] + page.views.plan[1] * 0.5 + 10.,
            ],
            format!(
                "{}/{} callouts; full source/conditions/equations retained",
                data.results.callouts.len().min(3),
                data.results.callouts.len()
            ),
            "layout",
        );
    }
    Ok(())
}

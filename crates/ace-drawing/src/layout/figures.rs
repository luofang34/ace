use super::{
    charts::{self, Curve},
    geometry, imported,
};
use crate::{DrawingError, DrawingInput, DrawingResult, FigureContent, FigureSpec, Scene, Stroke};
use ace_sheet::Sheet;

pub(crate) fn compose(
    sheet: &Sheet,
    input: &DrawingInput,
    spec: &FigureSpec,
) -> DrawingResult<Scene> {
    let width = f64::from(spec.width_px) * 0.5;
    let height = f64::from(spec.height_px) * 0.5;
    let mut scene = Scene {
        width_mm: width,
        height_mm: height,
        scale_denominator: 1.,
        elements: vec![],
        evidence: super::evidence(sheet, input),
    };
    scene.label([5., 9.], &spec.title, 6., true, Stroke::Outline, "layout");
    match &spec.content {
        FigureContent::GeneralArrangement => arrangement(&mut scene, sheet, input)?,
        FigureContent::Curves { ids } => curves(&mut scene, input, ids)?,
        FigureContent::Facts { ids } => facts(&mut scene, input, ids)?,
        FigureContent::Diagram { paths, labels } => diagram(&mut scene, paths, labels)?,
    }
    scene.text(
        [5., height - 8.],
        format!(
            "{} | {}",
            spec.id,
            super::short(&spec.reference, (width / 1.7) as usize)
        ),
        "layout",
    );
    scene.text(
        [5., height - 3.],
        format!(
            "REV {} / {} | Source evidence retained; N/C = unavailable",
            input.model.revision,
            super::short(&input.snapshot.model_digest, 12)
        ),
        "layout",
    );
    fit_labels(&mut scene);
    super::validate(&scene)?;
    Ok(scene)
}

fn arrangement(scene: &mut Scene, sheet: &Sheet, input: &DrawingInput) -> DrawingResult<()> {
    let (w, h) = (scene.width_mm, scene.height_mm);
    let geometry = geometry::DrawingGeometry::read(sheet, input)?;
    let Some([low, high]) = geometry.bounds else {
        scene.label(
            [8., h * 0.5],
            "N/C - supply configuration geometry",
            4.5,
            false,
            Stroke::Annotation,
            "layout",
        );
        return Ok(());
    };
    let usable = h - 38.;
    let length = (high[0] - low[0]).max(0.01);
    let span = (high[1] - low[1]).max(0.01);
    let scale = ((w * 0.56 - 15.) / length)
        .min(usable * 0.64 / span)
        .min((w * 0.34 - 10.) / span)
        .min(usable * 0.23 / (high[2] - low[2]).max(0.01));
    let frame = geometry::Frame {
        scale,
        x: w * 0.30 - (low[0] + high[0]) * scale * 0.5,
        plan: 25. + usable * 0.64 + (low[1] + high[1]) * scale * 0.5,
        side: 25. + usable * 0.12 + (low[2] + high[2]) * scale * 0.5,
        front: [
            w * 0.78 - (low[1] + high[1]) * scale * 0.5,
            25. + usable * 0.55 + (low[2] + high[2]) * scale * 0.5,
        ],
    };
    scene.scale_denominator = 1000. / scale;
    geometry::orthographic(scene, sheet, &geometry, &frame);
    for (label, at) in [
        ("SIDE", [8., 25. + usable * 0.12]),
        ("PLAN", [8., 25. + usable * 0.64]),
        ("FRONT", [w * 0.64, 25. + usable * 0.4]),
    ] {
        scene.label(at, label, 4.5, true, Stroke::Annotation, "layout");
    }
    for (i, text) in [
        format!("Length {length:.2} m"),
        format!("Span {span:.2} m"),
        format!("Shared scale 1:{:.0}", scene.scale_denominator),
    ]
    .into_iter()
    .enumerate()
    {
        scene.label(
            [w * 0.64, 25. + i as f64 * 7.],
            text,
            4.5,
            false,
            Stroke::Annotation,
            "geometry:extent",
        );
    }
    Ok(())
}

fn curves(scene: &mut Scene, input: &DrawingInput, ids: &[String]) -> DrawingResult<()> {
    let data = input
        .data
        .as_ref()
        .ok_or_else(|| DrawingError::Invalid("missing figure data".into()))?;
    let Some(first) = ids
        .first()
        .and_then(|id| data.results.curves.iter().find(|c| &c.id == id))
    else {
        scene.text(
            [5., 30.],
            "N/C - supply performance curves with conditions",
            "layout",
        );
        return Ok(());
    };
    let x_unit = charts::time_axis(first.role, first.x_axis.unit);
    let mut curves = Vec::new();
    for id in ids {
        if let Some(curve) = data.results.curves.iter().find(|c| &c.id == id) {
            let points = curve
                .points
                .iter()
                .map(|p| {
                    Ok(crate::data::resolve(&p.x, x_unit, input)?
                        .zip(crate::data::resolve(&p.y, first.y_axis.unit, input)?)
                        .map(|(x, y)| [x, y]))
                })
                .collect::<DrawingResult<Option<Vec<_>>>>()?
                .unwrap_or_default();
            curves.push(Curve {
                id: id.clone(),
                points,
                evidence: format!("curve:{id}"),
                envelope: curve.role == crate::CurveRole::CgEnvelope,
            });
        }
    }
    charts::display_units(scene, &curves, [x_unit, first.y_axis.unit]);
    charts::plot(
        scene,
        &curves,
        [5., 17., scene.width_mm - 10., scene.height_mm - 35.],
        &first.title,
        [
            &format!("{} / {}", first.x_axis.label, imported::unit(x_unit)),
            &format!(
                "{} / {}",
                first.y_axis.label,
                imported::unit(first.y_axis.unit)
            ),
        ],
    );
    Ok(())
}

fn facts(scene: &mut Scene, input: &DrawingInput, ids: &[String]) -> DrawingResult<()> {
    let data = input
        .data
        .as_ref()
        .ok_or_else(|| DrawingError::Invalid("missing figure data".into()))?;
    let mut y = 19.;
    for id in ids {
        if let Some(fact) = data.facts.iter().find(|f| &f.id == id) {
            let source = &data.evidence[&fact.evidence];
            let text = format!(
                "{}: {} | {:?} / {:?}",
                fact.label,
                fact.value.as_deref().unwrap_or("N/C"),
                source.kind,
                source.maturity
            );
            y = wrapped(
                scene,
                [5., y],
                scene.width_mm - 10.,
                &text,
                &format!("fact:{id}"),
            )? + 3.;
        }
    }
    if ids.is_empty() {
        scene.text(
            [5., 30.],
            "N/C - supply documentary fields and evidence",
            "layout",
        );
    }
    Ok(())
}

fn diagram(
    scene: &mut Scene,
    paths: &[crate::DiagramPath],
    labels: &[crate::DiagramLabel],
) -> DrawingResult<()> {
    let (w, h) = (scene.width_mm - 16., scene.height_mm - 40.);
    let point = |p: [f64; 2]| [8. + w * p[0], 17. + h * p[1]];
    for path in paths {
        scene.path(
            "supplied-diagram",
            path.points.iter().copied().map(point).collect(),
            path.closed,
            Stroke::Outline,
            &format!("source:{}", path.evidence),
        );
    }
    for label in labels {
        wrapped(
            scene,
            point(label.at),
            (1. - label.at[0]) * w,
            &label.text,
            &format!("source:{}", label.evidence),
        )?;
    }
    if paths.is_empty() && labels.is_empty() {
        scene.text(
            [5., 30.],
            "N/C - supply diagram boundaries, connections and labels",
            "layout",
        );
    }
    Ok(())
}

fn wrapped(
    scene: &mut Scene,
    at: [f64; 2],
    width: f64,
    text: &str,
    evidence: &str,
) -> DrawingResult<f64> {
    let count = (width / 2.7).floor() as usize;
    if count < 5 {
        return Err(DrawingError::Invalid(
            "diagram label has insufficient space".into(),
        ));
    }
    let mut line = String::new();
    let mut y = at[1];
    for word in text.split_whitespace() {
        if word.len() > count {
            return Err(DrawingError::Invalid(
                "figure word exceeds available label width".into(),
            ));
        }
        if line.len() + word.len() + 1 > count {
            scene.label([at[0], y], &line, 4.5, false, Stroke::Annotation, evidence);
            line.clear();
            y += 6.;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if y > scene.height_mm - 17. {
        return Err(DrawingError::Invalid(
            "figure content exceeds requested height; increase dimensions or split fields".into(),
        ));
    }
    scene.label([at[0], y], line, 4.5, false, Stroke::Annotation, evidence);
    Ok(y + 6.)
}

fn fit_labels(scene: &mut Scene) {
    for element in &mut scene.elements {
        if let crate::Primitive::Text { text, size, at, .. } = &mut element.primitive {
            let fitting = (scene.width_mm - 5. - at[0]) / (text.len().max(1) as f64 * 0.58);
            *size = size.max(3.8).min(fitting);
        }
    }
}

use super::{charts, geometry, statements};
use crate::{Branding, DrawingInput, DrawingResult, Paper, Primitive, Scene, Stroke};
use ace_sheet::Sheet;

pub(super) struct Views {
    pub longitudinal: [f64; 2],
    pub plan: [f64; 2],
    pub side: f64,
    pub area: [f64; 2],
    pub front: [f64; 3],
    pub sections: [f64; 4],
}

pub(super) struct Page {
    pub width: f64,
    pub height: f64,
    pub footer: f64,
    pub left: [f64; 2],
    pub right: [f64; 2],
    pub views: Views,
}

impl Page {
    fn new(paper: Paper) -> Self {
        if matches!(paper, Paper::Widescreen) {
            Self {
                width: 480.,
                height: 270.,
                footer: 252.,
                left: [10., 76.],
                right: [374., 96.],
                views: Views {
                    longitudinal: [98., 266.],
                    plan: [143., 126.],
                    side: 64.,
                    area: [28., 23.],
                    front: [196., 232., 160.],
                    sections: [288., 215., 76., 31.],
                },
            }
        } else {
            Self {
                width: 420.,
                height: 297.,
                footer: 279.,
                left: [10., 76.],
                right: [340., 70.],
                views: Views {
                    longitudinal: [98., 234.],
                    plan: [156., 140.],
                    side: 73.,
                    area: [29., 28.],
                    front: [170., 259., 140.],
                    sections: [246., 242., 86., 31.],
                },
            }
        }
    }
}

pub(super) fn compose(
    sheet: &Sheet,
    input: &DrawingInput,
    paper: Paper,
    branding: &Branding,
) -> DrawingResult<Scene> {
    let page = Page::new(paper);
    let mut scene = Scene {
        width_mm: page.width,
        height_mm: page.height,
        scale_denominator: 1.,
        elements: Vec::new(),
        evidence: super::evidence(sheet, input),
    };
    let geometry = geometry::DrawingGeometry::read(sheet, input)?;
    geometry::draw(&mut scene, sheet, input, &geometry, &page)?;
    charts::draw(&mut scene, sheet, input, &page)?;
    statements::draw(&mut scene, sheet, input, &page)?;
    frame(&mut scene, sheet, input, &page, branding);
    if matches!(paper, Paper::A1) {
        enlarge(&mut scene);
    }
    super::validate(&scene)?;
    Ok(scene)
}

fn frame(scene: &mut Scene, sheet: &Sheet, input: &DrawingInput, page: &Page, branding: &Branding) {
    scene.path(
        "drawing-border",
        vec![
            [6., 6.],
            [page.width - 6., 6.],
            [page.width - 6., page.height - 6.],
            [6., page.height - 6.],
        ],
        true,
        Stroke::Outline,
        "layout",
    );
    let heading = branding.heading();
    scene.label(
        [10., 14.],
        &heading,
        4.0_f64.min((page.width - 160.) / (heading.len() as f64 * 1.02)),
        true,
        Stroke::Outline,
        "layout",
    );
    scene.text(
        [10., 20.],
        super::short(&format!("{} | {}", sheet.model.name, input.scenario), 100),
        "layout",
    );
    scene.label(
        [page.width - 130., 13.],
        format!("EXECUTION {:?}", sheet.assessment.execution),
        2.6,
        true,
        Stroke::Annotation,
        "layout",
    );
    scene.text(
        [page.width - 130., 19.],
        format!(
            "FEASIBILITY {:?} / {:?}",
            sheet.assessment.feasibility, sheet.assessment.evidence
        ),
        "layout",
    );
    scene.line([6., 24.], [page.width - 6., 24.], Stroke::Outline, "layout");
    scene.line(
        [6., page.footer],
        [page.width - 6., page.footer],
        Stroke::Outline,
        "layout",
    );
    scene.text(
        [10., page.footer + 5.],
        format!(
            "REV {} | MODEL {} | BUILD {}",
            sheet.revision,
            super::short(&sheet.snapshot.model_digest, 12),
            super::short(&sheet.snapshot.build.source_digest, 12)
        ),
        "layout",
    );
    scene.text(
        [10., page.footer + 10.],
        "Units on axes; vehicle x aft / y right / z up | evidence register: drawing.json",
        "layout",
    );
    scene.text(
        [page.width - 142., page.footer + 5.],
        format!(
            "GENERAL ARRANGEMENT | Main 1:{:.1} | Sheet 1/1",
            scene.scale_denominator
        ),
        "layout",
    );
    scene.text(
        [page.width - 142., page.footer + 10.],
        "DRAFT - supplied evidence governs; N/C = unavailable",
        "layout",
    );
}

fn enlarge(scene: &mut Scene) {
    scene.width_mm = 841.;
    scene.height_mm = 594.;
    scene.scale_denominator *= 0.5;
    for element in &mut scene.elements {
        match &mut element.primitive {
            Primitive::Path { points, .. } => {
                for p in points {
                    p[0] *= 2.;
                    p[1] *= 2.;
                }
            }
            Primitive::Text { at, size, .. } => {
                at[0] *= 2.;
                at[1] *= 2.;
                *size *= 2.;
            }
        }
    }
    for element in &mut scene.elements {
        if let Primitive::Text { text, .. } = &mut element.primitive
            && text.starts_with("GENERAL ARRANGEMENT | Main")
        {
            *text = format!(
                "GENERAL ARRANGEMENT | Main 1:{:.1} | Sheet 1/1",
                scene.scale_denominator
            );
        }
        if (element.evidence == "geometry:extent" || element.evidence.starts_with("cut:"))
            && let Primitive::Text { text, .. } = &mut element.primitive
            && let Some(scale) = text.strip_prefix("1:").and_then(|s| s.parse::<f64>().ok())
        {
            *text = format!("1:{:.1}", scale * 0.5);
        }
    }
}

pub(super) fn heading(scene: &mut Scene, at: [f64; 2], width: f64, text: &str, evidence: &str) {
    scene.label(at, text, 2.8, true, Stroke::Outline, evidence);
    scene.line(
        [at[0], at[1] + 1.5],
        [at[0] + width, at[1] + 1.5],
        Stroke::Annotation,
        "layout",
    );
}

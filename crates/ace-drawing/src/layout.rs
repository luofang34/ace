use crate::{DrawingError, DrawingInput, DrawingResult, Evidence, Paper, Primitive, Scene, Stroke};
use ace_sheet::{DomainPanel, Sheet};
use std::collections::BTreeMap;

mod charts;
mod geometry;
mod statements;

pub(crate) fn compose(
    sheet: &Sheet,
    input: &DrawingInput,
    paper: Paper,
    branding: &crate::Branding,
) -> DrawingResult<Scene> {
    let mut scene = Scene {
        width_mm: 420.0,
        height_mm: 297.0,
        scale_denominator: 300.0,
        elements: Vec::new(),
        evidence: evidence(sheet, input),
    };
    geometry::draw(&mut scene, sheet)?;
    statements::draw(&mut scene, sheet, input)?;
    charts::draw(&mut scene, sheet, input)?;
    title(&mut scene, sheet, input, paper, branding);
    if matches!(paper, Paper::A1) {
        scene.width_mm = 841.0;
        scene.height_mm = 594.0;
        scene.scale_denominator *= 0.5;
        for element in &mut scene.elements {
            match &mut element.primitive {
                Primitive::Path { points, .. } => {
                    for p in points {
                        p[0] *= 2.0;
                        p[1] *= 2.0;
                    }
                }
                Primitive::Text { at, size, text, .. } => {
                    *text = text
                        .replace("1:200 (A3)", "1:100 (A1)")
                        .replace("1:100 (A3)", "1:50 (A1)");
                    at[0] *= 2.0;
                    at[1] *= 2.0;
                    *size *= 2.0;
                }
            }
        }
    }
    validate(&scene)?;
    Ok(scene)
}

fn evidence(sheet: &Sheet, input: &DrawingInput) -> BTreeMap<String, Evidence> {
    let base = Evidence {
        model_digest: sheet.snapshot.model_digest.clone(),
        revision: sheet.model.revision,
        scenario: input.scenario.clone(),
        run_id: None,
        source: "drawing layout".into(),
        quality: "annotation".into(),
    };
    let mut result = BTreeMap::from([("layout".into(), base.clone())]);
    for (id, value) in &sheet.snapshot.values {
        result.insert(
            format!("parameter:{id}"),
            Evidence {
                source: id.clone(),
                run_id: value.run_id.clone(),
                quality: format!("{:?}", value.status),
                ..base.clone()
            },
        );
    }
    for (id, margin) in &sheet.margins {
        result.insert(
            format!("requirement:{id}"),
            Evidence {
                source: id.clone(),
                run_id: margin.run_id.clone(),
                quality: format!("{:?}; provisional={}", margin.status, margin.provisional),
                ..base.clone()
            },
        );
    }
    for panel in &sheet.panels {
        result.insert(
            format!("analysis:{}", panel.analysis),
            Evidence {
                source: panel.analysis.clone(),
                run_id: Some(panel.run_id.clone()),
                quality: format!("{:?}; historical={}", panel.status, panel.historical),
                ..base.clone()
            },
        );
    }
    let geometry = panel(sheet, "vehicle.concept_geometry", None);
    for (id, object) in &sheet.model.geometry {
        result.insert(
            format!("geometry:{id}"),
            Evidence {
                source: id.clone(),
                run_id: geometry
                    .filter(|_| object.shape.is_some())
                    .map(|p| p.run_id.clone()),
                quality: if object.shape.is_some() {
                    "concept surface"
                } else {
                    "declared occupancy envelope"
                }
                .into(),
                ..base.clone()
            },
        );
    }
    study_evidence(&mut result, input, &base);
    result
}

fn study_evidence(result: &mut BTreeMap<String, Evidence>, input: &DrawingInput, base: &Evidence) {
    if let Some(study) = &input.study {
        result.insert(
            "study".into(),
            Evidence {
                source: study.id.clone(),
                quality: "bounded discrete study; no performance boundary".into(),
                ..base.clone()
            },
        );
        for candidate in &study.candidates {
            result.insert(
                format!("candidate:{}", candidate.id),
                Evidence {
                    model_digest: candidate.state.model_digest.clone(),
                    revision: candidate.model.revision,
                    scenario: format!("study:{}", study.id),
                    run_id: None,
                    source: candidate.id.clone(),
                    quality: format!("{:?}; discrete sample", candidate.feasibility),
                },
            );
        }
    }
}

fn title(
    scene: &mut Scene,
    sheet: &Sheet,
    input: &DrawingInput,
    paper: Paper,
    branding: &crate::Branding,
) {
    let heading = branding.heading();
    // Bound every glyph by one em so wide customer names cannot overlap status text.
    let size = 5.0_f64.min(280.0 / (heading.len() as f64 * 1.02));
    scene.label([8.0, 14.0], heading, size, true, Stroke::Outline, "layout");
    scene.text(
        [8.0, 20.0],
        format!(
            "{} | {} | CONCEPT - NOT FOR MANUFACTURE",
            short(&sheet.model.name, 48),
            input.scenario
        ),
        "layout",
    );
    scene.label(
        [300.0, 12.0],
        format!("EXECUTION: {:?}", sheet.assessment.execution),
        2.8,
        true,
        Stroke::Outline,
        "layout",
    );
    let stroke = if sheet.assessment.feasibility == ace_engine::Feasibility::Violated {
        Stroke::Failure
    } else {
        Stroke::Data
    };
    scene.label(
        [300.0, 17.0],
        format!("FEASIBILITY: {:?}", sheet.assessment.feasibility),
        2.8,
        true,
        stroke,
        "layout",
    );
    scene.text(
        [300.0, 22.0],
        format!("Evidence: {:?}", sheet.assessment.evidence),
        "layout",
    );
    scene.line([8.0, 25.0], [412.0, 25.0], Stroke::Outline, "layout");
    footer(scene, sheet, input, paper);
}

fn footer(scene: &mut Scene, sheet: &Sheet, input: &DrawingInput, paper: Paper) {
    scene.line([8.0, 282.0], [412.0, 282.0], Stroke::Outline, "layout");
    scene.text(
        [8.0, 287.0],
        format!(
            "REV {} | MODEL {} | BUILD {}",
            sheet.revision,
            short(&sheet.snapshot.model_digest, 12),
            short(&sheet.snapshot.build.source_digest, 12)
        ),
        "layout",
    );
    scene.text(
        [8.0, 292.0],
        evidence_index(sheet, &input.scenario),
        "layout",
    );
    scene.text(
        [270.0, 287.0],
        format!(
            "Selected: {}",
            input
                .candidate
                .as_deref()
                .map(|s| short(s, 14))
                .unwrap_or_else(|| "supplied design".into())
        ),
        "layout",
    );
    scene.text(
        [270.0, 292.0],
        match paper {
            Paper::A3 => "A3 420 x 297 mm | Main 1:300 | Sheet 1/1",
            Paper::A1 => "A1 841 x 594 mm | Main 1:150 | Sheet 1/1",
        },
        "layout",
    );
}

pub(super) fn panel<'a>(
    sheet: &'a Sheet,
    plugin: &str,
    scenario: Option<&str>,
) -> Option<&'a DomainPanel> {
    sheet.panels.iter().find(|p| {
        p.plugin == plugin
            && !p.historical
            && scenario
                .is_none_or(|name| p.details.get("mission").and_then(|v| v.as_str()) == Some(name))
    })
}

pub(super) fn short(s: &str, count: usize) -> String {
    s.chars().take(count).collect()
}

pub(crate) fn validate(scene: &Scene) -> DrawingResult<()> {
    for element in &scene.elements {
        if !scene.evidence.contains_key(&element.evidence) {
            return Err(DrawingError::Invalid(format!(
                "missing evidence {}",
                element.evidence
            )));
        }
        let check = |p: &[f64; 2]| {
            p.iter().all(|v| v.is_finite())
                && p[0] >= 2.0
                && p[0] <= scene.width_mm - 2.0
                && p[1] >= 2.0
                && p[1] <= scene.height_mm - 2.0
        };
        match &element.primitive {
            Primitive::Path { points, .. } if points.iter().any(|p| !check(p)) => {
                return Err(DrawingError::Invalid(format!(
                    "{} lies outside paper; use a suitable configuration",
                    element.id
                )));
            }
            Primitive::Text { at, text, size, .. }
                if !check(at)
                    || *size < 2.5
                    || !text.is_ascii()
                    || text.contains('\n')
                    || at[0] + text.len() as f64 * size * 0.58 > scene.width_mm - 2.0 =>
            {
                return Err(DrawingError::Invalid(format!(
                    "unreadable or out-of-page label: {text}"
                )));
            }
            _ => (),
        }
    }
    Ok(())
}

fn evidence_index(sheet: &Sheet, scenario: &str) -> String {
    let plugins = [
        "vehicle.concept_geometry",
        "vehicle.mission_sequence",
        "vehicle.mission_cg_envelope",
        "vehicle.operations",
    ];
    plugins
        .iter()
        .enumerate()
        .map(|(i, plugin)| {
            format!(
                "E{} {}",
                i + 1,
                panel(
                    sheet,
                    plugin,
                    plugin.contains("mission").then_some(scenario)
                )
                .map(|p| short(&p.run_id, 10))
                .unwrap_or_else(|| "N/C".into())
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

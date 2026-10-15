use crate::{DrawingError, DrawingInput, DrawingResult, Evidence, Paper, Primitive, Scene};
use ace_sheet::{DomainPanel, Sheet};
use std::collections::BTreeMap;

mod arrangement;
mod charts;
mod figures;
mod geometry;
mod imported;
mod statements;
pub(crate) use figures::compose as figure;

pub(crate) fn compose(
    sheet: &Sheet,
    input: &DrawingInput,
    paper: Paper,
    branding: &crate::Branding,
) -> DrawingResult<Scene> {
    arrangement::compose(sheet, input, paper, branding)
}

fn evidence(sheet: &Sheet, input: &DrawingInput) -> BTreeMap<String, Evidence> {
    let base = Evidence {
        model_digest: sheet.snapshot.model_digest.clone(),
        revision: sheet.model.revision,
        scenario: input.scenario.clone(),
        run_id: None,
        source: "drawing layout".into(),
        quality: "annotation".into(),
        units: Vec::new(),
        imported: None,
    };
    let mut result = BTreeMap::from([("layout".into(), base.clone())]);
    result.insert(
        "geometry:extent".into(),
        Evidence {
            source: "derived extents/stations of rendered geometry in the frozen input".into(),
            quality: "derived geometry; not structural validation".into(),
            units: vec![ace_model::Unit::Meter],
            ..base.clone()
        },
    );
    for (id, item) in &input.model.mass_items {
        let run_id = item
            .mass
            .parameter()
            .and_then(|id| sheet.snapshot.values.get(id))
            .and_then(|v| v.run_id.clone());
        result.insert(
            format!("mass:{id}"),
            Evidence {
                source: format!("mass_items.{id}: {}", item.source.reference),
                run_id,
                units: vec![ace_model::Unit::Kilogram],
                quality: item.source.kind.clone(),
                ..base.clone()
            },
        );
    }
    result.insert(
        "mass:total".into(),
        Evidence {
            source: "sum of current mass item bindings; unavailable if any contribution is missing"
                .into(),
            units: vec![ace_model::Unit::Kilogram],
            quality: "derived mass sum".into(),
            ..base.clone()
        },
    );
    result.insert(
        "operations:basis".into(),
        Evidence {
            source: "canonical operations economic basis".into(),
            ..base.clone()
        },
    );
    computed_evidence(&mut result, sheet, input, &base);
    study_evidence(&mut result, input, &base);
    imported::evidence(&mut result, input, &base);
    result
}

fn computed_evidence(
    result: &mut BTreeMap<String, Evidence>,
    sheet: &Sheet,
    input: &DrawingInput,
    base: &Evidence,
) {
    for (id, value) in &sheet.snapshot.values {
        result.insert(
            format!("parameter:{id}"),
            Evidence {
                source: id.clone(),
                run_id: value.run_id.clone(),
                quality: format!("{:?}", value.status),
                units: input
                    .model
                    .parameters
                    .get(id)
                    .map(|p| vec![p.unit])
                    .unwrap_or_default(),
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
                units: match panel.plugin.as_str() {
                    "vehicle.mission_cg_envelope" => {
                        vec![ace_model::Unit::Meter, ace_model::Unit::Kilogram]
                    }
                    "vehicle.mission_sequence" => {
                        vec![ace_model::Unit::Second, ace_model::Unit::Kilogram]
                    }
                    _ => Vec::new(),
                },
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
                    units: Vec::new(),
                    imported: None,
                },
            );
        }
    }
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
    if s.chars().count() > count {
        format!(
            "{}...",
            s.chars().take(count.saturating_sub(3)).collect::<String>()
        )
    } else {
        s.into()
    }
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

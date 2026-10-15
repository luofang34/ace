use crate::domain::diagnostic::{AexError, AexResult};
use ace_drawing::{DrawingDocument, FigureContent, Primitive};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize)]
pub(super) struct Manifest<'a> {
    schema: &'static str,
    figures: Vec<Figure<'a>>,
}

#[derive(Serialize)]
struct Figure<'a> {
    id: &'a str,
    reference: &'a str,
    files: Vec<String>,
    revision: u64,
    model_digest: &'a str,
    drawing_digest: &'a str,
    target_display_px: Option<[u32; 2]>,
    png_minimum_px: Option<[u32; 2]>,
    evidence: &'a BTreeMap<String, ace_drawing::Evidence>,
    units: BTreeSet<String>,
    missing_inputs: Vec<String>,
    presentation_complete: bool,
    assessment: ace_engine::Assessment,
}

pub(super) fn build(documents: &[(String, DrawingDocument)]) -> AexResult<Manifest<'_>> {
    let mut figures = Vec::new();
    for (stem, document) in documents {
        let missing_inputs = missing(document);
        let spec = document.figure.as_ref();
        let target = spec.map(|f| [f.width_px, f.height_px]);
        let assessment = ace_engine::assess(&document.input.model, &document.input.snapshot)?;
        let mut units = BTreeSet::new();
        for unit in document.scene.evidence.values().flat_map(|e| &e.units) {
            units.insert(serde_json::to_string(unit).map_err(|source| AexError::Json { source })?);
        }
        figures.push(Figure {
            id: stem,
            reference: spec.map_or("synthesis sheet", |f| &f.reference),
            files: ["svg", "pdf", "png", "json"]
                .map(|ext| format!("{stem}.{ext}"))
                .into(),
            revision: document.input.model.revision,
            model_digest: &document.input.snapshot.model_digest,
            drawing_digest: &document.digest,
            target_display_px: target,
            png_minimum_px: target.map(|[w, h]| [w * 4, h * 4]),
            evidence: &document.scene.evidence,
            units,
            presentation_complete: missing_inputs.is_empty(),
            missing_inputs,
            assessment,
        });
    }
    Ok(Manifest {
        schema: "ace.drawing-export/1",
        figures,
    })
}

fn missing(document: &DrawingDocument) -> Vec<String> {
    let mut missing: BTreeSet<String> = document
        .scene
        .elements
        .iter()
        .filter_map(|e| match &e.primitive {
            Primitive::Text { text, .. } if text.contains("N/C") && !text.contains("N/C =") => {
                Some(text.clone())
            }
            _ => None,
        })
        .collect();
    let data = document.input.data.as_ref();
    if document.figure.is_none()
        && data.is_none_or(|d| {
            d.geometry.paths.iter().all(|p| {
                !matches!(
                    p.role,
                    ace_drawing::PathRole::Structure | ace_drawing::PathRole::SectionMaterial
                )
            })
        })
    {
        missing.insert("Supplied structural geometry and verification evidence".into());
    }
    if let Some(spec) = &document.figure {
        match &spec.content {
            FigureContent::Facts { ids } if ids.is_empty() => {
                missing.insert("Documentary fields and evidence".into());
            }
            FigureContent::Curves { ids } if ids.is_empty() => {
                missing.insert("Performance curves with units and conditions".into());
            }
            FigureContent::Diagram { paths, labels } if paths.is_empty() && labels.is_empty() => {
                missing.insert("Author-supplied diagram".into());
            }
            _ => (),
        }
    }
    missing.into_iter().collect()
}

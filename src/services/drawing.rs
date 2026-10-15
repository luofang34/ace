use crate::domain::diagnostic::{AexError, AexResult};
use ace_drawing::{DrawingDocument, DrawingInput, Paper};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

mod export;
#[cfg(test)]
mod tests;

pub(crate) struct DrawingOptions {
    pub paper: Paper,
    pub plan: Option<PathBuf>,
    pub candidate: Option<usize>,
    pub scenario: String,
    pub replay: bool,
    pub branding: Option<ace_drawing::Branding>,
}

#[derive(Serialize)]
pub(crate) struct DrawingOutputs {
    directory: PathBuf,
    model_digest: String,
    drawing_digest: String,
    revision: u64,
    files: Vec<&'static str>,
}

pub(crate) fn run_blocking(
    source: &Path,
    directory: &Path,
    overrides: &BTreeMap<String, String>,
    options: DrawingOptions,
) -> AexResult<DrawingOutputs> {
    let document = if options.replay {
        if !overrides.is_empty()
            || options.plan.is_some()
            || options.candidate.is_some()
            || options.branding.is_some()
        {
            return Err(AexError::validation(
                "DRAWING_REPLAY",
                "options",
                "replay preserves frozen inputs and paper; edits require a new computation",
            ));
        }
        let bytes = std::fs::read(source).map_err(|source_error| AexError::Read {
            path: source.into(),
            source: source_error,
        })?;
        let document: DrawingDocument =
            serde_json::from_slice(&bytes).map_err(|source| AexError::Json { source })?;
        document.verify().map_err(drawing_error)?;
        document
    } else {
        ace_drawing::compose_branded(
            calculate_blocking(source, overrides, &options)?,
            options.paper,
            options.branding.clone().unwrap_or_default(),
        )
        .map_err(drawing_error)?
    };
    export::write_blocking(&document, directory)?;
    Ok(DrawingOutputs {
        directory: directory.into(),
        model_digest: document.input.snapshot.model_digest.clone(),
        drawing_digest: document.digest,
        revision: document.input.model.revision,
        files: vec!["drawing.json", "drawing.svg", "drawing.pdf", "drawing.png"],
    })
}

fn calculate_blocking(
    source: &Path,
    overrides: &BTreeMap<String, String>,
    options: &DrawingOptions,
) -> AexResult<DrawingInput> {
    let mut session = super::synthesis::open_blocking(source, overrides)?;
    if let Some(path) = &options.plan {
        let content = std::fs::read_to_string(path).map_err(|source| AexError::Read {
            path: path.clone(),
            source,
        })?;
        let plan: ace_study::StudyPlan =
            serde_yaml::from_str(&content).map_err(|source| AexError::Yaml {
                path: path.clone(),
                source,
            })?;
        let study = ace_study::run(&session, &plan)?;
        let index = options.candidate.ok_or_else(|| {
            AexError::validation(
                "DRAWING_SELECTION",
                "candidate",
                "explicit candidate index required with --plan",
            )
        })?;
        let candidate = study.candidates.get(index).ok_or_else(|| {
            AexError::validation(
                "DRAWING_SELECTION",
                "candidate",
                format!("index {index}; only {} candidates", study.candidates.len()),
            )
        })?;
        let model = candidate.model.clone();
        let id = candidate.id.clone();
        let snapshot = study.snapshot(&id)?;
        Ok(DrawingInput {
            model,
            snapshot,
            scenario: options.scenario.clone(),
            study: Some(study),
            candidate: Some(id),
        })
    } else {
        if options.candidate.is_some() {
            return Err(AexError::validation(
                "DRAWING_SELECTION",
                "candidate",
                "--candidate requires --plan",
            ));
        }
        let snapshot = session.run()?;
        Ok(DrawingInput {
            model: session.model().clone(),
            snapshot,
            scenario: options.scenario.clone(),
            study: None,
            candidate: None,
        })
    }
}

fn drawing_error(source: ace_drawing::DrawingError) -> AexError {
    AexError::Service {
        operation: "compose synthesis drawing",
        source: Box::new(source),
    }
}

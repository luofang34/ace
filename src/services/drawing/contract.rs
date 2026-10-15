use ace_drawing::{DrawingData, DrawingInput};
use std::{collections::BTreeMap, path::Path};

fn input() -> DrawingInput {
    let mut session = super::super::synthesis::open_blocking(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/synthesis/concept-transport.json"),
        &BTreeMap::new(),
    )
    .unwrap();
    let snapshot = session.run().unwrap();
    DrawingInput {
        model: session.model().clone(),
        snapshot,
        scenario: "design".into(),
        study: None,
        candidate: None,
        data: Some(profile()),
    }
}

fn profile() -> DrawingData {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/drawing/neutral-renderer.json"
    )))
    .unwrap()
}

fn texts(document: &ace_drawing::DrawingDocument) -> String {
    document
        .scene
        .elements
        .iter()
        .filter_map(|e| match &e.primitive {
            ace_drawing::Primitive::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

mod tests;

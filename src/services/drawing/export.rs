use crate::domain::diagnostic::{AexError, AexResult};
use ace_drawing::DrawingDocument;
use std::path::Path;

pub(super) fn write_blocking(document: &DrawingDocument, directory: &Path) -> AexResult<()> {
    let svg = document.svg().map_err(super::drawing_error)?;
    let pdf = document.pdf().map_err(super::drawing_error)?;
    let png = png_blocking(&svg)?;
    let json = serde_json::to_vec(document).map_err(|source| AexError::Json { source })?;
    if directory.exists() {
        return Err(AexError::validation(
            "DRAWING_DESTINATION",
            directory.display().to_string(),
            "use a new directory so exports cannot mix generations",
        ));
    }
    let parent = directory
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|source| AexError::Write {
        path: parent.into(),
        source,
    })?;
    let staging = tempfile::Builder::new()
        .prefix(".drawing-")
        .tempdir_in(parent)
        .map_err(|source| AexError::Write {
            path: parent.into(),
            source,
        })?;
    for (name, bytes) in [
        ("drawing.json", json),
        ("drawing.svg", svg.into_bytes()),
        ("drawing.pdf", pdf),
        ("drawing.png", png),
    ] {
        let path = staging.path().join(name);
        std::fs::write(&path, bytes).map_err(|source| AexError::Write { path, source })?;
    }
    std::fs::rename(staging.path(), directory).map_err(|source| AexError::Write {
        path: directory.into(),
        source,
    })?;
    Ok(())
}

fn png_blocking(svg: &str) -> AexResult<Vec<u8>> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &options).map_err(|source| AexError::Service {
        operation: "parse drawing SVG for preview",
        source: Box::new(source),
    })?;
    let scale = 200.0 / 96.0;
    let width = (tree.size().width() * scale).ceil() as u32;
    let height = (tree.size().height() * scale).ceil() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
        AexError::validation("DRAWING_IMAGE", "paper", "cannot allocate preview image")
    })?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|source| AexError::Service {
        operation: "encode drawing PNG",
        source: Box::new(source),
    })
}

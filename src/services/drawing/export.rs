use crate::domain::diagnostic::{AexError, AexResult};
use ace_drawing::DrawingDocument;
use std::path::Path;

pub(super) fn write_blocking(
    document: &DrawingDocument,
    directory: &Path,
) -> AexResult<Vec<String>> {
    let mut documents = vec![("drawing".to_owned(), document.clone())];
    for figure in ace_drawing::compose_figures(document).map_err(super::drawing_error)? {
        let id = figure
            .figure
            .as_ref()
            .map(|f| f.id.clone())
            .ok_or_else(|| {
                AexError::validation("DRAWING_FIGURE", "id", "missing figure identity")
            })?;
        documents.push((id, figure));
    }
    let mut files = Vec::new();
    for (stem, item) in &documents {
        let svg = item.svg().map_err(super::drawing_error)?;
        let pdf = item.pdf().map_err(super::drawing_error)?;
        let target = item
            .figure
            .as_ref()
            .map(|f| [f.width_px * 4, f.height_px * 4]);
        let png = png_blocking(&svg, target)?;
        let json = serde_json::to_vec(item).map_err(|source| AexError::Json { source })?;
        for (extension, bytes) in [
            ("json", json),
            ("svg", svg.into_bytes()),
            ("pdf", pdf),
            ("png", png),
        ] {
            files.push((format!("{stem}.{extension}"), bytes));
        }
    }
    let manifest = super::manifest::build(&documents)?;
    files.push((
        "manifest.json".into(),
        serde_json::to_vec_pretty(&manifest).map_err(|source| AexError::Json { source })?,
    ));
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
    for (name, bytes) in &files {
        let path = staging.path().join(name);
        std::fs::write(&path, bytes).map_err(|source| AexError::Write { path, source })?;
    }
    std::fs::rename(staging.path(), directory).map_err(|source| AexError::Write {
        path: directory.into(),
        source,
    })?;
    Ok(files.into_iter().map(|(name, _)| name).collect())
}

fn png_blocking(svg: &str, target: Option<[u32; 2]>) -> AexResult<Vec<u8>> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &options).map_err(|source| AexError::Service {
        operation: "parse drawing SVG for preview",
        source: Box::new(source),
    })?;
    let scale = target
        .map(|[w, h]| (w as f32 / tree.size().width()).max(h as f32 / tree.size().height()))
        .unwrap_or(200.0 / 96.0);
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

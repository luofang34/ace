use crate::{Primitive, Scene, Stroke};

mod pdf;
pub(crate) use pdf::pdf;

pub(crate) fn svg(scene: &Scene, digest: &str, branding: Option<&crate::Branding>) -> String {
    let title = branding
        .map(|b| b.heading())
        .unwrap_or_else(|| "ACE Aircraft Synthesis Drawing".into());
    let title = xml(&title);
    let mut result = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}mm" height="{}mm" viewBox="0 0 {} {}"><title>{title}</title><desc>Content {}; all source evidence in drawing.json. Conceptual, not for manufacture.</desc><rect width="100%" height="100%" fill="white"/>"#,
        scene.width_mm, scene.height_mm, scene.width_mm, scene.height_mm, digest
    );
    for element in &scene.elements {
        let metadata = format!(
            r#"id="{}" data-evidence="{}""#,
            xml(&element.id),
            xml(&element.evidence)
        );
        match &element.primitive {
            Primitive::Path {
                points,
                closed,
                stroke,
            } => {
                let (color, width, dash) = style(*stroke);
                let points = points
                    .iter()
                    .map(|p| format!("{:.5},{:.5}", p[0], p[1]))
                    .collect::<Vec<_>>()
                    .join(" ");
                let tag = if *closed { "polygon" } else { "polyline" };
                result.push_str(&format!(r#"<{tag} {metadata} points="{points}" fill="none" stroke="{color}" stroke-width="{width}" stroke-dasharray="{dash}" stroke-linejoin="round"/>"#));
            }
            Primitive::Text {
                at,
                text,
                size,
                bold,
                stroke,
            } => {
                let color = style(*stroke).0;
                let weight = if *bold { "bold" } else { "normal" };
                result.push_str(&format!(r#"<text {metadata} x="{:.5}" y="{:.5}" font-family="Helvetica,Arial,sans-serif" font-size="{size}" font-weight="{weight}" fill="{color}">{}</text>"#,at[0],at[1],xml(text)));
            }
        }
    }
    result.push_str("</svg>");
    result
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn style(stroke: Stroke) -> (&'static str, f64, &'static str) {
    match stroke {
        Stroke::Outline => ("#172c36", 0.28, "none"),
        Stroke::Envelope => ("#8b7044", 0.20, "1.2 0.8"),
        Stroke::Datum => ("#9ba6aa", 0.15, "2 0.7 0.3 0.7"),
        Stroke::Data => ("#155e75", 0.30, "none"),
        Stroke::Failure => ("#b33329", 0.28, "none"),
        Stroke::Annotation => ("#34454e", 0.18, "none"),
    }
}

use crate::{Primitive, Scene};

pub(crate) fn pdf(scene: &Scene, digest: &str, branding: Option<&crate::Branding>) -> Vec<u8> {
    let factor = 72.0 / 25.4;
    let mut content = format!(
        "q\n{factor} 0 0 -{factor} 0 {} cm\n1 J 1 j\n",
        scene.height_mm * factor
    );
    for element in &scene.elements {
        content.push_str(&format!(
            "% evidence {}\n",
            element.evidence.replace(['\r', '\n'], " ")
        ));
        match &element.primitive {
            Primitive::Path {
                points,
                closed,
                stroke,
            } => {
                let (hex, width, dash) = super::style(*stroke);
                content.push_str(&paint(hex, true));
                content.push_str(&format!(
                    "{width} w [{}] 0 d\n",
                    if dash == "none" { "" } else { dash }
                ));
                if let Some(first) = points.first() {
                    content.push_str(&format!("{} {} m\n", first[0], first[1]));
                    for p in points.iter().skip(1) {
                        content.push_str(&format!("{} {} l\n", p[0], p[1]));
                    }
                    content.push_str(if *closed { "h S\n" } else { "S\n" });
                }
            }
            Primitive::Text {
                at,
                text,
                size,
                bold,
                stroke,
            } => {
                content.push_str(&paint(super::style(*stroke).0, false));
                let font = if *bold { "F2" } else { "F1" };
                content.push_str(&format!(
                    "BT /{font} {size} Tf 1 0 0 -1 {} {} Tm ({}) Tj ET\n",
                    at[0],
                    at[1],
                    escape(text)
                ));
            }
        }
    }
    content.push_str("Q\n");
    encode(scene, content, digest, branding)
}

fn paint(hex: &str, stroke: bool) -> String {
    let channels = [1..3, 3..5, 5..7]
        .map(|range| u8::from_str_radix(&hex[range], 16).unwrap_or(0) as f64 / 255.0);
    format!(
        "{} {} {} {}\n",
        channels[0],
        channels[1],
        channels[2],
        if stroke { "RG" } else { "rg" }
    )
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn encode(
    scene: &Scene,
    content: String,
    digest: &str,
    branding: Option<&crate::Branding>,
) -> Vec<u8> {
    let factor = 72.0 / 25.4;
    let title = escape(branding.map_or("Aircraft Synthesis Drawing", |b| &b.title));
    let creator = escape(branding.map_or("ACE", |b| &b.organization));
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>",
            scene.width_mm * factor,
            scene.height_mm * factor
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".into(),
        format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
        format!(
            "<< /Title ({title}) /Creator ({creator}) /Subject (drawing sha256:{digest}; evidence in drawing.json) >>"
        ),
    ];
    let mut pdf = if branding.is_some() {
        b"%PDF-1.4\n%Drawing\n".to_vec()
    } else {
        b"%PDF-1.4\n%ACE\n".to_vec()
    };
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let start = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 7 0 R >>\nstartxref\n{start}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

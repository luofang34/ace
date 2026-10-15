use super::*;
use ace_drawing::{Primitive, Stroke};
use ace_model::{Quantity, Source, Unit};

fn session() -> ace_engine::Session {
    super::super::synthesis::open_blocking(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/synthesis/concept-transport.json"),
        &BTreeMap::new(),
    )
    .unwrap()
}

fn document(session: &mut ace_engine::Session, paper: Paper) -> DrawingDocument {
    let snapshot = session.run().unwrap();
    ace_drawing::compose(
        DrawingInput {
            model: session.model().clone(),
            snapshot,
            scenario: "design".into(),
            study: None,
            candidate: None,
        },
        paper,
    )
    .unwrap()
}

fn quantity(document: &DrawingDocument, id: &str) -> f64 {
    document.input.snapshot.values[id].quantity.unwrap().value
}

#[test]
fn calculated_warning_margin_stays_warn_in_scene_and_pdf() {
    let mut session = session();
    let document = document(&mut session, Paper::A3);
    let margins =
        ace_engine::evaluate_requirements(session.model(), &document.input.snapshot).unwrap();
    assert_eq!(margins["gate_span"].status, ace_engine::MarginStatus::Warn);
    assert!(!margins["gate_span"].provisional);
    assert!((margins["gate_span"].absolute.unwrap().value - 1.0).abs() < 1e-12);
    assert!(document.scene.elements.iter().any(|element| {
        element.evidence == "requirement:gate_span"
            && matches!(&element.primitive, Primitive::Text { text, .. } if text == "WARN +1.00 m")
    }));
    let pdf = document.pdf().unwrap();
    assert!(std::str::from_utf8(&pdf).unwrap().contains("WARN +1.00 m"));
}

fn span_mm(document: &DrawingDocument) -> f64 {
    let points: Vec<_> = document
        .scene
        .elements
        .iter()
        .filter(|e| e.id.starts_with("wing:view0"))
        .flat_map(|e| match &e.primitive {
            Primitive::Path { points, .. } => points.clone(),
            _ => Vec::new(),
        })
        .collect();
    points
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max)
        - points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min)
}

#[test]
fn canonical_edits_update_surfaces_mass_trajectory_and_hard_failures_together() {
    let mut session = session();
    let before = document(&mut session, Paper::A3);
    session
        .edit(
            &BTreeMap::from([
                (
                    "wing.span".into(),
                    Quantity::new(37.0, Unit::Meter).unwrap(),
                ),
                (
                    "mission.distance".into(),
                    Quantity::new(2200.0, Unit::NauticalMile).unwrap(),
                ),
                (
                    "tank.offset_x".into(),
                    Quantity::new(0.6, Unit::Meter).unwrap(),
                ),
            ]),
            Source {
                kind: "test".into(),
                reference: "geometry/mission transaction".into(),
                revision: None,
            },
        )
        .unwrap();
    let after = document(&mut session, Paper::A3);
    assert!((span_mm(&before) - 35.0 * 1000.0 / 300.0).abs() < 1e-9);
    assert!((span_mm(&after) - 37.0 * 1000.0 / 300.0).abs() < 1e-9);
    assert_ne!(
        quantity(&before, "fuel.required"),
        quantity(&after, "fuel.required")
    );
    assert_ne!(
        quantity(&before, "loading.cg_x"),
        quantity(&after, "loading.cg_x")
    );
    assert_ne!(
        before.input.snapshot.model_digest,
        after.input.snapshot.model_digest
    );
    let assessment = ace_engine::assess(&after.input.model, &after.input.snapshot).unwrap();
    assert_eq!(assessment.feasibility, ace_engine::Feasibility::Violated);
    assert!(assessment.violations.iter().any(|id| id == "gate_span"));
    assert!(after.scene.elements.iter().any(|e| matches!(
        &e.primitive,
        Primitive::Text {
            stroke: Stroke::Failure,
            ..
        }
    )));
    let before_cg: Vec<_> = before
        .scene
        .elements
        .iter()
        .filter(|e| e.id.starts_with("design-"))
        .collect();
    let after_cg: Vec<_> = after
        .scene
        .elements
        .iter()
        .filter(|e| e.id.starts_with("design-"))
        .collect();
    assert_ne!(
        serde_json::to_value(before_cg).unwrap(),
        serde_json::to_value(after_cg).unwrap()
    );
}

#[test]
fn archive_replay_is_identical_and_rejects_modified_evidence_or_mixed_revision() {
    let mut session = session();
    let document = document(&mut session, Paper::A3);
    let encoded = serde_json::to_vec(&document).unwrap();
    let reopened: DrawingDocument = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(document.svg().unwrap(), reopened.svg().unwrap());
    assert_eq!(document.pdf().unwrap(), reopened.pdf().unwrap());
    let mut altered = reopened.clone();
    altered.scene.scale_denominator = 250.0;
    assert!(altered.verify().is_err());
    let mut mixed = reopened.input;
    mixed.model.revision = mixed.model.revision.wrapping_add(1);
    assert!(ace_drawing::compose(mixed, Paper::A3).is_err());
    let mut altered = document.input;
    altered.snapshot.runs[0].outputs.clear();
    assert!(ace_drawing::compose(altered, Paper::A3).is_err());
}

#[test]
fn paper_sizes_preserve_scale_and_missing_study_remains_explicit() {
    let mut session = session();
    let a3 = document(&mut session, Paper::A3);
    let a1 = document(&mut session, Paper::A1);
    assert!((span_mm(&a1) - 2.0 * span_mm(&a3)).abs() < 1e-9);
    assert_eq!(a1.scene.width_mm, 841.0);
    assert_eq!(a1.scene.scale_denominator, 150.0);
    assert!(a3.svg().unwrap().contains("Study NOT CALCULATED"));
    assert!(
        a3.scene
            .elements
            .iter()
            .all(|e| a3.scene.evidence.contains_key(&e.evidence))
    );
}

#[test]
fn insufficient_capacity_is_exported_as_missing_analysis_not_a_light_optimum() {
    let mut session = session();
    session
        .edit(
            &BTreeMap::from([(
                "mission.distance".into(),
                Quantity::new(3000.0, Unit::NauticalMile).unwrap(),
            )]),
            Source {
                kind: "test".into(),
                reference: "beyond fuel capacity".into(),
                revision: None,
            },
        )
        .unwrap();
    let document = document(&mut session, Paper::A3);
    assert!(!document.input.snapshot.complete());
    assert!(
        document.input.snapshot.values["fuel.required"]
            .quantity
            .is_none()
    );
    let svg = document.svg().unwrap();
    assert!(svg.contains("NOT CLOSED"));
    assert!(svg.contains("Mission NOT CALCULATED"));
    assert!(svg.contains("CG envelope NOT CALCULATED"));
}

#[test]
fn wing_projected_area_matches_independent_trapezoid_geometry() {
    let mut session = session();
    let document = document(&mut session, Paper::A3);
    let mut area = 0.0;
    for e in document
        .scene
        .elements
        .iter()
        .filter(|e| e.id.starts_with("wing:view0"))
    {
        if let Primitive::Path { points, .. } = &e.primitive {
            area += points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
                .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
                .sum::<f64>()
                .abs()
                * 0.5;
        }
    }
    assert!((area - quantity(&document, "wing.area") * (1000.0 / 300.0_f64).powi(2)).abs() < 1e-8);
}

#[test]
fn customer_branding_changes_presentation_only_and_survives_replay() {
    let mut session = session();
    let neutral = document(&mut session, Paper::A3);
    let branded = ace_drawing::compose_branded(
        neutral.input.clone(),
        Paper::A3,
        ace_drawing::Branding {
            organization: "Example Aerospace".into(),
            title: "Transport Concept".into(),
        },
    )
    .unwrap();
    assert_eq!(
        neutral.input.snapshot.model_digest,
        branded.input.snapshot.model_digest
    );
    assert_eq!(
        serde_json::to_value(&neutral.input.snapshot).unwrap(),
        serde_json::to_value(&branded.input.snapshot).unwrap()
    );
    let svg = branded.svg().unwrap();
    assert!(svg.contains("Example Aerospace / Transport Concept"));
    assert!(!svg.contains("ACE"));
    assert!(!neutral.svg().unwrap().contains("ACE"));
    let pdf = branded.pdf().unwrap();
    let text = std::str::from_utf8(&pdf).unwrap();
    assert!(text.contains("/Creator (Example Aerospace)"));
    assert!(!text.contains("ACE"));
    let reopened: DrawingDocument =
        serde_json::from_slice(&serde_json::to_vec(&branded).unwrap()).unwrap();
    assert_eq!(pdf, reopened.pdf().unwrap());
    assert_eq!(svg, reopened.svg().unwrap());
    let mut altered = reopened;
    altered.branding.as_mut().unwrap().organization = "Changed".into();
    assert!(altered.verify().is_err());
    let invalid = ace_drawing::Branding {
        organization: "Unsupported\nHeader".into(),
        title: "Study".into(),
    };
    assert!(ace_drawing::compose_branded(neutral.input, Paper::A3, invalid).is_err());
}

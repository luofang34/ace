use super::*;
use ace_drawing::{
    EvidenceKind, EvidenceScope, Paper, Primitive, Stroke, compose, compose_figures,
};
use ace_model::{Quantity, Source, Unit};

#[test]
fn supplied_structure_is_projected_and_hatched_only_at_its_actual_station() {
    let doc = compose(input(), Paper::A3).unwrap();
    let views: Vec<_> = doc
        .scene
        .elements
        .iter()
        .filter(|e| e.id.starts_with("fixture-frame:view"))
        .collect();
    assert_eq!(views.len(), 3);
    let xs: Vec<_> = views
        .iter()
        .take(2)
        .map(|e| match &e.primitive {
            Primitive::Path { points, .. } => points[0][0],
            _ => unreachable_test(),
        })
        .collect();
    assert!((xs[0] - xs[1]).abs() < 1e-12);
    assert!(
        doc.scene
            .elements
            .iter()
            .any(|e| e.evidence == "path:fixture-frame"
                && matches!(
                    e.primitive,
                    Primitive::Path {
                        stroke: Stroke::Hatch,
                        ..
                    }
                ))
    );
    assert!(texts(&doc).contains("FS 35.00 m"));
    let mut moved = doc.input.clone();
    moved.data.as_mut().unwrap().geometry.sections[0].station =
        ace_model::Binding::Constant(Quantity::new(34., Unit::Meter).unwrap());
    let moved = compose(moved, Paper::A3).unwrap();
    assert!(!moved.scene.elements.iter().any(|e| matches!(
        e.primitive,
        Primitive::Path {
            stroke: Stroke::Hatch,
            ..
        }
    )));
}

fn unreachable_test() -> f64 {
    panic!("expected rendered path")
}

#[test]
fn parameter_edit_moves_bound_geometry_sections_and_results_together() {
    let before = input();
    let mut session =
        crate::services::synthesis::create_session_blocking(before.model.clone(), &BTreeMap::new())
            .unwrap();
    session
        .edit(
            &BTreeMap::from([("wing.span".into(), Quantity::new(37., Unit::Meter).unwrap())]),
            Source {
                kind: "test".into(),
                reference: "drawing binding".into(),
                revision: None,
            },
        )
        .unwrap();
    let snapshot = session.run().unwrap();
    let edited = DrawingInput {
        model: session.model().clone(),
        snapshot,
        ..before
    };
    let doc = compose(edited, Paper::A3).unwrap();
    assert!(texts(&doc).contains("FS 37.00 m"));
    assert!(texts(&doc).contains("37.000 m"));
    let paths: Vec<_> = doc
        .scene
        .elements
        .iter()
        .filter(|e| e.id.starts_with("fixture-frame:view"))
        .collect();
    assert_eq!(paths.len(), 3);
    assert!(
        doc.scene
            .elements
            .iter()
            .any(|e| e.evidence == "requirement:gate_span"
                && matches!(
                    e.primitive,
                    Primitive::Text {
                        stroke: Stroke::Failure,
                        ..
                    }
                ))
    );
}

#[test]
fn imported_claims_require_matching_revision_and_compatible_units() {
    let input = input();
    let mut frozen = input.clone();
    let source = frozen
        .data
        .as_mut()
        .unwrap()
        .evidence
        .get_mut("fixture")
        .unwrap();
    source.kind = EvidenceKind::Simulation;
    source.scope = EvidenceScope::Frozen {
        model_digest: frozen.snapshot.model_digest.clone(),
        revision: frozen.model.revision,
    };
    assert!(compose(frozen.clone(), Paper::A3).is_ok());
    frozen
        .data
        .as_mut()
        .unwrap()
        .evidence
        .get_mut("fixture")
        .unwrap()
        .scope = EvidenceScope::Frozen {
        model_digest: "wrong".into(),
        revision: 0,
    };
    assert!(compose(frozen, Paper::A3).is_err());
    let mut invalid = input;
    invalid.data.as_mut().unwrap().results.statements[0].unit = Unit::Kilogram;
    assert!(compose(invalid, Paper::A3).is_err());
}

#[test]
fn export_identifiers_reject_case_insensitive_file_collisions() {
    let input = input();
    for reserved in ["Drawing", "MANIFEST"] {
        let mut invalid = input.clone();
        invalid.data.as_mut().unwrap().figures[0].id = reserved.into();
        assert!(compose(invalid, Paper::A3).is_err());
    }
    let mut invalid = input;
    invalid.data.as_mut().unwrap().figures[1].id = "ga-EXAMPLE".into();
    assert!(compose(invalid, Paper::A3).is_err());
}

#[test]
fn independent_page_and_figure_layouts_preserve_frozen_inputs_and_unknowns() {
    let input = input();
    let a3 = compose(input.clone(), Paper::A3).unwrap();
    let a1 = compose(input.clone(), Paper::A1).unwrap();
    let wide = compose(input, Paper::Widescreen).unwrap();
    assert_eq!(wide.scene.width_mm / wide.scene.height_mm, 16. / 9.);
    assert_eq!(
        a3.input.snapshot.model_digest,
        a1.input.snapshot.model_digest
    );
    assert_eq!(
        a3.input.snapshot.model_digest,
        wide.input.snapshot.model_digest
    );
    let section_scale = |doc: &ace_drawing::DrawingDocument| {
        doc.scene
            .elements
            .iter()
            .find_map(|e| {
                if e.evidence == "cut:T"
                    && let Primitive::Text { text, .. } = &e.primitive
                {
                    text.strip_prefix("1:").and_then(|s| s.parse::<f64>().ok())
                } else {
                    None
                }
            })
            .unwrap()
    };
    assert_eq!(section_scale(&a3), 2. * section_scale(&a1));
    let figures = compose_figures(&wide).unwrap();
    assert_eq!(figures.len(), 3);
    assert!(texts(&figures[1]).contains("Declared MTOW: N/C"));
    assert!(texts(&figures[2]).contains("N/C - supply performance"));
    assert_eq!(
        figures[0].scene.width_mm / figures[0].scene.height_mm,
        1100. / 560.
    );
    for figure in figures {
        let reopened: ace_drawing::DrawingDocument =
            serde_json::from_slice(&serde_json::to_vec(&figure).unwrap()).unwrap();
        assert_eq!(figure.svg().unwrap(), reopened.svg().unwrap());
        assert_eq!(figure.pdf().unwrap(), reopened.pdf().unwrap());
    }
}

#[test]
fn pressure_and_energy_units_convert_against_independent_si_factors() {
    assert_eq!(
        Quantity::new(3., Unit::Megapascal)
            .unwrap()
            .to(Unit::Pascal)
            .unwrap()
            .value,
        3_000_000.
    );
    assert_eq!(
        Quantity::new(2., Unit::KilowattHour)
            .unwrap()
            .to(Unit::Joule)
            .unwrap()
            .value,
        7_200_000.
    );
    assert!(!Unit::Joule.compatible(Unit::Watt));
    assert!(!Unit::Pascal.compatible(Unit::Newton));
}

#[test]
fn supplied_callouts_retain_warning_margin_units_and_unknown_maturity() {
    let mut input = input();
    let callout = serde_json::from_value(serde_json::json!({
        "id":"fixture-check", "target":"fixture-frame",
        "anchor":["wing.span",{"value":0.,"unit":"m"},{"value":0.,"unit":"m"}],
        "result":{"label":"Renderer dimension check","value":"wing.span","unit":"m","check":{"status":"warn","priority":"soft","provisional":true,"margin":{"value":0.5,"unit":"m"}},"equation":"displayed length = bound parameter","evidence":"fixture"}
    })).unwrap();
    input.data.as_mut().unwrap().results.callouts.push(callout);
    let doc = compose(input.clone(), Paper::A3).unwrap();
    assert!(texts(&doc).contains("WARN [PROV]"));
    assert!(texts(&doc).contains("margin +0.500 m"));
    assert!(texts(&doc).contains("Definition / Unknown"));
    assert!(texts(&doc).contains("displayed length = bound parameter"));
    input.data.as_mut().unwrap().results.callouts[0]
        .result
        .value = None;
    assert!(compose(input, Paper::A3).is_err());
}

#[test]
fn long_supplied_curve_keeps_every_point_and_records_all_states() {
    let mut input = input();
    let points: Vec<_> = (0..257).map(|i| serde_json::json!({
        "x":{"value":i,"unit":"s"}, "y":{"value":120. - f64::from(i) / 10.,"unit":"kg"}, "state":format!("fixture-{i}")
    })).collect();
    let curve = serde_json::from_value(serde_json::json!({
        "id":"renderer-sequence", "title":"Synthetic curve fixture", "role":"energy",
        "x_axis":{"label":"elapsed","unit":"s"}, "y_axis":{"label":"quantity","unit":"kg"},
        "points":points,"evidence":"fixture"
    }))
    .unwrap();
    input.data.as_mut().unwrap().results.curves.push(curve);
    input.data.as_mut().unwrap().figures[2].content = ace_drawing::FigureContent::Curves {
        ids: vec!["renderer-sequence".into()],
    };
    let doc = compose(input, Paper::A3).unwrap();
    assert!(texts(&doc).contains("elapsed / h"));
    // 256 seconds / 3600, plus the plot's 5% padding, rounds to 0.07 hours.
    assert!(texts(&doc).lines().any(|line| line == "0.07"));
    let figures = compose_figures(&doc).unwrap();
    assert!(texts(&figures[2]).contains("elapsed / h"));
    assert!(texts(&figures[2]).lines().any(|line| line == "0.07"));
    assert_eq!(
        doc.scene.evidence["curve:renderer-sequence"].units,
        vec![Unit::Hour, Unit::Kilogram]
    );
    let path = doc
        .scene
        .elements
        .iter()
        .find(|e| e.id.starts_with("renderer-sequence-"))
        .unwrap();
    let Primitive::Path { points, .. } = &path.primitive else {
        panic!("expected curve")
    };
    assert_eq!(points.len(), 257);
    let first = points[0];
    let last = points[256];
    let midpoint = points[128];
    assert!((midpoint[0] - (first[0] + last[0]) / 2.).abs() < 1e-9);
    assert!((midpoint[1] - (first[1] + last[1]) / 2.).abs() < 1e-9);
    assert_eq!(
        doc.input.data.unwrap().results.curves[0].points[256].state,
        "fixture-256"
    );
}

#[test]
fn exported_manifest_flags_missing_facts_and_png_exceeds_display_resolution() {
    let doc = compose(input(), Paper::Widescreen).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("exports");
    let files = crate::services::drawing::export::write_blocking(&doc, &directory).unwrap();
    assert_eq!(files.len(), 17);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    assert!(
        !manifest["figures"][2]["presentation_complete"]
            .as_bool()
            .unwrap()
    );
    assert!(
        manifest["figures"][2]["missing_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("Declared MTOW"))
    );
    let png = std::fs::read(directory.join("GA-example.png")).unwrap();
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    assert!(width >= 4400 && height >= 2240);
    let frozen: ace_drawing::DrawingDocument =
        serde_json::from_slice(&std::fs::read(directory.join("GA-example.json")).unwrap()).unwrap();
    assert_eq!(
        frozen.input.snapshot.model_digest,
        doc.input.snapshot.model_digest
    );
    assert!(crate::services::drawing::export::write_blocking(&doc, &directory).is_err());
}

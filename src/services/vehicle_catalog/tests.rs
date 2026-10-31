use super::*;
use std::path::PathBuf;

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("vehicle.json")
}

#[test]
fn b777_catalog_entries_decode_and_name_their_consumers() {
    let listing = list_blocking(&example("b777")).unwrap();
    let ids: Vec<_> = listing
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(ids, ["engine", "limits", "polars"]);
    for entry in &listing.entries {
        assert!(
            matches!(entry.check, EntryCheck::Decoded { .. }),
            "{} {:?}",
            entry.id,
            entry.check
        );
    }
    let engine = &listing.entries[0];
    let EntryCheck::Decoded { summary } = &engine.check else {
        panic!("engine entry must decode: {:?}", engine.check);
    };
    assert_eq!(summary, "engine.ge90_115b_class");
    assert!(engine.selected_by.contains(&"performance".to_owned()));
    assert!(engine.selected_by.contains(&"path".to_owned()));
}

#[test]
fn rejected_and_unknown_entries_are_reported_not_hidden() {
    let path = example("c172");
    let mut model: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    model["catalog"]["polars"]["document"]["clean"]["cd0"] = serde_json::json!(-1.0);
    model["catalog"]["notes"] = serde_json::json!({
        "kind": "design_notes", "version": 1,
        "source": {"kind": "user", "reference": "test", "revision": null},
        "document": {"text": "free-form"}
    });
    let directory = tempfile::tempdir().unwrap();
    let edited = directory.path().join("vehicle.json");
    std::fs::write(&edited, serde_json::to_vec(&model).unwrap()).unwrap();
    let listing = list_blocking(&edited).unwrap();
    let check = |id: &str| {
        &listing
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .unwrap()
            .check
    };
    assert!(
        matches!(check("polars"), EntryCheck::Rejected { .. }),
        "{:?}",
        check("polars")
    );
    assert!(matches!(check("notes"), EntryCheck::Unchecked));
    let Err(AexError::Validation { code, .. }) = show_blocking(&edited, "missing") else {
        panic!("an unknown entry must be an error");
    };
    assert_eq!(code, "UNKNOWN_CATALOG_ENTRY");
    let shown = show_blocking(&edited, "notes").unwrap();
    assert_eq!(shown.document["text"], "free-form");
}

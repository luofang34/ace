use super::*;

#[test]
fn content_digest_rejects_changed_embedded_and_external_bytes() {
    let mut artifact = Artifact::from_text(
        "mesh vertices".into(),
        "text/plain",
        "test.mesh/1",
        GeneratorIdentity {
            id: "mesh".into(),
            version: "1".into(),
            implementation: "source-hash".into(),
        },
    );
    artifact.validate().unwrap();
    artifact.verify(b"mesh vertices").unwrap();
    assert!(artifact.verify(b"moved vertices").is_err());
    artifact.text = Some("moved vertices".into());
    assert!(artifact.validate().is_err());
    let changed = Artifact::from_text(
        "moved vertices".into(),
        "text/plain",
        "test.mesh/1",
        artifact.generator.clone(),
    );
    assert_ne!(artifact.digest, changed.digest);
    changed.validate().unwrap();
}

use super::{AircraftTopology, RawAircraftTopology};

#[test]
fn conventional_legacy_topology_is_inferred() -> Result<(), Box<dyn std::error::Error>> {
    let topology = AircraftTopology::resolve(None, "conventional_tail", 1, true)?;

    assert!(topology.inferred);
    assert!(topology.has_component_kind("fuselage"));
    assert!(topology.has_component_kind("horizontal_tail"));
    assert!(topology.has_component_kind("propeller"));
    assert!(!topology.has_component_kind("lifting_body"));
    Ok(())
}

#[test]
fn invalid_relationship_reference_is_rejected() {
    let raw: Result<RawAircraftTopology, _> = serde_yaml::from_str(
        r#"
version: 1
components:
  - { id: wing, kind: wing }
relationships:
  - { kind: attached_to, source: wing, target: fuselage }
"#,
    );
    assert!(raw.is_ok());
    if let Ok(topology) = raw {
        let result = AircraftTopology::resolve(Some(topology), "conventional", 1, false);
        assert!(
            result.is_err_and(|error| error.to_string().contains("UNKNOWN_RELATIONSHIP_COMPONENT"))
        );
    }
}

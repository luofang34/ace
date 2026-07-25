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

#[test]
fn canonical_kind_can_be_unsupported_by_a_backend() -> Result<(), Box<dyn std::error::Error>> {
    let raw: RawAircraftTopology = serde_yaml::from_str(
        r#"
version: 1
components:
  - { id: fuselage, kind: fuselage }
  - { id: twin_boom, kind: boom, count: 2 }
relationships:
  - { kind: attached_to, source: twin_boom, target: fuselage }
"#,
    )?;
    let topology = AircraftTopology::resolve(Some(raw), "twin_boom", 1, false)?;
    let unsupported = topology.unsupported_component_kinds(&["fuselage".to_owned()]);

    assert_eq!(unsupported, ["boom"]);
    Ok(())
}

use crate::backends::contracts::{BackendDescriptor, BackendTopologyCapabilities};

pub(super) fn native_descriptor() -> BackendDescriptor {
    BackendDescriptor {
        id: "native".to_owned(),
        display_name: "Native conceptual approximation".to_owned(),
        available: true,
        version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        capabilities: vec![
            "conceptual_geometry".to_owned(),
            "weight_iteration".to_owned(),
            "drag_polar".to_owned(),
            "mission_performance".to_owned(),
            "constraint_margins".to_owned(),
        ],
        disciplines: vec![
            "aerodynamics".to_owned(),
            "propulsion".to_owned(),
            "structures".to_owned(),
            "mission".to_owned(),
        ],
        fidelity_levels: vec![0, 1],
        topology: BackendTopologyCapabilities {
            component_kinds: vec![
                "fuselage".to_owned(),
                "wing".to_owned(),
                "horizontal_tail".to_owned(),
                "vertical_tail".to_owned(),
                "lifting_body".to_owned(),
                "engine".to_owned(),
                "propeller".to_owned(),
            ],
            relationship_kinds: vec![
                "attached_to".to_owned(),
                "symmetric_about".to_owned(),
                "carries_load_to".to_owned(),
            ],
            delegated_relationship_kinds: Vec::new(),
        },
        unavailable_reason: None,
    }
}

use crate::domain::schema::ResolvedScenario;

/// Whether the aircraft is a tailless lifting body, declared or inferred from a legacy
/// configuration name.
pub(crate) fn is_blended_wing_body(scenario: &ResolvedScenario) -> bool {
    let topology = &scenario.aircraft.topology;
    topology.has_component_kind("lifting_body")
        || topology.inferred
            && crate::domain::topology::legacy_lifting_body(&scenario.aircraft.configuration)
}

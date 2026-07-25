use crate::domain::schema::ResolvedScenario;
use crate::domain::topology::{AircraftTopology, ComponentRelationship};

pub(crate) fn native_topology_violations(scenario: &ResolvedScenario) -> Vec<String> {
    let topology = &scenario.aircraft.topology;
    let mut violations = Vec::new();
    if topology.has_component_kind("lifting_body") {
        validate_lifting_body(topology, &mut violations);
    } else {
        validate_conventional(topology, &mut violations);
    }
    validate_propulsion(scenario, &mut violations);
    validate_parameters(topology, &mut violations);
    validate_relationships(scenario, &mut violations);
    violations.sort();
    violations.dedup();
    violations
}

fn validate_conventional(topology: &AircraftTopology, violations: &mut Vec<String>) {
    for kind in ["fuselage", "wing", "horizontal_tail", "vertical_tail"] {
        require_single_component(topology, kind, "conventional", violations);
    }
    if topology.has_component_kind("lifting_body") {
        violations.push(
            "native conventional topology cannot combine a lifting_body with discrete surfaces"
                .to_owned(),
        );
    }
}

fn validate_lifting_body(topology: &AircraftTopology, violations: &mut Vec<String>) {
    require_single_component(topology, "lifting_body", "lifting-body", violations);
    for kind in ["fuselage", "wing", "horizontal_tail", "vertical_tail"] {
        if topology.has_component_kind(kind) {
            violations.push(format!(
                "native lifting-body topology cannot include a {kind} component"
            ));
        }
    }
}

fn validate_propulsion(scenario: &ResolvedScenario, violations: &mut Vec<String>) {
    let topology = &scenario.aircraft.topology;
    require_component_count(
        topology,
        "engine",
        scenario.aircraft.propulsion.engine_count,
        violations,
    );
    let expected_propellers = if scenario.propeller.is_some() {
        scenario.aircraft.propulsion.engine_count
    } else {
        0
    };
    require_component_count(topology, "propeller", expected_propellers, violations);
}

fn require_single_component(
    topology: &AircraftTopology,
    kind: &str,
    configuration: &str,
    violations: &mut Vec<String>,
) {
    let (instances, count) = component_count(topology, kind);
    if instances != 1 || count != 1 {
        violations.push(format!(
            "native {configuration} topology requires exactly one {kind} component with count 1"
        ));
    }
}

fn require_component_count(
    topology: &AircraftTopology,
    kind: &str,
    expected: u32,
    violations: &mut Vec<String>,
) {
    let (instances, count) = component_count(topology, kind);
    let expected_instances = usize::from(expected > 0);
    if instances != expected_instances || count != expected {
        violations.push(format!(
            "native topology requires {kind} count {expected} to match resolved propulsion"
        ));
    }
}

fn component_count(topology: &AircraftTopology, kind: &str) -> (usize, u32) {
    topology
        .components
        .iter()
        .filter(|component| component.kind == kind)
        .fold((0_usize, 0_u32), |(instances, count), component| {
            (
                instances.wrapping_add(1),
                count.wrapping_add(component.count),
            )
        })
}

fn validate_parameters(topology: &AircraftTopology, violations: &mut Vec<String>) {
    for component in &topology.components {
        if !component.parameters.is_empty() {
            violations.push(format!(
                "native topology does not support parameters on component {}",
                component.id
            ));
        }
    }
}

fn validate_relationships(scenario: &ResolvedScenario, violations: &mut Vec<String>) {
    let topology = &scenario.aircraft.topology;
    let mut actual = topology
        .relationships
        .iter()
        .filter_map(|relationship| {
            endpoint_kinds(topology, relationship).map(|(source, target)| {
                (
                    relationship.kind.clone(),
                    source.to_owned(),
                    target.to_owned(),
                )
            })
        })
        .collect::<Vec<_>>();
    let mut expected = expected_relationships(scenario);
    actual.sort();
    expected.sort();
    if actual != expected {
        violations.push(format!(
            "native topology relationships must exactly match {expected:?}; got {actual:?}"
        ));
    }
}

fn endpoint_kinds<'a>(
    topology: &'a AircraftTopology,
    relationship: &ComponentRelationship,
) -> Option<(&'a str, &'a str)> {
    let source = topology
        .components
        .iter()
        .find(|component| component.id == relationship.source)?;
    let target = topology
        .components
        .iter()
        .find(|component| component.id == relationship.target)?;
    Some((&source.kind, &target.kind))
}

fn expected_relationships(scenario: &ResolvedScenario) -> Vec<(String, String, String)> {
    let topology = &scenario.aircraft.topology;
    let mut expected = Vec::new();
    if topology.has_component_kind("lifting_body") {
        expected.push(shape("attached_to", "engine", "lifting_body"));
    } else {
        for source in ["wing", "horizontal_tail", "vertical_tail"] {
            expected.push(shape("attached_to", source, "fuselage"));
        }
        let engine_target = if scenario.aircraft.propulsion.engine_count > 1 {
            "wing"
        } else {
            "fuselage"
        };
        expected.push(shape("attached_to", "engine", engine_target));
        expected.push(shape("carries_load_to", "wing", "fuselage"));
    }
    if scenario.propeller.is_some() {
        expected.push(shape("attached_to", "propeller", "engine"));
    }
    if scenario.aircraft.propulsion.engine_count > 1 {
        let symmetry_target = if topology.has_component_kind("lifting_body") {
            "lifting_body"
        } else {
            "fuselage"
        };
        expected.push(shape("symmetric_about", "engine", symmetry_target));
    }
    expected
}

fn shape(kind: &str, source: &str, target: &str) -> (String, String, String) {
    (kind.to_owned(), source.to_owned(), target.to_owned())
}

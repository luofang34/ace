use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::domain::diagnostic::{AexError, AexResult};

const COMPONENT_KINDS: [&str; 11] = [
    "fuselage",
    "wing",
    "horizontal_tail",
    "vertical_tail",
    "canard",
    "lifting_body",
    "boom",
    "engine",
    "propeller",
    "fuel_system",
    "payload",
];
const RELATIONSHIP_KINDS: [&str; 7] = [
    "attached_to",
    "symmetric_about",
    "repeated_about",
    "aligned_with",
    "parallel_to",
    "continuous_with",
    "carries_load_to",
];

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(crate) struct RawAircraftTopology {
    #[serde(default = "topology_version")]
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) components: Vec<AircraftComponent>,
    #[serde(default)]
    pub(crate) relationships: Vec<ComponentRelationship>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct AircraftComponent {
    pub(crate) id: String,
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) definition: Option<String>,
    #[serde(default = "one")]
    pub(crate) count: u32,
    #[serde(default)]
    pub(crate) roles: Vec<String>,
    #[serde(default)]
    pub(crate) parameters: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ComponentRelationship {
    pub(crate) kind: String,
    pub(crate) source: String,
    pub(crate) target: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AircraftTopology {
    pub(crate) version: u32,
    pub(crate) inferred: bool,
    pub(crate) components: Vec<AircraftComponent>,
    pub(crate) relationships: Vec<ComponentRelationship>,
}

impl AircraftTopology {
    pub(crate) fn resolve(
        raw: Option<RawAircraftTopology>,
        configuration: &str,
        engine_count: u32,
        has_propeller: bool,
    ) -> AexResult<Self> {
        let (inferred, topology) = raw.map_or_else(
            || {
                (
                    true,
                    inferred_topology(configuration, engine_count, has_propeller),
                )
            },
            |topology| (false, topology),
        );
        validate_topology(&topology)?;
        Ok(Self {
            version: topology.version,
            inferred,
            components: topology.components,
            relationships: topology.relationships,
        })
    }

    pub(crate) fn has_component_kind(&self, kind: &str) -> bool {
        self.components
            .iter()
            .any(|component| component.kind == kind)
    }

    pub(crate) fn unsupported_component_kinds(&self, supported: &[String]) -> Vec<String> {
        unsupported_values(
            self.components.iter().map(|component| &component.kind),
            supported,
        )
    }

    pub(crate) fn unsupported_relationship_kinds(&self, supported: &[String]) -> Vec<String> {
        unsupported_values(
            self.relationships
                .iter()
                .map(|relationship| &relationship.kind),
            supported,
        )
    }
}

fn unsupported_values<'a>(
    values: impl Iterator<Item = &'a String>,
    supported: &[String],
) -> Vec<String> {
    let supported = supported
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    values
        .map(String::as_str)
        .filter(|value| !supported.contains(value))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn validate_topology(topology: &RawAircraftTopology) -> AexResult<()> {
    if topology.version != topology_version() {
        return Err(AexError::validation(
            "UNSUPPORTED_TOPOLOGY_VERSION",
            "aircraft.topology.version",
            format!("expected {}, got {}", topology_version(), topology.version),
        ));
    }
    if topology.components.is_empty() {
        return Err(AexError::validation(
            "EMPTY_AIRCRAFT_TOPOLOGY",
            "aircraft.topology.components",
            "topology requires at least one component",
        ));
    }
    let mut ids = BTreeSet::new();
    for component in &topology.components {
        validate_component(component, &mut ids)?;
    }
    for relationship in &topology.relationships {
        validate_relationship(relationship, &ids)?;
    }
    Ok(())
}

fn validate_component(component: &AircraftComponent, ids: &mut BTreeSet<String>) -> AexResult<()> {
    if component.id.is_empty() || component.count == 0 {
        return Err(AexError::validation(
            "INVALID_AIRCRAFT_COMPONENT",
            "aircraft.topology.components",
            "component id must be non-empty and count must be positive",
        ));
    }
    if !COMPONENT_KINDS.contains(&component.kind.as_str()) {
        return Err(AexError::validation(
            "UNSUPPORTED_COMPONENT_KIND",
            "aircraft.topology.components",
            format!("unknown component kind {}", component.kind),
        ));
    }
    if !ids.insert(component.id.clone()) {
        return Err(AexError::validation(
            "DUPLICATE_AIRCRAFT_COMPONENT",
            "aircraft.topology.components",
            format!("component id {} appears more than once", component.id),
        ));
    }
    Ok(())
}

fn validate_relationship(
    relationship: &ComponentRelationship,
    component_ids: &BTreeSet<String>,
) -> AexResult<()> {
    if !RELATIONSHIP_KINDS.contains(&relationship.kind.as_str()) {
        return Err(AexError::validation(
            "UNSUPPORTED_COMPONENT_RELATIONSHIP",
            "aircraft.topology.relationships",
            format!("unknown relationship kind {}", relationship.kind),
        ));
    }
    for id in [&relationship.source, &relationship.target] {
        if !component_ids.contains(id) {
            return Err(AexError::validation(
                "UNKNOWN_RELATIONSHIP_COMPONENT",
                "aircraft.topology.relationships",
                format!("relationship references unknown component {id}"),
            ));
        }
    }
    if relationship.source == relationship.target {
        return Err(AexError::validation(
            "SELF_REFERENTIAL_COMPONENT_RELATIONSHIP",
            "aircraft.topology.relationships",
            format!("component {} references itself", relationship.source),
        ));
    }
    Ok(())
}

fn inferred_topology(
    configuration: &str,
    engine_count: u32,
    has_propeller: bool,
) -> RawAircraftTopology {
    let lifting_body = legacy_lifting_body(configuration);
    let mut components = if lifting_body {
        vec![component("lifting_body", "lifting_body", 1)]
    } else {
        vec![
            component("fuselage", "fuselage", 1),
            component("wing", "wing", 1),
            component("horizontal_tail", "horizontal_tail", 1),
            component("vertical_tail", "vertical_tail", 1),
        ]
    };
    components.push(component("powerplant", "engine", engine_count));
    if has_propeller {
        components.push(component("propeller", "propeller", engine_count));
    }
    let parent = if lifting_body {
        "lifting_body"
    } else {
        "fuselage"
    };
    let mut relationships = components
        .iter()
        .filter(|item| item.id != parent && item.id != "propeller")
        .map(|item| {
            let target = if !lifting_body && item.kind == "engine" && engine_count > 1 {
                "wing"
            } else {
                parent
            };
            relationship("attached_to", &item.id, target)
        })
        .collect::<Vec<_>>();
    if has_propeller {
        relationships.push(relationship("attached_to", "propeller", "powerplant"));
    }
    if engine_count > 1 {
        relationships.push(relationship("symmetric_about", "powerplant", parent));
    }
    if !lifting_body {
        relationships.push(relationship("carries_load_to", "wing", "fuselage"));
    }
    RawAircraftTopology {
        version: topology_version(),
        components,
        relationships,
    }
}

pub(crate) fn legacy_lifting_body(configuration: &str) -> bool {
    let configuration = configuration.to_ascii_lowercase();
    configuration.contains("blended_wing")
        || configuration.contains("flying_wing")
        || configuration.contains("tailless")
}

fn component(id: &str, kind: &str, count: u32) -> AircraftComponent {
    AircraftComponent {
        id: id.to_owned(),
        kind: kind.to_owned(),
        definition: None,
        count,
        roles: Vec::new(),
        parameters: BTreeMap::new(),
    }
}

fn relationship(kind: &str, source: &str, target: &str) -> ComponentRelationship {
    ComponentRelationship {
        kind: kind.to_owned(),
        source: source.to_owned(),
        target: target.to_owned(),
    }
}

const fn topology_version() -> u32 {
    1
}

const fn one() -> u32 {
    1
}

#[cfg(test)]
mod tests;

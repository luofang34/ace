use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BackendDescriptor {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) capabilities: Vec<String>,
    pub(crate) disciplines: Vec<String>,
    pub(crate) fidelity_levels: Vec<u8>,
    pub(crate) topology: BackendTopologyCapabilities,
    pub(crate) unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BackendTopologyCapabilities {
    pub(crate) component_kinds: Vec<String>,
    pub(crate) relationship_kinds: Vec<String>,
    pub(crate) delegated_relationship_kinds: Vec<String>,
}

/// A backend that reports its identity and topology support.
pub(crate) trait GeometryBackend: Send + Sync {
    fn descriptor(&self) -> BackendDescriptor;
}

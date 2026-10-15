use serde::{Deserialize, Serialize};

include!(concat!(env!("OUT_DIR"), "/implementation.rs"));

/// Build identity covering workspace implementation, assets, dependency lock and compiler settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildIdentity {
    /// Combined immutable implementation fingerprint.
    pub digest: String,
    /// Workspace source, manifests and bundled asset fingerprint.
    pub source_digest: String,
    /// Exact dependency-lock fingerprint, or explicitly unavailable outside a locked workspace.
    pub dependency_digest: String,
    /// Rust compiler identity including its commit and host.
    pub compiler: String,
    /// Build target.
    pub target: String,
    /// Profile, optimization, debug, flags and enabled engine features.
    pub options: String,
}

/// Identity compiled into this execution engine; retained when historical results are restored.
pub fn implementation_identity() -> BuildIdentity {
    BuildIdentity {
        digest: BUILD_DIGEST.into(),
        source_digest: BUILD_PARTS[0].into(),
        dependency_digest: BUILD_PARTS[1].into(),
        compiler: BUILD_PARTS[2].into(),
        target: BUILD_PARTS[3].into(),
        options: BUILD_PARTS[4].into(),
    }
}

use ace_model::ModelError;
use thiserror::Error;

/// Domain or serialization error that prevents constructing or editing a session.
#[derive(Debug, Error)]
pub enum EngineError {
    /// A registered solver cannot satisfy the selected analysis contract.
    #[error("analysis {analysis}: {source}")]
    Contract {
        /// Analysis or registered solver identifier.
        analysis: String,
        /// Original contract failure.
        #[source]
        source: ace_model::ContractError,
    },
    /// Execution canceled or its deadline elapsed.
    #[error(transparent)]
    Interrupted(#[from] crate::Interruption),
    /// Model or dependency graph is invalid.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// Evidence could not be serialized.
    #[error("could not serialize synthesis evidence: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Duplicate solver registration would make execution identity ambiguous.
    #[error("solver already registered: {0}@{1}")]
    DuplicatePlugin(String, String),
    /// A preview replacement violates registration or port-contract requirements.
    #[error("invalid preview for {plugin}@{version}: {reason}")]
    InvalidPreview {
        /// Authoritative solver identifier.
        plugin: String,
        /// Exact authoritative solver version.
        version: String,
        /// Registration failure with contract context.
        reason: String,
    },
}

/// Result from orchestration and validated edits.
pub type EngineResult<T> = Result<T, EngineError>;

/// Typed failure from an installed solver.
#[derive(Debug, Error)]
pub enum SolverError {
    /// A declared applicability or invocation contract was violated.
    #[error(transparent)]
    Contract(#[from] ace_model::ContractError),
    /// Execution canceled or its deadline elapsed.
    #[error(transparent)]
    Interrupted(#[from] crate::Interruption),
    /// Physical values or dimensions are invalid.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// Solver failed or rejected its validity envelope.
    #[error("{0}")]
    Failed(String),
    /// A domain solver preserved its own typed error.
    #[error("{solver}: {source}")]
    External {
        /// Selected solver.
        solver: String,
        /// Original adapter or solver error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// An external solver or artifact operation failed.
    #[error("{operation}: {source}")]
    Io {
        /// Operation with solver/artifact context.
        operation: String,
        /// Original I/O error.
        #[source]
        source: std::io::Error,
    },
}

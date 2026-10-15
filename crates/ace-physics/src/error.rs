use ace_model::ModelError;
use thiserror::Error;

/// Semantic-model validation or physical execution failure.
#[derive(Debug, Error)]
pub enum PhysicsError {
    /// Execution canceled or exceeded its monotonic deadline.
    #[error(transparent)]
    Interrupted(#[from] ace_engine::Interruption),
    /// Requested consumption exceeds the fuel available in the declared tank sequence.
    #[error("{stage}: fuel shortfall {missing_kg} kg in selected tank sequence")]
    FuelShortfall {
        /// Phase or stage identifier.
        stage: String,
        /// Additional fuel needed for the attempted interval.
        missing_kg: f64,
    },
    /// A registered phase adapter failed while retaining its typed source.
    #[error("phase {stage}: {source}")]
    Phase {
        /// Stage identifier.
        stage: String,
        /// Original solver failure.
        #[source]
        source: ace_engine::SolverError,
    },
    /// Canonical quantity or reference is invalid.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// Physical state cannot satisfy the operation.
    #[error("{object}: {message}")]
    Invalid {
        /// Component, mission stage, or operating case.
        object: String,
        /// Failed physical invariant.
        message: String,
    },
    /// Result evidence could not be encoded.
    #[error("cannot encode physics evidence: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result from a semantic analysis.
pub type PhysicsResult<T> = Result<T, PhysicsError>;

impl PhysicsError {
    pub(crate) fn invalid(object: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Invalid {
            object: object.into(),
            message: message.into(),
        }
    }
}

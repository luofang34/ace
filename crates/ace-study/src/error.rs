use thiserror::Error;

/// Failure to validate or execute a vehicle design study.
#[derive(Debug, Error)]
pub enum StudyError {
    /// A plan field violates its bounds or the model's declared search space.
    #[error("invalid study field {field}: {message}")]
    Invalid {
        /// Field or parameter identifier.
        field: String,
        /// Context for the rejected value.
        message: String,
    },
    /// Invalid physical quantity or canonical model declaration.
    #[error(transparent)]
    Model(#[from] ace_model::ModelError),
    /// Failure in the shared synthesis engine.
    #[error(transparent)]
    Engine(#[from] ace_engine::EngineError),
    /// Evidence serialization failure.
    #[error("could not serialize vehicle study evidence: {0}")]
    Serialization(#[from] serde_json::Error),
    /// An archived snapshot refers to absent calculation evidence.
    #[error("candidate {candidate} references missing run {run}")]
    MissingEvidence {
        /// Candidate content identifier.
        candidate: String,
        /// Missing calculation content identifier.
        run: String,
    },
}

impl StudyError {
    pub(crate) fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Invalid {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Result of a design study operation.
pub type StudyResult<T> = Result<T, StudyError>;

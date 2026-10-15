use thiserror::Error;

/// Validation or dimensional arithmetic failure with the affected design context.
#[derive(Debug, Error)]
pub enum ModelError {
    /// A record violates its schema or physical preconditions.
    #[error("invalid model at {path}: {reason}")]
    Invalid {
        /// Parameter or object being validated.
        path: String,
        /// Invariant that could not be satisfied.
        reason: String,
    },
    /// An expression or binding references a missing canonical value.
    #[error("unknown parameter or object: {0}")]
    UnknownReference(String),
    /// Arithmetic used incompatible physical dimensions.
    #[error("incompatible dimensions: {0}")]
    Dimensions(String),
}

/// Result returned by the vehicle model's validation and arithmetic APIs.
pub type ModelResult<T> = Result<T, ModelError>;

impl ModelError {
    /// Construct a contextual schema error.
    pub fn invalid(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Invalid {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

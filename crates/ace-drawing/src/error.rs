use thiserror::Error;

/// Drawing construction, evidence or serialization failure.
#[derive(Debug, Error)]
pub enum DrawingError {
    /// Invalid drawing or source context.
    #[error("drawing {0}")]
    Invalid(String),
    /// Canonical model error.
    #[error(transparent)]
    Model(#[from] ace_model::ModelError),
    /// Source snapshot does not support the requested projection.
    #[error(transparent)]
    Engine(#[from] ace_engine::EngineError),
    /// Study evidence cannot be restored.
    #[error(transparent)]
    Study(#[from] ace_study::StudyError),
    /// Evidence cannot be serialized.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Result of creating or replaying an engineering drawing.
pub type DrawingResult<T> = Result<T, DrawingError>;

/// Failure of a physical or numerical precondition.
#[derive(Debug, thiserror::Error)]
pub enum CfdError {
    /// Invalid user configuration.
    #[error("invalid CFD configuration: {0}")]
    Configuration(String),
    /// Nonphysical state; no density or temperature clipping is applied.
    #[error("nonphysical state at cell {cell}: density={density}, pressure={pressure}")]
    State {
        /// Index in mesh cell order.
        cell: usize,
        /// Density in kg/m³.
        density: f64,
        /// Pressure in Pa.
        pressure: f64,
    },
    /// Incorrect state vector length.
    #[error("mesh has {expected} cells but state has {actual}")]
    StateCount {
        /// Required number of states.
        expected: usize,
        /// Supplied number of states.
        actual: usize,
    },
}

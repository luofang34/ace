//! Incremental vehicle synthesis with typed solver plugins and traceable closure.

mod assessment;
mod closure;
mod control;
mod error;
mod execution;
mod graph;
mod identity;
mod plugin;
mod record;
mod requirement;
mod session;

pub use assessment::{Assessment, EvidenceQuality, ExecutionStatus, Feasibility, assess};
pub use control::{Interruption, RunControl};
pub use error::{EngineError, EngineResult, SolverError};
pub use identity::{BuildIdentity, implementation_identity};
pub use plugin::{
    AnalysisContext, AnalysisPlugin, Fidelity, PhaseIncrement, PhaseOutput, PhaseRun,
    PluginIdentity, PluginOutput, PluginSpec, Registry,
};
pub use record::{ClosureResult, RunRecord, Snapshot, ValueRecord, ValueStatus};
pub use requirement::{Margin, MarginStatus, evaluate_requirements};
pub use session::Session;

//! Conservative finite-volume kernels for reproducible aerothermal experiments.

mod error;
mod flux;
mod gas;
mod mesh;
mod operator;
mod solver;
mod vector;

pub use error::CfdError;
pub use flux::Convection;
pub use gas::{Conserved, IdealGas, Primitive};
pub use mesh::{Mesh, SphereGrid};
pub use operator::{
    CellDiagnostic, EnergyBalance, FaceDiagnostic, FlowOperator, Outflow, WallSample,
};
pub use solver::{
    ConvergenceSample, ObservationError, Solution, SolverConfig, solve, solve_checkpointed,
    solve_observed,
};

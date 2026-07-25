//! Deterministic low-fidelity aircraft concept analysis with CLI and MCP adapters.

#![forbid(unsafe_code)]

mod backends;
mod charts;
mod cli;
mod domain;
mod mcp;
mod models;
mod services;
mod storage;

pub use cli::run_cli;
pub use domain::diagnostic::AexError;
pub use domain::validity::{ModelDomainViolation, ValidityBasis, ValidityVariable};
pub use models::weight::{WeightClosureInput, WeightClosureResult, solve_weight_closure};

#[cfg(test)]
mod test_support;

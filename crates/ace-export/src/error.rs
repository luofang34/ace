use ace_model::ModelError;
use mbd_core::MbdError;
use syspack::SyspackError;

/// Why a design could not become a system package.
#[derive(Debug, thiserror::Error)]
pub enum AceExportError {
    /// The model failed its own validation.
    #[error("the vehicle model is invalid")]
    InvalidModel(#[source] ModelError),
    /// The model is of a domain this bridge has no flight model for.
    #[error("no flight model for domain {domain:?}; only fixed_wing compiles")]
    UnsupportedDomain {
        /// The model's domain.
        domain: String,
    },
    /// A value the bridge needs could not be read in its unit.
    #[error("cannot read {name}")]
    Parameter {
        /// The parameter or binding.
        name: String,
        /// Why it could not be read.
        #[source]
        source: ModelError,
    },
    /// A parameter holds neither a value nor an expression.
    #[error("parameter {name} has no value: it is an analysis output")]
    UnresolvedParameter {
        /// The parameter.
        name: String,
    },
    /// The mass statement lacks a contribution the tree needs.
    #[error("the mass statement has no {kind} item")]
    MissingMassItem {
        /// The missing kind.
        kind: &'static str,
    },
    /// A quantity is outside what the flight model can represent.
    #[error("{quantity} = {value} is out of range: {expected}")]
    OutOfRange {
        /// The quantity.
        quantity: &'static str,
        /// What was found.
        value: f64,
        /// What the bridge requires.
        expected: &'static str,
    },
    /// The catalog lacks data the powerplant needs, or holds it in another
    /// form.
    #[error("catalog entry: {field}: {detail}")]
    Catalog {
        /// The offending field.
        field: &'static str,
        /// What is wrong with it.
        detail: String,
    },
    /// The manifest could not be serialized.
    #[error("the manifest does not serialize")]
    ManifestEncode(#[source] serde_json::Error),
    /// A package could not be written out.
    #[error("cannot write {path}")]
    Write {
        /// The file or directory.
        path: String,
        /// What the operating system reported.
        #[source]
        source: std::io::Error,
    },
    /// The compiled package failed the loader, so it is not emitted.
    #[error("the compiled package does not load")]
    PackageRejected(#[source] SyspackError),
    /// The CFD solution never settled, so its wall loads describe no flow.
    #[error("the CFD solution did not converge in {steps} steps")]
    SolutionNotConverged {
        /// Iterations spent.
        steps: usize,
    },
    /// The freestream cannot normalise wall loads.
    #[error("freestream {field} = {value} is not a positive number")]
    InvalidFreestream {
        /// The offending field.
        field: &'static str,
        /// What was found.
        value: f64,
    },
    /// The wall samples do not cover the heat shield.
    #[error("wall samples unusable: {detail}")]
    InvalidWall {
        /// What is wrong with them.
        detail: &'static str,
    },
    /// The derived coefficients are refused by the capsule model.
    #[error("the derived capsule coefficients are invalid")]
    InvalidCapsule(#[source] MbdError),
}

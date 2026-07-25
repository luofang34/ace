use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub(crate) type AexResult<T> = Result<T, AexError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Diagnostic {
    pub(crate) code: String,
    pub(crate) severity: Severity,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) context: Value,
}

impl Diagnostic {
    pub(crate) fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Warning,
            message: message.into(),
            path: Some(path.into()),
            context: Value::Object(serde_json::Map::new()),
        }
    }

    pub(crate) fn limitation(message: impl Into<String>) -> Self {
        Self::warning("LOW_FIDELITY_MODEL", message, "models")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Severity {
    Warning,
    Error,
}

/// Typed failure returned by public CLI and analysis entry points.
#[derive(Debug, Error)]
pub enum AexError {
    /// A source document could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Path that could not be read.
        path: PathBuf,
        /// Operating-system failure.
        #[source]
        source: std::io::Error,
    },
    /// An artifact could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Intended artifact path.
        path: PathBuf,
        /// Operating-system failure.
        #[source]
        source: std::io::Error,
    },
    /// YAML parsing or serialization failed.
    #[error("invalid YAML document {path}: {source}")]
    Yaml {
        /// Document or output path.
        path: PathBuf,
        /// YAML codec failure.
        #[source]
        source: serde_yaml::Error,
    },
    /// JSON serialization or parsing failed.
    #[error("cannot serialize JSON result: {source}")]
    Json {
        /// JSON codec failure.
        #[source]
        source: serde_json::Error,
    },
    /// A persisted JSON record could not be decoded.
    #[error("invalid stored JSON record {path}: {source}")]
    StoredJson {
        /// Content-addressed record path.
        path: PathBuf,
        /// JSON codec failure.
        #[source]
        source: serde_json::Error,
    },
    /// A persisted record failed its domain integrity checks.
    #[error("invalid stored record {path}: {source}")]
    StoredRecord {
        /// Content-addressed record path.
        path: PathBuf,
        /// Domain validation failure.
        #[source]
        source: Box<AexError>,
    },
    /// A document violated a stable validation rule.
    #[error("{code} at {path}: {message}")]
    Validation {
        /// Stable machine-readable error code.
        code: &'static str,
        /// Domain path associated with the error.
        path: String,
        /// Human-readable error detail.
        message: String,
    },
    /// A physical quantity was ambiguous or dimensionally incompatible.
    #[error("invalid quantity {value:?} for {target}: {reason}")]
    Quantity {
        /// Original interface value.
        value: String,
        /// Required SI target unit.
        target: &'static str,
        /// Parsing or conversion detail.
        reason: String,
    },
    /// A deterministic numerical analysis failed.
    #[error("{code}: {message}")]
    Analysis {
        /// Stable machine-readable analysis code.
        code: &'static str,
        /// Human-readable failure detail.
        message: String,
    },
    /// A referenced data profile was not found.
    #[error("requested profile {profile_id} was not found below {directory}")]
    ProfileNotFound {
        /// Referenced profile identifier.
        profile_id: String,
        /// Profile search root.
        directory: PathBuf,
    },
    /// A requested optional analysis backend is not installed or configured.
    #[error("analysis backend {backend} is unavailable: {reason}")]
    BackendUnavailable {
        /// Stable backend identifier.
        backend: String,
        /// Discovery or configuration detail.
        reason: String,
    },
    /// A backend subprocess could not be launched.
    #[error("failed to launch backend executable {executable}: {source}")]
    BackendLaunch {
        /// Executable selected by backend discovery.
        executable: PathBuf,
        /// Operating-system launch failure.
        #[source]
        source: std::io::Error,
    },
    /// A backend subprocess completed without a usable result.
    #[error("backend {backend} failed during {operation}: {message}")]
    BackendExecution {
        /// Stable backend identifier.
        backend: String,
        /// Backend operation that failed.
        operation: String,
        /// Exit status, reported backend error, or parsing detail.
        message: String,
    },
}

impl AexError {
    pub(crate) fn validation(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Validation {
            code,
            path: path.into(),
            message: message.into(),
        }
    }

    pub(crate) fn analysis(code: &'static str, message: impl Into<String>) -> Self {
        Self::Analysis {
            code,
            message: message.into(),
        }
    }
}

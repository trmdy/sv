//! Error types for sv
//!
//! Exit codes per spec:
//! - 0: Success
//! - 2: User error (bad args, missing repo)
//! - 3: Blocked by policy (protected paths, active exclusive lease conflict)
//! - 4: Operation failed (git error, merge conflict)

use std::path::PathBuf;
use thiserror::Error;

/// Exit codes for sv CLI
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const USER_ERROR: i32 = 2;
    pub const POLICY_BLOCKED: i32 = 3;
    pub const OPERATION_FAILED: i32 = 4;
}

/// Main error type for sv operations
#[derive(Error, Debug)]
pub enum Error {
    // User errors (exit code 2)
    #[error("Not a git repository: {0}")]
    NotARepo(PathBuf),

    #[error("Repository not found from {0}")]
    RepoNotFound(PathBuf),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("Lease not found: {0}")]
    LeaseNotFound(String),

    // Policy blocks (exit code 3)
    #[error("Protected path would be committed: {0}")]
    ProtectedPath(PathBuf),

    #[error("Lease conflict: {path} is held by {holder} with {strength} strength")]
    LeaseConflict {
        path: PathBuf,
        holder: String,
        strength: String,
    },

    #[error("Note required for {0} strength lease")]
    NoteRequired(String),

    // Operation failures (exit code 4)
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("Lock acquisition failed: {0}")]
    LockFailed(PathBuf),

    #[error("Merge conflict in {0}")]
    MergeConflict(PathBuf),

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

impl Error {
    /// Get the exit code for this error
    pub fn exit_code(&self) -> i32 {
        match self {
            // User errors
            Error::NotARepo(_)
            | Error::RepoNotFound(_)
            | Error::InvalidConfig(_)
            | Error::InvalidArgument(_)
            | Error::WorkspaceNotFound(_)
            | Error::LeaseNotFound(_) => exit_codes::USER_ERROR,

            // Policy blocks
            Error::ProtectedPath(_)
            | Error::LeaseConflict { .. }
            | Error::NoteRequired(_) => exit_codes::POLICY_BLOCKED,

            // Operation failures
            Error::Git(_)
            | Error::Io(_)
            | Error::Json(_)
            | Error::TomlParse(_)
            | Error::TomlSerialize(_)
            | Error::LockFailed(_)
            | Error::MergeConflict(_)
            | Error::OperationFailed(_) => exit_codes::OPERATION_FAILED,
        }
    }
}

/// Result type alias for sv operations
pub type Result<T> = std::result::Result<T, Error>;

/// Wrapper for displaying errors in JSON format
#[derive(serde::Serialize)]
pub struct JsonError {
    pub error: String,
    pub code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl From<&Error> for JsonError {
    fn from(err: &Error) -> Self {
        JsonError {
            error: err.to_string(),
            code: err.exit_code(),
            details: None,
        }
    }
}

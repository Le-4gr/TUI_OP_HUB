//! Error types for TUI-OP-HUB (US-NF-08).
//!
//! Provides a layered error model:
//! - [`AppError`]: typed domain errors for programmatic handling.
//! - [`anyhow::Error`]: catch-all for application-level propagation.

use thiserror::Error;

/// Top-level application error.
///
/// Every fallible operation in the hub returns `Result<T, AppError>`.
/// Variants carry enough context for the user to understand *what* went
/// wrong and *why*, satisfying US-NF-08 (graceful error handling).
#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("not found: {entity} with id {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("validation error: {0}")]
    Validation(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

/// Convenience type alias used throughout the codebase.
pub type AppResult<T> = Result<T, AppError>;
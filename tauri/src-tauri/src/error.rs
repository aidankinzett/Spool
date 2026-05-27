//! Unified application error type.
//!
//! All backend modules surface errors through [`AppError`]. The type is
//! [`serde::Serialize`] so it round-trips cleanly across the Tauri IPC
//! boundary — the frontend receives the error as a string.

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("library lock poisoned")]
    LockPoisoned,

    #[error("{0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Convenience alias used by Tauri command return types.
pub type AppResult<T> = Result<T, AppError>;

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

    /// Receiver-initiated cancel of an in-flight LAN install. The
    /// download loop sets a cancel flag; tasks return this on the
    /// next poll. The spawn handler maps it to a UI "canceled"
    /// status rather than the red-error path.
    #[error("install cancelled")]
    Canceled,

    /// Host-initiated cancel of an in-flight LAN install — detected
    /// by the heartbeat task seeing 410 Gone on `/cancel-check`.
    /// Treated identically to `Canceled` in terms of cleanup, but
    /// kept separate so the UI can surface "Cancelled by host" if
    /// it ever wants to (currently shows generic "Cancelled").
    #[error("install cancelled by host")]
    HostCanceled,

    /// File arrived but its blake3 digest didn't match the manifest.
    /// The bad file is deleted before the error is raised so a retry
    /// re-fetches from byte 0.
    #[error("checksum mismatch for {path} — expected {expected}, got {actual}")]
    ChecksumMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// True for either flavour of LAN install cancellation. Cleanup
    /// (deleting the `.partial` dir, emitting the canceled event) is
    /// the same for both.
    pub fn is_canceled(&self) -> bool {
        matches!(self, AppError::Canceled | AppError::HostCanceled)
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Convenience alias used by Tauri command return types.
pub type AppResult<T> = Result<T, AppError>;

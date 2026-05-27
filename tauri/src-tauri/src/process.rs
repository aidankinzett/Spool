//! Game process spawn + wait.
//!
//! Currently a thin wrapper around `tokio::process::Command`. Game runs in
//! its own folder as cwd so its relative-path lookups work. Run-as-admin
//! elevation (Windows `runas` shell verb) is a v1.1 follow-up — most
//! user-installed games launch fine as the current user.

use crate::error::{AppError, AppResult};
use std::path::Path;
use tokio::process::Command;

/// Spawns the game and waits for it to exit. Returns the exit code (or -1
/// if the process was killed by a signal / didn't yield a code).
pub async fn run_game(exe_path: &Path) -> AppResult<i32> {
    let cwd = exe_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let mut child = Command::new(exe_path)
        .current_dir(cwd)
        .spawn()
        .map_err(|e| AppError::Other(format!("failed to start game: {e}")))?;

    let status = child
        .wait()
        .await
        .map_err(|e| AppError::Other(format!("failed waiting on game: {e}")))?;

    Ok(status.code().unwrap_or(-1))
}

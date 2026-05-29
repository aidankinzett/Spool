//! Game process spawn + wait.
//!
//! Two launch paths, picked based on the effective Run-As-Admin flag:
//!
//!   - Normal: `tokio::process::Command` — runs as the current user,
//!     async wait, child inherits stdio, cwd set to the exe's dir.
//!   - Elevated (Windows only): `runas` crate → `ShellExecuteExW`
//!     with the `runas` verb. Triggers UAC. Sync wait wrapped in
//!     `tokio::task::spawn_blocking` so we don't block the runtime.
//!
//! Per `m07-concurrency` rule "Blocking code → spawn_blocking": the
//! elevated path holds a blocking thread for the entire game session
//! (potentially hours). That's a tokio blocking-pool thread, which
//! defaults to 512 capacity — one stuck thread is fine. We could
//! switch to direct `ShellExecuteExW` + tokio handle waiting later
//! if the trade-off ever matters.
//!
//! Caveat: the `runas` crate's API doesn't expose `lpDirectory`, so
//! elevated games don't get their exe's directory as cwd. Most games
//! work fine without it (they resolve resources relative to their
//! own exe path). For the small set that need a specific cwd, the
//! workaround is to disable Run-As-Admin and rely on the game's own
//! UAC manifest if any.

use crate::error::{AppError, AppResult};
use crate::proton;
use std::path::Path;
use tokio::process::Command;

/// How to launch a game. The `Native` path is unchanged from before; the
/// `Proton` path wraps the exe in umu-run with a per-game Wine prefix.
pub enum LaunchSpec<'a> {
    Native {
        run_as_admin: bool,
    },
    Proton {
        umu_run: &'a Path,
        prefix_root: &'a Path,
        proton_path: &'a Path,
        game_id: &'a str,
        extra_args: &'a [String],
    },
}

/// Spawns the game and waits for it to exit. Returns the exit code (or -1
/// if the process was killed by a signal / didn't yield a code).
pub async fn run_game(exe_path: &Path, spec: LaunchSpec<'_>) -> AppResult<i32> {
    let cwd = exe_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    match spec {
        LaunchSpec::Native { run_as_admin } => {
            if cfg!(windows) && run_as_admin {
                return run_elevated(exe_path).await;
            }

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
        LaunchSpec::Proton {
            umu_run,
            prefix_root,
            proton_path,
            game_id,
            extra_args,
        } => {
            let launch = proton::build_umu_launch(
                umu_run,
                exe_path,
                extra_args,
                prefix_root,
                proton_path,
                game_id,
            );

            // Block until the game exits — the run workflow's backup phase
            // depends on the real exit. (Notably NOT the detach/quick-exit
            // heuristic some launchers use.)
            let mut child = Command::new(&launch.program)
                .args(&launch.args)
                .envs(launch.env)
                .current_dir(cwd)
                .spawn()
                .map_err(|e| AppError::Other(format!("failed to start game via Proton: {e}")))?;

            let status = child
                .wait()
                .await
                .map_err(|e| AppError::Other(format!("failed waiting on Proton game: {e}")))?;

            Ok(status.code().unwrap_or(-1))
        }
    }
}

/// Spawns the game via ShellExecuteExW with the `runas` verb. Triggers
/// the UAC prompt; blocks the calling thread until the elevated
/// process exits. Wrapped in `spawn_blocking` by the caller so the
/// async runtime keeps moving.
#[cfg(windows)]
async fn run_elevated(exe_path: &Path) -> AppResult<i32> {
    let exe = exe_path.to_path_buf();
    let code = tokio::task::spawn_blocking(move || -> std::io::Result<i32> {
        let status = runas::Command::new(&exe).gui(true).status()?;
        Ok(status.code().unwrap_or(-1))
    })
    .await
    .map_err(|e| AppError::Other(format!("elevated spawn join: {e}")))?
    .map_err(|e| AppError::Other(format!("elevated spawn: {e}")))?;
    Ok(code)
}

#[cfg(not(windows))]
async fn run_elevated(_exe_path: &Path) -> AppResult<i32> {
    Err(AppError::Other(
        "Run-as-administrator is only supported on Windows".into(),
    ))
}

//! Open a filesystem path with the OS's default handler.
//!
//! This replaces the frontend `@tauri-apps/plugin-opener` `openPath` for the
//! "Open folder" actions. The plugin spawns the platform opener (`xdg-open` on
//! Linux) as a child of Spool, **inheriting Spool's environment**. When Spool
//! runs as an AppImage that environment is rewritten to point at the bundled
//! runtime (`LD_LIBRARY_PATH`, `GTK_*`/`GDK_*`, `PYTHONHOME`, …); the host file
//! manager launched by `xdg-open` then inherits those stale bundled libs and
//! fails to start — so "Open folder" silently did nothing on Linux AppImage
//! builds (issue #95).
//!
//! Routing through Rust lets us reuse `process::strip_appimage_env`, the same
//! sanitisation game launches already rely on, so the spawned file manager
//! sees the host environment.

use crate::error::{AppError, AppResult};

/// Open `path` (a file or directory) with the OS default handler.
#[tauri::command]
pub async fn open_path(path: String) -> AppResult<()> {
    open_path_impl(&path)
}

#[cfg(target_os = "linux")]
fn open_path_impl(path: &str) -> AppResult<()> {
    use tokio::process::Command;

    let mut cmd = Command::new("xdg-open");
    cmd.arg(path);
    // Hand the host file manager the host environment, not Spool's AppImage
    // bundle — see the module note and `process::strip_appimage_env`.
    crate::process::strip_appimage_env(&mut cmd);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("failed to open path: {e}")))
}

#[cfg(target_os = "macos")]
fn open_path_impl(path: &str) -> AppResult<()> {
    use tokio::process::Command;

    Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("failed to open path: {e}")))
}

#[cfg(target_os = "windows")]
fn open_path_impl(path: &str) -> AppResult<()> {
    use tokio::process::Command;

    // explorer.exe opens a folder (or selects/launches a file) with the shell
    // default — matching the previous opener-plugin behaviour on Windows.
    Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("failed to open path: {e}")))
}

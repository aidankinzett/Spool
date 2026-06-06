//! Headless entry point — no GUI, no tray, no single-instance.
//!
//! `spool --headless-server` starts the plugin loopback server and runs until
//! killed, dispatched from `run()` before any Tauri setup. It's the persistent
//! IPC endpoint the Decky plugin uses for session/backup/library operations
//! (game-stop backups, the unsynced "release lock" marker, "Back up now"), which
//! replaced the old per-operation `--backup` / `--release-lock` subprocess spawns.

/// Start the plugin loopback server and run until killed. No tray, no window,
/// no single-instance registration.
///
/// Used by the Decky plugin (`spool --headless-server`). The server is
/// Linux/Unix-only; on other platforms this exits immediately with an error.
pub(crate) fn run_headless_server() -> i32 {
    #[cfg(unix)]
    {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!(error = %e, "--headless-server: failed to start tokio runtime");
                return 1;
            }
        };
        rt.block_on(async {
            if let Err(e) = crate::plugin_server::serve().await {
                tracing::error!(error = %e, "--headless-server: exited with error");
                1
            } else {
                0
            }
        })
    }
    #[cfg(not(unix))]
    {
        tracing::error!("--headless-server is only supported on Linux/Unix");
        1
    }
}

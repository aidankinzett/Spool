//! Headless CLI subcommands — no GUI, no tray, no single-instance.
//!
//! These run to completion and return a process exit code, dispatched from
//! `run()` before any Tauri setup:
//!   * `--backup "Name"`        → [`run_backup_headless`]
//!   * `--release-lock "Name"`  → [`run_release_lock_headless`]
//!   * `--headless-server`      → [`run_headless_server`]
//!
//! The first two are the Decky plugin's forced-close fallback (Game Mode kills
//! Spool before its post-session backup); the third is the persistent IPC
//! endpoint that replaced per-operation subprocess spawns.

use crate::config::{self, Config};
use crate::library::Library;
use crate::ludusavi::LudusaviClient;
use crate::{ludusavi_config, paths, rclone, runner, session};
use std::sync::Arc;

/// Headless one-shot backup: load config + library, run ludusavi backup for
/// the named game, mark the session record, then return a process exit code.
/// No GUI / tray / single-instance. Used by `spool --backup "Name"` (the
/// Decky plugin's forced-close fallback).
pub(crate) fn run_backup_headless(game_name: &str) -> i32 {
    let Some(ludusavi_exe) = paths::resolve_ludusavi_path() else {
        tracing::error!("--backup: ludusavi sidecar not found");
        return 1;
    };

    // Make sure Spool's ludusavi config (backup path, cloud remote) exists.
    if let Err(e) = ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "--backup: ensure_config failed");
    }

    let config_dir = paths::ludusavi_config_dir();
    let client = LudusaviClient::new();

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "--backup: failed to start tokio runtime");
            return 1;
        }
    };

    // Open the shared library DB and resolve the game id.
    let prepared = rt.block_on(async {
        let library = match Library::open().await {
            Ok(l) => Arc::new(l),
            Err(e) => {
                tracing::error!(error = %e, "--backup: failed to open library");
                return None;
            }
        };
        match library.find_id_by_name(game_name).await {
            Ok(Some(id)) => Some((library, id)),
            _ => {
                tracing::error!(name = %game_name, "--backup: no library entry matches");
                None
            }
        }
    });
    let Some((lib_state, game_id)) = prepared else {
        return 1;
    };

    // Capture the active session id BEFORE the (slow, async) backup so we only
    // touch THIS session's record afterward. A new game can launch during the
    // backup and overwrite active-session.json with a fresh session_id; an
    // unguarded update would corrupt the new session's state, and the Decky
    // forced-close fallback keys its decision on `backed_up`. Filter by game name
    // so we never act on a different game's record. (#273)
    let session_id = session::read()
        .filter(|s| s.game == game_name)
        .map(|s| s.session_id);

    let result = rt.block_on(async {
        runner::backup_game_core(&client, &ludusavi_exe, &config_dir, &lib_state, &game_id).await
    });
    let cfg_data = config::Config::load().map(|c| c.data).unwrap_or_default();

    match result {
        Ok(r) => {
            tracing::info!(game_name, games = r.game_count, "--backup complete");
            if r.cloud_synced {
                // Fully reconciled — clear the record so a later "Back up now" /
                // game-stop can't act on this already-synced session. (#280)
                if let Some(id) = &session_id {
                    session::clear_if(id);
                }
                rt.block_on(async {
                    rclone::complete_session_backup_from_config(&cfg_data, game_name).await;
                });
            } else {
                // Local backup landed but the cloud upload failed — keep the
                // record (flagged backed_up) so peers/next-launch reconcile, and
                // leave the unsynced-session marker in place.
                if let Some(id) = &session_id {
                    session::mark_backed_up_if(id);
                }
                tracing::warn!(game_name, "--backup: cloud upload failed — leaving session marker in place");
            }
            0
        }
        Err(e) => {
            tracing::error!(error = %e, "--backup failed");
            1
        }
    }
}

/// Headless one-shot: flip the named game's unsynced-session marker to
/// `pending-backup`, then return a process exit code. No GUI / tray.
///
/// Used by `spool --release-lock "Name"` — the Decky plugin's forced-close
/// fallback runs this *before* `--backup` so peers immediately see "this
/// device has unsynced saves" the moment Steam kills Spool, independent of
/// whether the subsequent backup succeeds. The follow-up `--backup` deletes
/// the marker once the saves actually reach the cloud. No-op (success) when
/// cloud saves aren't configured.
pub(crate) fn run_release_lock_headless(game_name: &str) -> i32 {
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "--release-lock: failed to load config");
            return 1;
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "--release-lock: failed to start tokio runtime");
            return 1;
        }
    };
    rt.block_on(async {
        rclone::mark_session_pending_backup_from_config(&config.data, game_name).await;
    });
    tracing::info!(game_name, "--release-lock complete");
    0
}

/// Start the plugin Unix socket server and run until killed. No tray, no
/// window, no single-instance registration.
///
/// Used by the Decky plugin (`spool --headless-server`) to get a persistent
/// IPC endpoint for session/backup/library queries — replacing the old
/// per-operation `--backup` / `--release-lock` subprocess spawns. The server
/// is Linux/Unix-only; on other platforms this exits immediately with an error.
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

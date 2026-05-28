//! Run workflow — the marquee feature.
//!
//! Orchestrates the five-phase game launch:
//!
//!   restoring → launching → playing → backing-up → done
//!
//! Each transition emits a `run:phase` event so the UI can update the
//! Play button label. Cloud-sync conflicts during restore are surfaced
//! as an error phase and the launch aborts (we won't blindly overwrite
//! the user's cloud save). Backup failures after a successful play
//! session are logged but don't fail the workflow — the game ran fine
//! and the user shouldn't see a red toast for a flaky network call.
//!
//! Single-launch model: only one game can be running at a time. A second
//! `launch_game` while a workflow is in flight returns an error rather
//! than overlapping. This matches the cassette-shelf metaphor (one tape
//! in the deck) and avoids two restores trampling the same save dir.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use crate::ludusavi::LudusaviClient;
use crate::sync::{self, AcquireOutcome};
use crate::{process, paths, registry};
use chrono::Utc;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

// `paths` import retained for future log-file work; not used yet.
#[allow(unused_imports)]
use paths as _paths;

/// Shared runner state. v1 only tracks "is anything running?" — when we
/// allow concurrent launches for different games this becomes a HashMap.
#[derive(Default)]
pub struct RunState {
    current: Mutex<Option<String>>,
}

impl RunState {
    fn try_acquire(&self, game_id: &str) -> AppResult<RunGuard<'_>> {
        let mut guard = self.current.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(running) = guard.as_ref() {
            return Err(AppError::Other(format!(
                "Another game is already running (id {running})"
            )));
        }
        *guard = Some(game_id.to_string());
        Ok(RunGuard { state: self })
    }
}

/// RAII guard — drops the running-id when the workflow finishes (or
/// panics). Without this a crashed workflow would leave the slot occupied
/// and the user could never launch another game until they restarted Spool.
struct RunGuard<'a> {
    state: &'a RunState,
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.state.current.lock() {
            *guard = None;
        }
    }
}

#[derive(Debug, Serialize, Clone)]
struct RunPhaseEvent {
    game_id: String,
    phase: String,
    message: Option<String>,
}

fn emit_phase(app: &AppHandle, game_id: &str, phase: &str, message: Option<&str>) {
    let payload = RunPhaseEvent {
        game_id: game_id.to_string(),
        phase: phase.to_string(),
        message: message.map(String::from),
    };
    if let Err(e) = app.emit("run:phase", &payload) {
        tracing::warn!(phase = phase, error = %e, "failed to emit run:phase");
    }
}

/// Fires a native OS notification, but only when the main Spool
/// window isn't visible — otherwise we'd double up with the in-app
/// toasts the frontend renders for the same run-phase events.
///
/// The intent is: while the user is in-game (Spool hidden / tray-
/// resident), they get desktop toasts in the OS notification centre
/// for things they need to know about ("saves backed up", "launch
/// failed"). While Spool's window is in focus they see the in-app
/// toast instead.
///
/// Best-effort: a notification failure is logged and otherwise
/// ignored — never blocks the workflow.
fn os_toast_if_hidden(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;

    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        return;
    }

    if let Err(e) = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        tracing::warn!(error = %e, "OS toast failed");
    }
}

/// Manual backup — runs `ludusavi backup` for a single game outside
/// the full play workflow. Used by the right-click "Back up saves
/// now" action so users can snapshot saves before risky operations
/// without launching the game.
///
/// Returns the count of backup bundles ludusavi produced and the
/// total size in bytes. Persists the entry's
/// `save_last_backed_up_at` / `save_backup_count` / `save_backup_size_mb`
/// the same way the post-session backup phase does, then records a
/// sync event so peers can see the new save.
#[tauri::command]
pub async fn manual_backup(app: AppHandle, game_id: String) -> AppResult<ManualBackupResult> {
    // Snapshot config + library entry first; drop the locks before
    // the long-running subprocess await.
    let (game_name, ludusavi_exe) = manual_prep(&app, &game_id)?;
    let ludusavi_client = app.state::<LudusaviClient>();
    let out = ludusavi_client
        .backup(&ludusavi_exe, &game_name)
        .await
        .map_err(|e| AppError::Other(format!("ludusavi backup: {e}")))?;

    let (game_count, bytes_total) = out
        .overall
        .as_ref()
        .map(|o| (o.total_games as i32, o.total_bytes))
        .unwrap_or((0, 0));

    if game_count > 0 {
        {
            let library = app.state::<SharedLibrary>();
            let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
            if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
                entry.save_backup_count += 1;
                entry.save_last_backed_up_at = Some(Utc::now());
                entry.save_backup_size_mb = (bytes_total as f64) / (1024.0 * 1024.0);
                entry.sync_badge = Some("synced".to_string());
            }
            lib.save()?;
        }
        let _ = app.emit("library:changed", &game_id);
        // Record on the sync server so peers see the new event.
        sync::record_backup_event(&app, &game_name).await;
    }

    Ok(ManualBackupResult {
        game_count,
        bytes_total: bytes_total as u64,
    })
}

/// Manual restore — runs `ludusavi restore` for a single game.
/// Surfaces cloud-sync conflicts as an explicit error so the UI can
/// prompt the user to open Ludusavi (same behaviour as the launch
/// path).
#[tauri::command]
pub async fn manual_restore(app: AppHandle, game_id: String) -> AppResult<ManualRestoreResult> {
    let (game_name, ludusavi_exe) = manual_prep(&app, &game_id)?;
    let ludusavi_client = app.state::<LudusaviClient>();
    let out = ludusavi_client
        .restore(&ludusavi_exe, &game_name)
        .await
        .map_err(|e| AppError::Other(format!("ludusavi restore: {e}")))?;

    if out
        .errors
        .as_ref()
        .and_then(|e| e.cloud_conflict.as_ref())
        .is_some()
    {
        return Err(AppError::Other(
            "Cloud sync conflict — open Ludusavi to resolve before restoring.".into(),
        ));
    }

    let game_count = out
        .overall
        .as_ref()
        .map(|o| o.total_games as i32)
        .unwrap_or(0);

    // Record the restore on the sync server so peers know we just
    // pulled the latest. Best-effort.
    if game_count > 0 {
        sync::record_restore_event(&app, &game_name).await;
    }

    Ok(ManualRestoreResult { game_count })
}

#[derive(Debug, Serialize)]
pub struct ManualBackupResult {
    pub game_count: i32,
    pub bytes_total: u64,
}

#[derive(Debug, Serialize)]
pub struct ManualRestoreResult {
    pub game_count: i32,
}

/// Shared snapshot for the manual backup/restore commands. Pulls the
/// game name + validated ludusavi path so the caller has everything
/// it needs to await on the subprocess.
fn manual_prep(app: &AppHandle, game_id: &str) -> AppResult<(String, PathBuf)> {
    let game_name = {
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        entry.game_name.clone()
    };
    let ludusavi_exe = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        let p = cfg.data.ludusavi_path.clone();
        if p.is_empty() || !PathBuf::from(&p).is_file() {
            return Err(AppError::Other(
                "Ludusavi is not configured. Set its path in Settings.".into(),
            ));
        }
        PathBuf::from(p)
    };
    Ok((game_name, ludusavi_exe))
}

#[tauri::command]
pub async fn launch_game(app: AppHandle, game_id: String) -> AppResult<()> {
    launch_game_inner(&app, &game_id).await
}

/// Inner launch function callable from non-command contexts (e.g. the
/// `tauri-plugin-single-instance` callback when a forwarded `--run` arrives).
/// Same behaviour as the `launch_game` command — single-launch guard +
/// full workflow + phase emission.
pub async fn launch_game_inner(app: &AppHandle, game_id: &str) -> AppResult<()> {
    let run_state = app.state::<RunState>();
    let _guard = run_state.try_acquire(game_id)?;

    // Snapshot what we need from state up front so we don't hold any
    // sync Mutex across the long-running awaits below. We also fold
    // the registry-level Run-As-Admin compat flag into the effective
    // `needs_admin` here so the launch path doesn't have to know
    // about the registry concept.
    let (game_name, exe_path, needs_admin) = {
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        if entry.exe_path.is_empty() {
            return Err(AppError::Other("Game has no executable configured".into()));
        }
        let needs_admin =
            entry.run_as_admin || registry::run_as_admin_in_registry(&entry.exe_path);
        (entry.game_name.clone(), entry.exe_path.clone(), needs_admin)
    };

    let ludusavi_exe = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        let p = cfg.data.ludusavi_path.clone();
        if p.is_empty() || !PathBuf::from(&p).is_file() {
            return Err(AppError::Other(
                "Ludusavi is not configured. Set its path in Settings.".into(),
            ));
        }
        PathBuf::from(p)
    };

    let ludusavi_client = app.state::<LudusaviClient>();
    let result = run_workflow(
        app,
        game_id,
        &game_name,
        &exe_path,
        needs_admin,
        &ludusavi_exe,
        &ludusavi_client,
    )
    .await;

    if let Err(e) = &result {
        emit_phase(app, game_id, "error", Some(&e.to_string()));
        // Surface the failure via the OS notification centre too —
        // most workflow errors happen while the user is mid-launch
        // with Spool tucked into the tray.
        os_toast_if_hidden(
            app,
            "Spool: launch failed",
            &format!("{game_name} — {e}"),
        );
    }
    result
}

async fn run_workflow(
    app: &AppHandle,
    game_id: &str,
    game_name: &str,
    exe_path: &str,
    run_as_admin: bool,
    ludusavi_exe: &Path,
    ludusavi_client: &LudusaviClient,
) -> AppResult<()> {
    tracing::info!(game_id, game_name, "starting run workflow");

    // ── Phase 1: restore ──────────────────────────────────────────────
    emit_phase(app, game_id, "restoring", Some("Restoring saves…"));
    os_toast_if_hidden(
        app,
        "Restoring saves",
        &format!("{game_name} — restoring before launch"),
    );
    tracing::info!(game_name, "ludusavi restore");
    let restore = ludusavi_client.restore(ludusavi_exe, game_name).await?;
    if restore
        .errors
        .as_ref()
        .and_then(|e| e.cloud_conflict.as_ref())
        .is_some()
    {
        return Err(AppError::Other(
            "Cloud sync conflict — open Ludusavi to resolve before launching.".into(),
        ));
    }
    // "No saves to restore" is fine — game just hasn't been played yet.
    let no_saves = restore
        .errors
        .as_ref()
        .map(|e| !e.unknown_games.is_empty())
        .unwrap_or(false)
        || restore
            .overall
            .as_ref()
            .map(|o| o.total_games == 0)
            .unwrap_or(false);

    // Record the restore event on the sync server (best-effort, fires
    // only when sync is configured + reachable). The server uses these
    // events to power the cross-device save sync status badges.
    sync::record_restore_event(app, game_name).await;

    // ── Phase 1.5: acquire play-state lock ────────────────────────────
    // Asks the sync server to lock this game to this device so a
    // second device can't simultaneously launch and trample the
    // save. No-op when sync is disabled / unreachable; only blocks on
    // a real 409 conflict.
    match sync::acquire_lock(app, game_name).await {
        AcquireOutcome::Acquired => {}
        AcquireOutcome::Conflict { device_name } => {
            return Err(AppError::Other(format!(
                "Already playing on {device_name}. Close it there before launching here."
            )));
        }
    }

    // ── Phase 2: launch + wait ───────────────────────────────────────
    emit_phase(app, game_id, "launching", Some("Launching game…"));
    let exe_pathbuf = PathBuf::from(exe_path);
    if !exe_pathbuf.is_file() {
        return Err(AppError::Other(format!(
            "Game executable not found at {exe_path}"
        )));
    }

    emit_phase(app, game_id, "playing", None);
    tracing::info!(exe_path, run_as_admin, "launching game process");
    let session_start = Utc::now();

    // Spawn the lock-heartbeat task. Pings /heartbeat every 30s so
    // the sync server doesn't mark our lock stale during long
    // sessions. Aborted unconditionally on exit so it doesn't
    // outlive the game.
    let heartbeat = sync::start_heartbeat(app.clone(), game_name.to_string());

    let spawn_result = process::run_game(&exe_pathbuf, run_as_admin).await;
    let session_end = Utc::now();

    // Always abort the heartbeat + release the lock — even if launch
    // failed mid-spawn. Lock release is fire-and-forget; the server
    // stale-detection would eventually reclaim a missed release but
    // we want the next device to be able to launch immediately.
    heartbeat.abort();
    sync::release_lock(app, game_name).await;

    tracing::info!(
        game_name,
        duration_min = (session_end - session_start).num_minutes(),
        "game exited"
    );

    if let Err(e) = spawn_result {
        return Err(AppError::Other(format!("Game failed to launch: {e}")));
    }

    // ── Update last_played + playtime (best-effort) ───────────────────
    let session_minutes = (session_end - session_start).num_minutes().max(0) as i32;
    {
        let library = app.state::<SharedLibrary>();
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
            entry.last_played_at = Some(session_end);
            entry.playtime_minutes += session_minutes;
        }
        lib.save()?;
    }
    let _ = app.emit("library:changed", &game_id.to_string());

    // Cross-device sync — push the session timestamps to the server
    // so other devices pick them up on their next startup_sync. The
    // timestamp is RFC 3339 / ISO 8601; the server requires the
    // playtime delta to be a positive integer, which `push_playtime_delta`
    // already enforces.
    sync::push_last_played(app, game_name, &session_end.to_rfc3339()).await;
    sync::push_playtime_delta(app, game_name, session_minutes).await;

    // ── Phase 3: backup (skip if ludusavi didn't recognise the game) ──
    if !no_saves {
        emit_phase(app, game_id, "backing-up", Some("Backing up saves…"));
        os_toast_if_hidden(
            app,
            "Backing up saves",
            &format!("{game_name} — session ended"),
        );
        tracing::info!(game_name, "ludusavi backup");
        match ludusavi_client.backup(ludusavi_exe, game_name).await {
            Ok(out) => {
                if let Some(overall) = &out.overall {
                    if overall.total_games > 0 {
                        let library = app.state::<SharedLibrary>();
                        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
                        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
                            entry.save_backup_count += 1;
                            entry.save_last_backed_up_at = Some(Utc::now());
                            entry.save_backup_size_mb =
                                (overall.total_bytes as f64) / (1024.0 * 1024.0);
                        } else {
                            tracing::warn!(game_id, "backup stats not persisted: library entry missing after session");
                        }
                        lib.save()?;
                        let _ = app.emit("library:changed", &game_id.to_string());
                    }
                }
                // Tell the sync server we backed up — peers can use
                // this to know they're behind on saves. Best-effort.
                sync::record_backup_event(app, game_name).await;

                // Mark the entry as synced. After a successful backup
                // we ARE the most recent device on the server (assuming
                // the event recorded). If the event recording failed
                // silently (offline), the badge will flip to
                // local-newer on the next startup_sync.
                let library = app.state::<SharedLibrary>();
                let badge_changed = if let Ok(mut lib) = library.lock() {
                    let mut changed = false;
                    if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
                        if entry.sync_badge.as_deref() != Some("synced") {
                            entry.sync_badge = Some("synced".to_string());
                            changed = true;
                        }
                    }
                    if changed {
                        let _ = lib.save();
                    }
                    changed
                } else {
                    false
                };
                drop(library);
                if badge_changed {
                    let _ = app.emit("library:changed", &game_id.to_string());
                }
            }
            Err(e) => {
                // Don't fail the workflow — the user already played the game
                // successfully and getting a red toast for a flaky network
                // call would be misleading. Surface it in the log instead.
                tracing::warn!(game_id = %game_id, error = %e, "post-session backup failed");
            }
        }
    }

    emit_phase(app, game_id, "done", None);
    // Final completion ping — the most useful native toast since the
    // user may have closed the game and walked away from the PC.
    os_toast_if_hidden(
        app,
        "Saves backed up",
        &format!("{game_name} — session complete"),
    );
    tracing::info!(game_name, "run workflow complete");
    Ok(())
}

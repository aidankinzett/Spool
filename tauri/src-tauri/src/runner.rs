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
use crate::{process, paths};
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
    // sync Mutex across the long-running awaits below.
    let (game_name, exe_path) = {
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        if entry.exe_path.is_empty() {
            return Err(AppError::Other("Game has no executable configured".into()));
        }
        (entry.game_name.clone(), entry.exe_path.clone())
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
    let result =
        run_workflow(app, game_id, &game_name, &exe_path, &ludusavi_exe, &ludusavi_client).await;

    if let Err(e) = &result {
        emit_phase(app, game_id, "error", Some(&e.to_string()));
    }
    result
}

async fn run_workflow(
    app: &AppHandle,
    game_id: &str,
    game_name: &str,
    exe_path: &str,
    ludusavi_exe: &Path,
    ludusavi_client: &LudusaviClient,
) -> AppResult<()> {
    tracing::info!(game_id, game_name, "starting run workflow");

    // ── Phase 1: restore ──────────────────────────────────────────────
    emit_phase(app, game_id, "restoring", Some("Restoring saves…"));
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

    // ── Phase 2: launch + wait ───────────────────────────────────────
    emit_phase(app, game_id, "launching", Some("Launching game…"));
    let exe_pathbuf = PathBuf::from(exe_path);
    if !exe_pathbuf.is_file() {
        return Err(AppError::Other(format!(
            "Game executable not found at {exe_path}"
        )));
    }

    emit_phase(app, game_id, "playing", None);
    tracing::info!(exe_path, "launching game process");
    let session_start = Utc::now();
    let spawn_result = process::run_game(&exe_pathbuf).await;
    let session_end = Utc::now();
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

    // ── Phase 3: backup (skip if ludusavi didn't recognise the game) ──
    if !no_saves {
        emit_phase(app, game_id, "backing-up", Some("Backing up saves…"));
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
    tracing::info!(game_name, "run workflow complete");
    Ok(())
}

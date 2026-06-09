use super::PluginState;
use axum::{extract::State as AxState, response::Json};
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) async fn get_session() -> Json<Value> {
    match crate::session::read() {
        None => Json(json!({ "hasSession": false })),
        Some(rec) => Json(json!({
            "hasSession": true,
            "game": rec.game,
            "backedUp": rec.backed_up,
            "startedAt": rec.started_at,
        })),
    }
}

#[derive(Deserialize)]
pub(super) struct GameStoppedRequest {
    appid: u32,
}

/// Called by the plugin on every game-stop event. Checks whether the session
/// record matches `appid` and hasn't been backed up yet; if so, releases the
/// play lock then runs a backup — the same forced-close fallback logic that
/// `backup_logic.py::should_backup` + `main.py::on_app_stop` used to perform
/// via subprocesses.
pub(super) async fn post_game_stopped(
    AxState(state): AxState<PluginState>,
    Json(body): Json<GameStoppedRequest>,
) -> Json<Value> {
    // Stamp the session end at the moment the game-stop event arrives, BEFORE
    // the (network) marker write and backup, so the recorded duration reflects
    // the real game-stop instant rather than the plugin/rclone latency. (#5)
    let ended_at = chrono::Utc::now();

    let Some(rec) = crate::session::read() else {
        return Json(json!({ "acted": false }));
    };

    // The JS frontend coerces unAppID to unsigned with `>>> 0` before sending,
    // so body.appid and rec.steam_appid are both u32 — compare directly.
    if rec.backed_up || rec.steam_appid != body.appid {
        tracing::info!(
            appid = body.appid,
            session_appid = rec.steam_appid,
            backed_up = rec.backed_up,
            "plugin: game-stopped no-op",
        );
        return Json(json!({ "acted": false }));
    }

    tracing::info!(game = %rec.game, "plugin: forced-close fallback triggered");

    // Reload config fresh so any path/server changes made in the GUI are seen.
    let config = crate::config::Config::load().unwrap_or_default();

    // Flag this device's session as unsynced first — independent of the backup
    // result, so peers immediately see that this device has saves not yet in
    // the cloud. The backup below clears the marker once the upload lands.
    crate::rclone::mark_session_pending_backup_from_config(&config.data, &rec.game).await;

    // Reuse the config already loaded above rather than reloading inside run_backup.
    run_backup(
        &state,
        &rec.game,
        &rec.session_id,
        Some((rec.started_at, rec.suspended_secs, ended_at)),
        config,
    )
    .await
}

/// Manual backup from the QAM "Back up now" button. No appid check; no lock
/// release — just backs up whatever game the current session record points to.
pub(super) async fn post_backup_now(AxState(state): AxState<PluginState>) -> Json<Value> {
    let Some(rec) = crate::session::read() else {
        return Json(json!({ "acted": false, "ok": false, "reason": "no active session" }));
    };
    // "Back up now" can fire mid-session — pass None so we don't record a
    // premature/short play session; the real game-stop path records it.
    let config = crate::config::Config::load().unwrap_or_default();
    run_backup(&state, &rec.game, &rec.session_id, None, config).await
}

/// Run the forced-close backup for `game_name`/`session_id`, using the
/// already-loaded `config` (callers reload it once, not twice). When `session`
/// is `Some((started_at, suspended_secs, ended_at))`, this is the real game-stop
/// path (`ended_at` is stamped by the caller at event arrival, before the marker
/// write, so it reflects the real game-stop instant — #5), so the play session
/// that the SIGKILLed workflow never recorded is written here
/// too (playtime + history row + cross-device playtime, with sleep time
/// subtracted) and the session record is reconciled on completion. The manual
/// "Back up now" passes `None` — the game may still be running, so recording a
/// session or clearing the record then would be premature (#3); it just backs
/// up saves.
async fn run_backup(
    state: &PluginState,
    game_name: &str,
    session_id: &str,
    session: Option<(chrono::DateTime<chrono::Utc>, i64, chrono::DateTime<chrono::Utc>)>,
    config: crate::config::Config,
) -> Json<Value> {
    let config_dir = crate::paths::ludusavi_config_dir();

    let Some(game_id) = state.library.find_id_by_name(game_name, None).await.ok().flatten() else {
        tracing::error!(name = %game_name, "plugin backup: game not in library");
        return Json(json!({ "acted": true, "ok": false, "reason": "game not in library" }));
    };

    // Only the real game-stop path manages the session record's lifecycle.
    // "Back up now" (None) fires while the game is still running, so it must
    // leave the record in place — clearing it here would make the later
    // game-stop see no session and record nothing. (#3)
    let is_game_stop = session.is_some();

    // Record the play session + local + cross-device playtime FIRST, independent
    // of the backup outcome — a session happened whether or not the backup
    // succeeds, mirroring the in-process path which records in phase_launch
    // *before* phase_backup. It shares the in-process session_id recipe
    // (session::write_start's started_at), so if the workflow already recorded
    // this session `record_session_headless` dedupes and is a no-op. It pushes
    // the cross-device playtime itself (gated on the row insert), so the backup
    // branches below only do the backup-completion side effects. The manual
    // "Back up now" path (None) skips this — recording mid-session is premature.
    if let Some((started, suspended_secs, ended_at)) = session {
        crate::runner::record_session_headless(
            &state.library,
            &config.data,
            game_name,
            started,
            ended_at,
            suspended_secs,
        )
        .await;
    }

    let Some(ludusavi_exe) = crate::paths::resolve_ludusavi_path() else {
        tracing::error!("plugin backup: ludusavi sidecar not found");
        // The session is already recorded above; we just can't back up saves.
        return Json(
            json!({ "acted": true, "ok": false, "reason": "ludusavi sidecar not found" }),
        );
    };
    if let Err(e) = crate::ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "plugin backup: ensure_config warning");
    }

    // NB: backup_game_core takes the machine-wide backup lock. An in-process
    // workflow may still be mid-backup and flip `backed_up` while we wait on it,
    // so this can do a redundant backup — harmless: the shared session_id means
    // no duplicate row, and `record_session_headless` already pushed playtime
    // exactly once (gated on the row insert), so it isn't double-counted; the
    // worst case is re-uploading identical saves (force-overwrite).
    match crate::runner::backup_game_core(
        state.ludusavi.as_ref(),
        &ludusavi_exe,
        &config_dir,
        &state.library,
        &game_id,
    )
    .await
    {
        Ok(r) => {
            // Playtime was already recorded above (gated on the row insert).
            // Here we only finish the backup and reconcile the record — and the
            // record only on the real game-stop path (#3). Both record mutations
            // guard on `session_id` so a new game starting while this async
            // backup was in flight is never clobbered.
            if r.cloud_synced {
                // Fully reconciled — drop the record so a later "Back up now" /
                // game-stop can't act on this already-synced session (the record
                // is never otherwise deleted, only overwritten on next launch).
                // (#280)
                if is_game_stop {
                    crate::session::clear_if(session_id);
                }
                // Clear the unsynced marker + stamp this device as latest backer.
                crate::rclone::complete_session_backup_from_config(&config.data, game_name).await;
                tracing::info!(game = %game_name, "plugin backup: complete");
            } else {
                // Local backup landed but the upload failed or hit a conflict —
                // keep the record flagged so peers keep warning until a real sync
                // happens. This is the forced-close fallback, so a flaky Deck
                // Wi-Fi must not silently drop the "unsynced session" signal.
                if is_game_stop {
                    crate::session::mark_backed_up_if(session_id);
                }
                tracing::warn!(game = %game_name, "plugin backup: cloud upload failed — leaving session marker in place");
            }
            Json(json!({ "acted": true, "ok": true, "game": game_name, "cloud_synced": r.cloud_synced }))
        }
        Err(e) => {
            // The session (incl. cross-device playtime) is already recorded; the
            // PendingBackup marker stays so peers/next-launch reconcile.
            tracing::error!(error = %e, game = %game_name, "plugin backup: failed");
            Json(
                json!({ "acted": true, "ok": false, "game": game_name, "reason": e.to_string() }),
            )
        }
    }
}

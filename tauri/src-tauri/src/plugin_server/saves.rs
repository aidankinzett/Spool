use super::PluginState;
use axum::{
    extract::{Path as AxPath, State as AxState},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// Pull cloud saves for a game down to this device and restore them to disk,
/// without launching. Mirrors the desktop `pull_cloud_saves` command via the
/// shared [`crate::runner::pull_cloud_saves_core`]. Loads the library fresh (the
/// GUI may also be running) and reports the outcome so the Decky UI can toast
/// "Pulled latest saves" / "Already up to date" / "Local saves are newer", or a
/// conflict the user must resolve in the desktop app.
pub(super) async fn post_pull_cloud_saves(AxPath(id): AxPath<String>, AxState(state): AxState<PluginState>) -> Json<Value> {
    let Some(ludusavi_exe) = crate::paths::resolve_ludusavi_path() else {
        return Json(json!({ "ok": false, "reason": "ludusavi sidecar not found" }));
    };
    if let Err(e) = crate::ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "plugin pull: ensure_config warning");
    }
    let config_dir = crate::paths::ludusavi_config_dir();

    match crate::runner::pull_cloud_saves_core(
        state.ludusavi.as_ref(),
        &ludusavi_exe,
        &config_dir,
        &state.library,
        &id,
    )
    .await
    {
        Ok(r) => {
            let outcome = serde_json::to_value(r.outcome)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            tracing::info!(game_id = %id, outcome = %outcome, "plugin pull: complete");
            Json(json!({ "ok": true, "outcome": outcome, "game_count": r.game_count }))
        }
        Err(e) => {
            tracing::warn!(game_id = %id, error = %e, "plugin pull: failed");
            Json(json!({ "ok": false, "reason": e.to_string() }))
        }
    }
}

/// Lists the save revisions ludusavi retains locally for a game, newest-first,
/// with the tip flagged. Backs the Game-Mode "restore an earlier save" picker.
/// Mirrors the desktop `list_save_revisions` command.
pub(super) async fn get_revisions(
    AxPath(id): AxPath<String>,
    AxState(state): AxState<PluginState>,
) -> Json<Value> {
    let Some(ludusavi_exe) = crate::paths::resolve_ludusavi_path() else {
        return Json(json!({ "ok": false, "reason": "ludusavi sidecar not found" }));
    };
    if let Err(e) = crate::ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "plugin revisions: ensure_config warning");
    }
    let config_dir = crate::paths::ludusavi_config_dir();
    let Some(entry) = state.library.find(&id).await.ok().flatten() else {
        return Json(json!({ "ok": false, "reason": "game not in library" }));
    };
    match state
        .ludusavi
        .list_revisions(&ludusavi_exe, &config_dir, &entry.game_name)
        .await
    {
        Ok(revisions) => Json(json!({ "ok": true, "revisions": revisions })),
        Err(e) => {
            tracing::warn!(game_id = %id, error = %e, "plugin revisions: failed");
            Json(json!({ "ok": false, "reason": e.to_string() }))
        }
    }
}

#[derive(Deserialize)]
pub(super) struct RestoreRequest {
    /// ludusavi backup id to roll back to (a `SaveRevision.name`).
    backup_name: String,
}

/// Rolls a game back to an earlier save revision and pins it as the new tip.
/// Mirrors the desktop `restore_save_revision` command (via the shared
/// `restore_save_revision_core`). Note: unlike the command, this isn't behind
/// the single-launch run-lock — the plugin server is a separate process, so its
/// lock wouldn't coordinate with the GUI or an attached `spool --run`. In Game
/// Mode the user triggers this from the game's page while it isn't running, so
/// racing a live session is unlikely.
pub(super) async fn post_restore(
    AxPath(id): AxPath<String>,
    AxState(state): AxState<PluginState>,
    Json(req): Json<RestoreRequest>,
) -> Json<Value> {
    let Some(ludusavi_exe) = crate::paths::resolve_ludusavi_path() else {
        return Json(json!({ "ok": false, "reason": "ludusavi sidecar not found" }));
    };
    if let Err(e) = crate::ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "plugin restore: ensure_config warning");
    }
    let config = crate::config::Config::load().unwrap_or_default();
    let config_dir = crate::paths::ludusavi_config_dir();

    match crate::runner::restore_save_revision_core(
        state.ludusavi.as_ref(),
        &ludusavi_exe,
        &config_dir,
        &state.library,
        &config.data,
        &id,
        &req.backup_name,
    )
    .await
    {
        Ok(r) => {
            tracing::info!(game_id = %id, backup = %req.backup_name, "plugin restore: complete");
            Json(json!({ "ok": true, "game_count": r.game_count }))
        }
        Err(e) => {
            tracing::warn!(game_id = %id, error = %e, "plugin restore: failed");
            Json(json!({ "ok": false, "reason": e.to_string() }))
        }
    }
}

/// Triggers a cross-device rclone fold and waits for it to complete. The
/// Decky UI calls this on game-page navigation so playtime and last-played
/// are fresh without requiring the full Spool GUI to be running.
pub(super) async fn post_fold() -> Json<Value> {
    let changed = crate::rclone::fold_devices_from_config().await;
    Json(json!({ "changed": changed }))
}

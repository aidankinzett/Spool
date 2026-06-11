use super::PluginState;
use axum::{
    extract::{Path as AxPath, State as AxState},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};

pub(super) async fn get_library(AxState(state): AxState<PluginState>) -> Json<Value> {
    let entries = state.library.list().await.unwrap_or_default();
    let spool_exe = crate::paths::spool_executable()
        .or_else(|| std::env::current_exe().ok())
        .map(|p| p.to_string_lossy().to_string());
    let entries: Vec<Value> = entries
        .iter()
        .filter_map(|entry| match serde_json::to_value(entry) {
            Ok(mut v) => {
                if let (Some(map), Some(exe)) = (v.as_object_mut(), &spool_exe) {
                    let app_id = crate::steam::compute_shortcut_app_id(&entry.game_name, exe);
                    map.insert("shortcut_app_id".to_string(), json!(app_id));
                }
                Some(v)
            }
            Err(e) => {
                tracing::warn!(game_id = %entry.id, game_name = %entry.game_name, error = %e, "get_library: failed to serialize entry");
                None
            }
        })
        .collect();
    Json(json!(entries))
}

/// Deletes a game's install folder from disk and removes its library entry.
/// Mirrors the desktop `delete_game_from_disk` command. Loads the library
/// fresh (the GUI may also be running), applies the same folder-safety guards,
/// and saves atomically. Returns `{ ok: true }` on success.
pub(super) async fn delete_game(
    AxState(state): AxState<PluginState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, StatusCode> {
    if !state.library_available {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    match crate::library::delete_game_core(&state.library, &id).await {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => {
            tracing::warn!(game_id = %id, error = %e, "plugin: delete_game failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Removes a game's install folder from disk but KEEPS its library entry
/// (dimmed, not launchable until re-added). Mirrors the desktop `uninstall_game`
/// command: backs the saves up first (the wipe also deletes the Proton prefix),
/// aborting the uninstall if that backup fails. Returns `{ ok: true }` on
/// success, or `{ ok: false, reason }` (so the Decky toast shows why).
pub(super) async fn post_uninstall_game(
    AxState(state): AxState<PluginState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, StatusCode> {
    if !state.library_available {
        return Ok(Json(
            json!({ "ok": false, "reason": "library unavailable" }),
        ));
    }
    let (ludusavi_exe, config_dir) = match super::ludusavi_prep("plugin uninstall") {
        Ok(pair) => pair,
        Err(reason) => return Ok(Json(json!({ "ok": false, "reason": reason }))),
    };
    match crate::runner::uninstall_game_with_backup(
        &state.ludusavi,
        &ludusavi_exe,
        &config_dir,
        &state.library,
        &id,
    )
    .await
    {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => {
            tracing::warn!(game_id = %id, error = %e, "plugin: uninstall_game failed");
            Ok(Json(json!({ "ok": false, "reason": e.to_string() })))
        }
    }
}

/// Forgets a game's library entry but leaves its files on disk. Mirrors the
/// desktop `remove_game` command. Returns `{ ok: true }` when a row was
/// removed, `{ ok: false, reason }` when the id wasn't in the library (so the
/// plugin doesn't drop a Steam shortcut for an entry it never actually forgot).
pub(super) async fn post_forget_game(
    AxState(state): AxState<PluginState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, StatusCode> {
    if !state.library_available {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    match state.library.remove(&id).await {
        Ok(true) => Ok(Json(json!({ "ok": true }))),
        Ok(false) => Ok(Json(
            json!({ "ok": false, "reason": "game not in library" }),
        )),
        Err(e) => {
            tracing::warn!(game_id = %id, error = %e, "plugin: forget_game failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

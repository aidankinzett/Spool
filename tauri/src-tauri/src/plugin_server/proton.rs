use super::PluginState;
use axum::{
    extract::{Path as AxPath, State as AxState},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub(super) struct InstallDepsRequest {
    verbs: String,
}

/// Install winetricks verbs (e.g. `vcrun2022 dotnet48`) into a game's Proton
/// prefix. Loads the library + config fresh, looks the game up by id, and runs
/// the same state-free core the desktop `install_proton_deps` command uses.
/// Long-running (downloads + installs into the prefix) — the Decky UI shows a
/// blocking spinner and uses a generous client timeout. Returns
/// `{ ok: true, message }` on success or `{ ok: false, reason }` on failure.
pub(super) async fn post_install_deps(
    AxState(state): AxState<PluginState>,
    AxPath(id): AxPath<String>,
    Json(body): Json<InstallDepsRequest>,
) -> Json<Value> {
    if !state.library_available {
        return Json(json!({ "ok": false, "reason": "library unavailable" }));
    }
    let Some(entry) = state.library.find(&id).await.ok().flatten() else {
        return Json(json!({ "ok": false, "reason": "game not in library" }));
    };
    let prefix_override = entry.wine_prefix_path.clone();
    let proton_override = entry.proton_version_path.clone();

    let config = crate::config::Config::load().unwrap_or_default();
    let umu_run_path = config.data.launch.umu_run_path.clone();
    let default_proton_path = config.data.launch.default_proton_path.clone();

    match crate::proton::install_proton_deps_core(
        &id,
        &body.verbs,
        prefix_override.as_deref(),
        proton_override.as_deref(),
        &umu_run_path,
        &default_proton_path,
    )
    .await
    {
        Ok(message) => {
            tracing::info!(game_id = %id, verbs = %body.verbs, "plugin: install-deps complete");
            Json(json!({ "ok": true, "message": message }))
        }
        Err(e) => {
            tracing::warn!(game_id = %id, verbs = %body.verbs, error = %e, "plugin: install-deps failed");
            Json(json!({ "ok": false, "reason": e.to_string() }))
        }
    }
}

/// List the Proton builds discovered on this machine, newest-first. Mirrors
/// the desktop `list_proton_versions` command so the Decky picker shows the
/// same options. Empty on a host with no Proton installed.
pub(super) async fn get_proton_versions() -> Json<Value> {
    // The scan `canonicalize`s every candidate dir; keep that syscall storm off
    // the axum worker thread.
    let versions = tokio::task::spawn_blocking(crate::proton::installed_proton_versions)
        .await
        .unwrap_or_default();
    Json(serde_json::to_value(versions).unwrap_or(json!([])))
}

#[derive(Deserialize)]
pub(super) struct SetProtonRequest {
    /// Absolute path to the Proton dir to pin, or empty/null for "auto" (let
    /// umu-run pick its own default — clears the per-game override).
    proton_version_path: Option<String>,
}

/// Pin (or clear) a game's Proton version. Loads the library fresh, sets the
/// entry's `proton_version_path`, and saves atomically — matching what the
/// desktop edit page's `update_game` does for this one field. An empty or
/// absent path clears the override. Returns `{ ok: true }` on success.
pub(super) async fn post_set_proton(
    AxState(state): AxState<PluginState>,
    AxPath(id): AxPath<String>,
    Json(body): Json<SetProtonRequest>,
) -> Json<Value> {
    if !state.library_available {
        return Json(json!({ "ok": false, "reason": "library unavailable" }));
    }
    let trimmed = body
        .proton_version_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let value = trimmed
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);

    match state
        .library
        .update_fields(&id, &[("proton_version_path", value)])
        .await
    {
        Ok(true) => {
            tracing::info!(game_id = %id, proton = ?trimmed, "plugin: set proton version");
            Json(json!({ "ok": true }))
        }
        Ok(false) => Json(json!({ "ok": false, "reason": "game not in library" })),
        Err(e) => {
            tracing::warn!(game_id = %id, error = %e, "plugin: set proton version failed");
            Json(json!({ "ok": false, "reason": e.to_string() }))
        }
    }
}

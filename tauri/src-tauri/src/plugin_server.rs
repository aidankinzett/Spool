//! Unix-socket HTTP server for the companion Decky plugin.
//!
//! `spool --headless-server` starts this server so the Decky plugin can
//! query library/session state and trigger backup operations over a local
//! Unix socket rather than spawning `spool --backup` / `spool --release-lock`
//! subprocesses for each operation.
//!
//! The socket lives at `~/.local/share/Spool/plugin.sock`. It is created at
//! server startup (removing any stale socket from a prior crash) and removed
//! on clean shutdown. An absent socket file means the server is not running.
//!
//! Linux/Unix only — `#[cfg(unix)]` gates the whole module.

#![cfg(unix)]

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::library::{Library, SharedLibrary};
use crate::ludusavi::LudusaviClient;
use axum::{
    extract::State as AxState,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::net::UnixListener;

// ── Server state ─────────────────────────────────────────────────────────────

/// State shared across all request handlers.
#[derive(Clone)]
struct PluginState {
    config: Arc<Config>,
    library: Arc<SharedLibrary>,
    ludusavi: Arc<LudusaviClient>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Start the plugin Unix socket server and run until killed. Loads config and
/// library from disk, then serves requests indefinitely. Called from
/// `lib.rs::run_headless_server`.
pub async fn serve() -> AppResult<()> {
    let config = Config::load().unwrap_or_else(|e| {
        tracing::warn!(error = %e, "plugin server: failed to load config, using defaults");
        Config::default()
    });
    let library = Library::load().unwrap_or_else(|e| {
        tracing::warn!(error = %e, "plugin server: failed to load library, starting empty");
        Library::default()
    });

    let state = PluginState {
        config: Arc::new(config),
        library: Arc::new(Mutex::new(library)),
        ludusavi: Arc::new(LudusaviClient::new()),
    };

    let socket_path = crate::paths::plugin_socket_path();
    // Remove a stale socket left by a prior crash.
    let _ = std::fs::remove_file(&socket_path);
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&socket_path)
        .map_err(|e| AppError::Other(format!("plugin socket bind: {e}")))?;

    tracing::info!(path = %socket_path.display(), "plugin socket server listening");

    let router = Router::new()
        .route("/status", get(get_status))
        .route("/session", get(get_session))
        .route("/session/game-stopped", post(post_game_stopped))
        .route("/session/backup-now", post(post_backup_now))
        .route("/library", get(get_library))
        .with_state(state);

    axum::serve(listener, router)
        .await
        .map_err(|e| AppError::Other(format!("plugin socket serve: {e}")))?;

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn get_status() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn get_session() -> Json<Value> {
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
struct GameStoppedRequest {
    appid: u32,
}

/// Called by the plugin on every game-stop event. Checks whether the session
/// record matches `appid` and hasn't been backed up yet; if so, releases the
/// play lock then runs a backup — the same forced-close fallback logic that
/// `backup_logic.py::should_backup` + `main.py::on_app_stop` used to perform
/// via subprocesses.
async fn post_game_stopped(
    AxState(state): AxState<PluginState>,
    Json(body): Json<GameStoppedRequest>,
) -> Json<Value> {
    let Some(rec) = crate::session::read() else {
        return Json(json!({ "acted": false }));
    };

    // Spool's non-Steam shortcut appids set the high bit (crc32 | 0x80000000).
    // Steam surfaces those as a signed int32 in some code paths, so the same id
    // can arrive negative. Mask both to unsigned 32-bit before comparing.
    if rec.backed_up || (rec.steam_appid & 0xFFFF_FFFF) != (body.appid & 0xFFFF_FFFF) {
        tracing::info!(
            appid = body.appid,
            session_appid = rec.steam_appid,
            backed_up = rec.backed_up,
            "plugin: game-stopped no-op",
        );
        return Json(json!({ "acted": false }));
    }

    tracing::info!(game = %rec.game, "plugin: forced-close fallback triggered");

    // Release the sync-server play lock first — independent of backup result
    // so the game stops showing as "playing on <device>" immediately.
    crate::sync::release_lock_headless(&state.config.data, &rec.game).await;

    run_backup(&state, &rec.game).await
}

/// Manual backup from the QAM "Back up now" button. No appid check; no lock
/// release — just backs up whatever game the current session record points to.
async fn post_backup_now(AxState(state): AxState<PluginState>) -> Json<Value> {
    let Some(rec) = crate::session::read() else {
        return Json(json!({ "acted": false, "ok": false, "reason": "no active session" }));
    };
    run_backup(&state, &rec.game).await
}

async fn get_library(AxState(state): AxState<PluginState>) -> Json<Value> {
    let lib = match state.library.lock() {
        Ok(g) => g,
        Err(_) => return Json(json!([])),
    };
    Json(serde_json::to_value(&lib.entries).unwrap_or(json!([])))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn run_backup(state: &PluginState, game_name: &str) -> Json<Value> {
    let Some(ludusavi_exe) =
        crate::paths::resolve_ludusavi_path(&state.config.data.ludusavi_path)
    else {
        tracing::error!("plugin backup: ludusavi not configured");
        return Json(
            json!({ "acted": true, "ok": false, "reason": "ludusavi not configured" }),
        );
    };

    if let Err(e) = crate::ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "plugin backup: ensure_config warning");
    }

    let config_dir = crate::paths::ludusavi_config_dir();

    let game_id = {
        let lib = match state.library.lock() {
            Ok(g) => g,
            Err(_) => {
                return Json(
                    json!({ "acted": true, "ok": false, "reason": "library lock poisoned" }),
                )
            }
        };
        lib.entries
            .iter()
            .find(|e| e.game_name == game_name)
            .map(|e| e.id.clone())
    };

    let Some(game_id) = game_id else {
        tracing::error!(name = %game_name, "plugin backup: game not in library");
        return Json(json!({ "acted": true, "ok": false, "reason": "game not in library" }));
    };

    match crate::runner::backup_game_core(
        state.ludusavi.as_ref(),
        &ludusavi_exe,
        &config_dir,
        state.library.as_ref(),
        &game_id,
    )
    .await
    {
        Ok(_) => {
            crate::session::mark_backed_up();
            tracing::info!(game = %game_name, "plugin backup: complete");
            Json(json!({ "acted": true, "ok": true, "game": game_name }))
        }
        Err(e) => {
            tracing::error!(error = %e, game = %game_name, "plugin backup: failed");
            Json(
                json!({ "acted": true, "ok": false, "game": game_name, "reason": e.to_string() }),
            )
        }
    }
}

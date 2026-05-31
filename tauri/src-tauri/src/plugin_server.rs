//! Loopback HTTP server for the companion Decky plugin.
//!
//! `spool --headless-server` starts this server so the Decky plugin can
//! query library/session state, serve cover art, and trigger backup
//! operations over local HTTP rather than spawning `spool --backup` /
//! `spool --release-lock` subprocesses for each operation.
//!
//! It binds a loopback TCP port (preferring 47650, falling back to an
//! ephemeral port) and writes the resolved port to
//! `~/.local/share/Spool/plugin-http-port`. Both the plugin's Python backend
//! and its React UI read that file to build the `http://127.0.0.1:<port>`
//! base URL — the UI fetches `/library` and `<img>`-loads `/covers/*`
//! directly. An absent port file means the server is not running.
//!
//! Linux/Unix only — `#[cfg(unix)]` gates the whole module.

#![cfg(unix)]

use crate::error::{AppError, AppResult};
use crate::library::Library;
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
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, services::ServeDir};

// ── Server state ─────────────────────────────────────────────────────────────

/// State shared across all request handlers.
///
/// Config and library are intentionally **not** cached here — they are
/// reloaded from disk on every request so the server always sees changes made
/// by the main Spool GUI (new games, updated paths, etc.) without a restart.
#[derive(Clone)]
struct PluginState {
    ludusavi: Arc<LudusaviClient>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Start the plugin Unix socket server and run until killed.
/// Called from `lib.rs::run_headless_server`.
pub async fn serve() -> AppResult<()> {
    let state = PluginState {
        ludusavi: Arc::new(LudusaviClient::new()),
    };

    let router = Router::new()
        .route("/status", get(get_status))
        .route("/session", get(get_session))
        .route("/session/game-stopped", post(post_game_stopped))
        .route("/session/backup-now", post(post_backup_now))
        .route("/library", get(get_library))
        // Static cover art straight off disk — no per-cover handler. The UI
        // `<img>`-loads `/covers/<safe_name>.<ext>`.
        .nest_service("/covers", ServeDir::new(crate::paths::covers_dir()))
        // The Decky UI runs under https://steamloopback.host, so its `/library`
        // fetch is cross-origin and needs CORS. `<img>` covers are not
        // CORS-gated and load without this.
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Bind a loopback TCP port. Prefer a stable one so the plugin can find us;
    // fall back to ephemeral if it's already taken (e.g. a stale instance).
    const PREFERRED_PORT: u16 = 47650;
    let listener = match TcpListener::bind(("127.0.0.1", PREFERRED_PORT)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(
                port = PREFERRED_PORT,
                error = %e,
                "preferred plugin HTTP port unavailable; falling back to ephemeral"
            );
            TcpListener::bind(("127.0.0.1", 0))
                .await
                .map_err(|e| AppError::Other(format!("plugin http bind: {e}")))?
        }
    };

    let port = listener
        .local_addr()
        .map_err(|e| AppError::Other(format!("plugin http local_addr: {e}")))?
        .port();

    // Publish the resolved port so the Decky plugin (Python backend + React
    // UI) can reach us. An absent file means the server is not running.
    let port_path = crate::paths::plugin_http_port_path();
    if let Some(parent) = port_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&port_path, port.to_string())?;

    tracing::info!(port, path = %port_path.display(), "plugin HTTP server listening");

    axum::serve(listener, router.into_make_service())
        .await
        .map_err(|e| AppError::Other(format!("plugin http serve: {e}")))?;

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

    // Release the sync-server play lock first — independent of backup result
    // so the game stops showing as "playing on <device>" immediately.
    crate::sync::release_lock_headless(&config.data, &rec.game).await;

    run_backup(&state, &rec.game, &rec.session_id).await
}

/// Manual backup from the QAM "Back up now" button. No appid check; no lock
/// release — just backs up whatever game the current session record points to.
async fn post_backup_now(AxState(state): AxState<PluginState>) -> Json<Value> {
    let Some(rec) = crate::session::read() else {
        return Json(json!({ "acted": false, "ok": false, "reason": "no active session" }));
    };
    run_backup(&state, &rec.game, &rec.session_id).await
}

async fn get_library() -> Json<Value> {
    let library = Library::load().unwrap_or_default();
    Json(serde_json::to_value(&library.entries).unwrap_or(json!([])))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn run_backup(state: &PluginState, game_name: &str, session_id: &str) -> Json<Value> {
    let Some(ludusavi_exe) = crate::paths::resolve_ludusavi_path() else {
        tracing::error!("plugin backup: ludusavi sidecar not found");
        return Json(
            json!({ "acted": true, "ok": false, "reason": "ludusavi sidecar not found" }),
        );
    };

    if let Err(e) = crate::ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "plugin backup: ensure_config warning");
    }

    let config_dir = crate::paths::ludusavi_config_dir();

    let library = Library::load().unwrap_or_default();
    let library = Mutex::new(library);

    let game_id = library
        .lock()
        .ok()
        .and_then(|lib| {
            lib.entries
                .iter()
                .find(|e| e.game_name == game_name)
                .map(|e| e.id.clone())
        });

    let Some(game_id) = game_id else {
        tracing::error!(name = %game_name, "plugin backup: game not in library");
        return Json(json!({ "acted": true, "ok": false, "reason": "game not in library" }));
    };

    match crate::runner::backup_game_core(
        state.ludusavi.as_ref(),
        &ludusavi_exe,
        &config_dir,
        &library,
        &game_id,
    )
    .await
    {
        Ok(_) => {
            // Only mark backed-up when the session that triggered this
            // backup is still the active one — guards against a new game
            // starting while the async backup was in-flight.
            crate::session::mark_backed_up_if(session_id);
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

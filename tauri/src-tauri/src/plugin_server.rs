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

use base64::Engine as _;
use crate::error::{AppError, AppResult};
use crate::lan::LanState;
use crate::library::Library;
use crate::ludusavi::LudusaviClient;
use axum::{
    body::Body,
    extract::{Path as AxPath, State as AxState},
    http::{header, StatusCode},
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
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
    /// Discovered LAN peers, kept fresh by a background listener spawned in
    /// `serve`. The Decky UI reads it via `GET /lan/peers`.
    lan: Arc<LanState>,
    /// Shared HTTP client for proxying requests to peer file servers.
    http: reqwest::Client,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Start the plugin loopback HTTP server and run until killed.
/// Called from `lib.rs::run_headless_server`.
pub async fn serve() -> AppResult<()> {
    let state = PluginState {
        ludusavi: Arc::new(LudusaviClient::new()),
        lan: Arc::new(LanState::new()),
        http: reqwest::Client::new(),
    };

    // Spawn the LAN discovery listener so `/lan/peers` has data. The Deck is a
    // pure consumer here — no announce, no file server. Read our own device id
    // from config so we self-filter the local machine's announces when the GUI
    // also runs on the same box. Non-fatal: a bind failure just means no peers.
    {
        let device_id = crate::config::Config::load()
            .map(|c| c.data.device_id)
            .unwrap_or_default();
        if let Err(e) = crate::lan::discovery::spawn_peer_listener(state.lan.clone(), device_id) {
            tracing::warn!(error = %e, "LAN peer listener failed to start; /lan/peers will be empty");
        }
    }

    let router = Router::new()
        .route("/status", get(get_status))
        .route("/session", get(get_session))
        .route("/session/game-stopped", post(post_game_stopped))
        .route("/session/backup-now", post(post_backup_now))
        .route("/library", get(get_library))
        // Steam-shortcut launch info: the UI uses this to create a non-Steam
        // shortcut live (via SteamClient.Apps) and launch it, reusing the
        // exact exe/launch-options the desktop "Add to Steam" would write.
        .route("/games/:id/steam-launch-info", get(get_steam_launch_info))
        // SteamGridDB art for a library game, transcoded to PNG/JPEG for the
        // live `SteamClient.Apps.SetCustomArtworkForApp` call.
        .route("/games/:id/steam-art/:kind", get(get_steam_art))
        // LAN browsing: list discovered peers, and proxy a peer's game list /
        // covers server-side (the UI can't reach a peer's non-loopback http
        // directly — mixed content). See `lan/server.rs` for the peer API.
        .route("/lan/peers", get(get_lan_peers))
        .route("/lan/peers/:addr/:port/games", get(get_lan_peer_games))
        .route(
            "/lan/peers/:addr/:port/games/:id/cover",
            get(get_lan_peer_cover),
        )
        // Static cover art straight off disk — no per-cover handler. The UI
        // `<img>`-loads `/covers/<safe_name>.<ext>`.
        .nest_service("/covers", ServeDir::new(crate::paths::covers_dir()))
        // The Decky UI runs under https://steamloopback.host, so its JSON
        // fetches are cross-origin and need CORS. `<img>` covers are not
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

    // Flag this device's session as unsynced first — independent of the backup
    // result, so peers immediately see that this device has saves not yet in
    // the cloud. The backup below clears the marker once the upload lands.
    crate::rclone::mark_session_pending_backup_from_config(&config.data, &rec.game).await;

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

/// Fields the UI needs to create a non-Steam shortcut (live, via
/// `SteamClient.Apps.AddShortcut`) and launch it. Mirrors what the desktop
/// `steam::add_to_steam` writes: the shortcut's exe is the stable Spool
/// binary (`spool_executable`, the `$APPIMAGE` path so it survives restarts)
/// and its launch options are `--run "<name>" "<game exe>"`, which the
/// Game-Mode attached `--run` flow consumes. The UI owns the actual shortcut
/// creation so it can use the live API (no Steam restart) and the appid Steam
/// returns.
async fn get_steam_launch_info(
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, StatusCode> {
    let library = Library::load().unwrap_or_default();
    let entry = library.find(&id).ok_or(StatusCode::NOT_FOUND)?;

    let spool_exe = crate::paths::spool_executable().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let start_dir = spool_exe
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    Ok(Json(json!({
        "appName": entry.game_name,
        "exe": spool_exe.to_string_lossy(),
        "startDir": start_dir,
        "launchOptions": crate::steam::build_launch_options(&entry.game_name, &entry.exe_path),
    })))
}

/// Returns `{ imageType: "png"|"jpeg", base64: "<data>" }` for the requested
/// art kind (`capsule`, `hero`, `logo`, `header`). Portraits are served from
/// the on-disk cover cache; other kinds are fetched live from SteamGridDB.
/// WebP images are transcoded to PNG because `SetCustomArtworkForApp` rejects
/// them. Returns 404 if there is no art or SteamGridDB is not configured.
async fn get_steam_art(
    AxState(state): AxState<PluginState>,
    AxPath((id, kind)): AxPath<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    let library = Library::load().unwrap_or_default();
    let entry = library.find(&id).ok_or(StatusCode::NOT_FOUND)?;

    // Portrait / capsule: reuse the on-disk cover already downloaded during
    // "Add game" — no extra network call needed.
    if kind == "capsule" {
        if let Some(path_str) = &entry.cover_image_path {
            let path = std::path::Path::new(path_str);
            if let Ok(bytes) = std::fs::read(path) {
                let mime = mime_from_path(path);
                let (image_type, bytes) = transcode_webp_to_png(mime, bytes);
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                return Ok(Json(json!({ "imageType": image_type, "base64": b64 })));
            }
        }
        return Err(StatusCode::NOT_FOUND);
    }

    // Other kinds (hero, logo, header): fetch from SteamGridDB.
    let config = crate::config::Config::load().unwrap_or_default();
    if !config.data.steamgriddb_enabled || config.data.steamgriddb_api_key.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    let steam_id = entry.steam_id;
    let art = crate::steamgriddb::fetch_art_bytes(
        &state.http,
        &config.data.steamgriddb_api_key,
        steam_id,
        &entry.game_name,
        &kind,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (image_type, bytes) = transcode_webp_to_png(&art.mime, art.bytes);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    Ok(Json(json!({ "imageType": image_type, "base64": b64 })))
}

/// Infers a MIME type string from a file extension (used for cached covers).
fn mime_from_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

/// Transcodes WebP bytes to PNG and returns `("png", transcoded_bytes)`.
/// For non-WebP MIME types, returns the normalised Steam imageType string
/// (`"jpeg"` or `"png"`) with the bytes unchanged.
fn transcode_webp_to_png(mime: &str, bytes: Vec<u8>) -> (&'static str, Vec<u8>) {
    if mime.contains("webp") {
        match image::load_from_memory(&bytes) {
            Ok(img) => {
                let mut out = std::io::Cursor::new(Vec::new());
                if img.write_to(&mut out, image::ImageFormat::Png).is_ok() {
                    return ("png", out.into_inner());
                }
            }
            Err(e) => tracing::warn!(error = %e, "steam-art: webp→png transcode failed; sending as-is"),
        }
    }
    let image_type = if mime.contains("jpeg") || mime.contains("jpg") {
        "jpeg"
    } else {
        "png"
    };
    (image_type, bytes)
}

// ── LAN browsing ───────────────────────────────────────────────────────────

const PEER_PROXY_TIMEOUT: Duration = Duration::from_secs(5);

/// Currently-discovered LAN peers (snapshot of the background listener).
async fn get_lan_peers(AxState(state): AxState<PluginState>) -> Json<Value> {
    Json(serde_json::to_value(state.lan.snapshot()).unwrap_or(json!([])))
}

/// Proxy a peer's `GET /games` (server-side so the UI dodges mixed content).
async fn get_lan_peer_games(
    AxState(state): AxState<PluginState>,
    AxPath((addr, port)): AxPath<(String, u16)>,
) -> Result<Json<Value>, StatusCode> {
    if port == 0 {
        return Err(StatusCode::BAD_REQUEST); // discovery-only peer, no file server
    }
    let url = format!("http://{addr}:{port}/games");
    let resp = state
        .http
        .get(&url)
        .timeout(PEER_PROXY_TIMEOUT)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !resp.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let games: Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(games))
}

/// Proxy a peer's cover image so the LAN grid can `<img>`-load it by URL.
async fn get_lan_peer_cover(
    AxState(state): AxState<PluginState>,
    AxPath((addr, port, id)): AxPath<(String, u16, String)>,
) -> Result<Response, StatusCode> {
    if port == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    let url = format!("http://{addr}:{port}/games/{id}/cover");
    let resp = state
        .http
        .get(&url)
        .timeout(PEER_PROXY_TIMEOUT)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !resp.status().is_success() {
        return Err(StatusCode::NOT_FOUND);
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let mut response = Response::new(Body::from(bytes));
    if let Ok(value) = content_type.parse() {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    Ok(response)
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

    let config = crate::config::Config::load().unwrap_or_default();
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
            // Saves are now in the cloud: clear the unsynced-session marker and
            // record this device as the latest backer for the badge. Best-effort.
            crate::rclone::complete_session_backup_from_config(&config.data, game_name).await;
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

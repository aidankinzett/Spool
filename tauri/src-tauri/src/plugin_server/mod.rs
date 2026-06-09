//! Loopback HTTP server for the companion Decky plugin.
//!
//! `spool --headless-server` starts this server so the Decky plugin can
//! query library/session state, serve cover art, and trigger backup
//! operations over local HTTP. It's the only channel the plugin uses; it
//! replaced the per-operation `spool --backup` / `--release-lock` subprocess
//! spawns an earlier plugin version relied on.
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

mod lan;
mod library;
mod proton;
mod saves;
mod session;
mod steam;

use crate::error::{AppError, AppResult};
use crate::lan::{install::LanDownloadState, LanState};
use crate::library::Library;
use crate::ludusavi::LudusaviClient;
use axum::{
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, services::ServeDir};

// ── Server state ─────────────────────────────────────────────────────────────

/// State shared across all request handlers.
///
/// The `library` is cached here as a shared SQLite connection pool
/// (`SharedLibrary`), opened once when the server starts — not reloaded per
/// request. WAL mode means the running GUI's writes are still visible to every
/// request without a restart, so caching the pool costs no freshness. Config,
/// by contrast, is a small JSON file reloaded from disk per request where it's
/// needed.
#[derive(Clone)]
struct PluginState {
    ludusavi: Arc<LudusaviClient>,
    /// The game library, backed by the shared SQLite database — opened once
    /// here rather than reloaded per request. Because every write goes to the
    /// same DB (in WAL mode), the running GUI's changes are visible to every
    /// request without a reload, which is what the old per-request
    /// `Library::load()` was emulating.
    library: crate::library::SharedLibrary,
    /// Discovered LAN peers, kept fresh by a background listener spawned in
    /// `serve`. The Decky UI reads it via `GET /lan/peers`.
    lan: Arc<LanState>,
    /// Shared HTTP client for proxying requests to peer file servers.
    http: reqwest::Client,
    /// Single-slot in-flight LAN install state. The Decky UI polls
    /// `GET /lan/download` instead of receiving Tauri events.
    download: Arc<LanDownloadState>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Start the plugin loopback HTTP server and run until killed.
/// Called from `lib.rs::run_headless_server`.
pub async fn serve() -> AppResult<()> {
    let library = match Library::open().await {
        Ok(l) => Arc::new(l),
        Err(e) => {
            // An empty in-memory library keeps the server responsive rather
            // than refusing to start; library reads just come back empty.
            tracing::error!(error = %e, "plugin server: failed to open library DB; using empty in-memory library");
            Arc::new(
                Library::open_in_memory()
                    .await
                    .expect("in-memory library must open"),
            )
        }
    };
    let state = PluginState {
        ludusavi: Arc::new(LudusaviClient::new()),
        library,
        lan: Arc::new(LanState::new()),
        http: reqwest::Client::new(),
        download: Arc::new(LanDownloadState::default()),
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
        .route("/session", get(session::get_session))
        .route("/session/game-stopped", post(session::post_game_stopped))
        .route("/session/backup-now", post(session::post_backup_now))
        // Pull cloud saves for a game down to this device without launching it,
        // for the Quick Access "Sync now" action. Mirrors the desktop
        // `pull_cloud_saves` command. Pull-only — never uploads.
        .route("/games/{id}/pull", post(saves::post_pull_cloud_saves))
        // Save rollback: list a game's retained backups (newest-first), and
        // roll back to one — restoring it then pinning it as the new tip.
        // Mirrors the desktop `list_save_revisions` / `restore_save_revision`
        // commands so a Deck user can recover an earlier save from Game Mode.
        .route("/games/{id}/revisions", get(saves::get_revisions))
        .route("/games/{id}/restore", post(saves::post_restore))
        .route("/library", get(library::get_library))
        .route("/games/{id}", delete(library::delete_game))
        .route("/games/{id}/uninstall", post(library::post_uninstall_game))
        .route("/games/{id}/forget", post(library::post_forget_game))
        .route("/fold", post(saves::post_fold))
        // Steam-shortcut launch info: the UI uses this to create a non-Steam
        // shortcut live (via SteamClient.Apps) and launch it, reusing the
        // exact exe/launch-options the desktop "Add to Steam" would write.
        .route("/games/{id}/steam-launch-info", get(steam::get_steam_launch_info))
        // SteamGridDB art for a library game, transcoded to PNG/JPEG for the
        // live `SteamClient.Apps.SetCustomArtworkForApp` call.
        .route("/games/{id}/steam-art/{kind}", get(steam::get_steam_art))
        // Install Windows runtime deps (winetricks verbs) into a game's Proton
        // prefix, so a Deck user can do it from Game Mode without dropping to
        // desktop. Mirrors the desktop `install_proton_deps` command.
        .route("/games/{id}/install-deps", post(proton::post_install_deps))
        // Per-game Proton version: list the installed builds for a picker, and
        // pin the one a game launches with. Mirrors the desktop edit page's
        // Proton dropdown so a Deck user can switch versions from Game Mode.
        .route("/proton-versions", get(proton::get_proton_versions))
        .route("/games/{id}/proton", post(proton::post_set_proton))
        // LAN browsing: list discovered peers, and proxy a peer's game list /
        // covers server-side (the UI can't reach a peer's non-loopback http
        // directly — mixed content). See `lan/server.rs` for the peer API.
        .route("/lan/peers", get(lan::get_lan_peers))
        .route("/lan/peers/{addr}/{port}/games", get(lan::get_lan_peer_games))
        .route(
            "/lan/peers/{addr}/{port}/games/{id}/cover",
            get(lan::get_lan_peer_cover),
        )
        // LAN download: start an install, poll progress, and cancel.
        // The Decky UI polls GET /lan/download instead of subscribing to
        // Tauri events (which don't exist in the headless server).
        .route("/lan/install", post(lan::post_lan_install))
        .route("/lan/download", get(lan::get_lan_download))
        .route("/lan/download", delete(lan::delete_lan_download))
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

async fn get_status() -> Json<Value> {
    Json(json!({ "ok": true }))
}

/// Resolves the ludusavi executable and ensures the config file is valid —
/// the three-step preamble shared by [`saves::post_pull_cloud_saves`],
/// [`saves::get_revisions`], [`saves::post_restore`], and
/// [`library::post_uninstall_game`].
/// Returns `Err("ludusavi sidecar not found")` when the executable is absent;
/// an `ensure_config` warning is logged and the call proceeds (a bad config
/// surfaces as a downstream backup or restore error).
fn ludusavi_prep(log_tag: &str) -> Result<(std::path::PathBuf, std::path::PathBuf), &'static str> {
    let Some(ludusavi_exe) = crate::paths::resolve_ludusavi_path() else {
        return Err("ludusavi sidecar not found");
    };
    if let Err(e) = crate::ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "{log_tag}: ensure_config warning");
    }
    Ok((ludusavi_exe, crate::paths::ludusavi_config_dir()))
}

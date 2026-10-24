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

mod achievements;
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
    extract::{Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, services::ServeDir};

/// The only browser origin allowed to call this server: the Decky UI runs in
/// Steam's CEF under this origin. Requests from the plugin's Python backend
/// carry no `Origin` header and are not affected.
const DECKY_UI_ORIGIN: &str = "https://steamloopback.host";

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
    /// Whether the database was successfully opened. If false, we fell back
    /// to an empty in-memory database to keep the server responsive, but write/modify
    /// and backup operations must be rejected.
    library_available: bool,
    /// Discovered LAN peers, kept fresh by a background listener spawned in
    /// `serve`. The Decky UI reads it via `GET /lan/peers`.
    lan: Arc<LanState>,
    /// Shared HTTP client for proxying requests to peer file servers and LAN
    /// installs. No client-wide timeout: an install streams whole games through
    /// it; the proxy requests set their own.
    http: reqwest::Client,
    /// HTTP client for Steam / SteamGridDB API and art requests (achievement
    /// schemas, Steam art), bounded like the desktop's metadata client so a
    /// stalled request can't leave a Decky UI request pending.
    api_http: reqwest::Client,
    /// Single-slot in-flight LAN install state. The Decky UI polls
    /// `GET /lan/download` instead of receiving Tauri events.
    download: Arc<LanDownloadState>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Start the plugin loopback HTTP server and run until killed.
/// Called from `lib.rs::run_headless_server`.
pub async fn serve() -> AppResult<()> {
    let (library, library_available) = match Library::open().await {
        Ok(l) => (Arc::new(l), true),
        Err(e) => {
            // An empty in-memory library keeps the server responsive rather
            // than refusing to start; library reads just come back empty.
            tracing::error!(error = %e, "plugin server: failed to open library DB; using empty in-memory library");
            let in_mem = Library::open_in_memory()
                .await
                .expect("in-memory library must open");
            (Arc::new(in_mem), false)
        }
    };
    let state = PluginState {
        ludusavi: Arc::new(LudusaviClient::new()),
        library,
        library_available,
        lan: Arc::new(LanState::new()),
        http: reqwest::Client::new(),
        api_http: reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default(),
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

    // Brighten stored accents that are too dark to read (see `accent.rs`). In
    // Game Mode this server may be the only Spool process that runs, so it
    // can't rely on the tray GUI's startup backfill to do it.
    if library_available {
        let library = state.library.clone();
        tokio::spawn(async move { crate::accent::normalize_stored(&library).await });
    }

    let router = Router::new()
        .route("/status", get(get_status))
        .route("/session", get(session::get_session))
        .route("/session/game-stopped", post(session::post_game_stopped))
        .route("/session/backup-now", post(session::post_backup_now))
        // Live achievement unlocks since a timestamp, for the in-game toast.
        .route("/achievements/recent", get(achievements::get_recent))
        // A game's achievement list and play sessions, for the game page's
        // Activity tab. Mirror the desktop `get_game_achievements` /
        // `list_play_sessions` commands.
        .route(
            "/games/{id}/achievements",
            get(achievements::get_game_achievements),
        )
        .route("/games/{id}/sessions", get(achievements::get_game_sessions))
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
        .route(
            "/games/{id}/steam-launch-info",
            get(steam::get_steam_launch_info),
        )
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
        .route(
            "/lan/peers/{addr}/{port}/games",
            get(lan::get_lan_peer_games),
        )
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
        // Achievement icons copied from games' `steam_settings`, under the
        // schema cache. `/games/{id}/achievements` rewrites their local paths
        // to this route.
        .nest_service(
            "/achievement-icons",
            ServeDir::new(crate::achievements::schema::cache_dir()),
        )
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
    let router = harden(router, port);

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

/// Applies the browser-facing protections to the whole router.
///
/// This server has no authentication and exposes destructive endpoints
/// (delete/uninstall a game, restore a save revision, set a Proton path), so
/// any web page open in a browser on the same machine must not be able to
/// call it. Two checks, run before the handlers:
///
///   * `Host` must be this loopback address and port. A DNS-rebinding page
///     reaches the server under its own hostname, which fails this.
///   * A request that carries an `Origin` header must come from the Decky UI.
///     Browsers attach `Origin` to cross-origin requests and to every POST, so
///     another site's `fetch` (including a "simple" POST that skips the CORS
///     preflight) is rejected here.
///
/// CORS then allows only the Decky UI origin, so even a request that got
/// through couldn't have its response read by another site. Requests with no
/// `Origin` (the plugin's Python backend, `<img>` loads) pass the second check.
///
/// Browsers also omit `Origin` on simple cross-origin GETs, so another site can
/// still trigger a GET it can't read the result of. Keep GET handlers free of
/// side effects beyond idempotent reads (the achievement scan only imports
/// unlocks already on disk). Any other process on the machine can reach the
/// server too; this guards against web pages, not local processes.
fn harden(router: Router, port: u16) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(HeaderValue::from_static(DECKY_UI_ORIGIN))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE]);
    let hosts = Arc::new([format!("127.0.0.1:{port}"), format!("localhost:{port}")]);
    router
        .layer(cors)
        .layer(middleware::from_fn_with_state(hosts, guard_host_and_origin))
}

async fn guard_host_and_origin(
    State(hosts): State<Arc<[String; 2]>>,
    req: Request,
    next: Next,
) -> Response {
    // Host names are case-insensitive.
    let host_ok = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| hosts.iter().any(|allowed| h.eq_ignore_ascii_case(allowed)));
    if !host_ok {
        return (StatusCode::FORBIDDEN, "unexpected Host").into_response();
    }
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        if origin.as_bytes() != DECKY_UI_ORIGIN.as_bytes() {
            return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
        }
    }
    next.run(req).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    const PORT: u16 = 47650;

    fn app() -> Router {
        let router = Router::new()
            .route("/status", get(get_status))
            .route("/games/{id}", delete(|| async { "deleted" }))
            .route("/fold", post(|| async { "folded" }));
        harden(router, PORT)
    }

    async fn call(method: Method, uri: &str, headers: &[(&str, &str)]) -> Response {
        let mut req = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        app()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    const HOST: (&str, &str) = ("host", "127.0.0.1:47650");

    #[tokio::test]
    async fn plain_loopback_requests_without_an_origin_pass() {
        // The plugin's Python backend and `<img>` loads send no Origin.
        assert_eq!(call(Method::GET, "/status", &[HOST]).await.status(), 200);
        assert_eq!(call(Method::POST, "/fold", &[HOST]).await.status(), 200);
        let localhost = [("host", "LocalHost:47650")];
        assert_eq!(call(Method::GET, "/status", &localhost).await.status(), 200);
    }

    #[tokio::test]
    async fn the_decky_ui_origin_is_allowed_and_may_read_the_response() {
        let res = call(Method::GET, "/status", &[HOST, ("origin", DECKY_UI_ORIGIN)]).await;
        assert_eq!(res.status(), 200);
        assert_eq!(
            res.headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            DECKY_UI_ORIGIN
        );
    }

    #[tokio::test]
    async fn another_sites_origin_is_rejected_for_reads_and_writes() {
        for origin in ["https://evil.example", "http://localhost:3000", "null"] {
            for (method, uri) in [
                (Method::GET, "/status"),
                (Method::POST, "/fold"),
                (Method::DELETE, "/games/abc"),
            ] {
                let res = call(method.clone(), uri, &[HOST, ("origin", origin)]).await;
                assert_eq!(res.status(), 403, "{method} {uri} from {origin}");
            }
        }
    }

    #[tokio::test]
    async fn a_dns_rebinding_host_is_rejected() {
        for host in [
            "evil.example:47650",
            "127.0.0.1:1",
            "127.0.0.1",
            "localhost",
        ] {
            let res = call(Method::GET, "/status", &[("host", host)]).await;
            assert_eq!(res.status(), 403, "Host: {host}");
        }
        assert_eq!(call(Method::GET, "/status", &[]).await.status(), 403);
    }

    #[tokio::test]
    async fn preflight_is_answered_only_for_the_decky_ui() {
        let allowed = call(
            Method::OPTIONS,
            "/fold",
            &[
                HOST,
                ("origin", DECKY_UI_ORIGIN),
                ("access-control-request-method", "POST"),
                ("access-control-request-headers", "content-type"),
            ],
        )
        .await;
        assert!(allowed.status().is_success());
        assert_eq!(
            allowed
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            DECKY_UI_ORIGIN
        );

        let denied = call(
            Method::OPTIONS,
            "/fold",
            &[
                HOST,
                ("origin", "https://evil.example"),
                ("access-control-request-method", "POST"),
            ],
        )
        .await;
        assert_eq!(denied.status(), 403);
    }
}

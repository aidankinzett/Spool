//! The in-process axum HTTP server that exposes our shared library to
//! peers: `/games`, `/manifest`, per-file downloads (with HTTP range
//! resume), cover/hero artwork, and the cancel-check endpoint. Plus the
//! host-side uploads ledger commands.

use super::{LanUploadsState, PeerFile, PeerGame, PeerGameManifest, UploadSnapshot};
use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use axum::{
    body::Body,
    extract::{ConnectInfo, Path as AxPath, Query as AxQuery, State as AxState},
    http::{header, HeaderMap, StatusCode},
    response::{Json, Response},
    routing::get,
    Router,
};
use futures_util::StreamExt as _;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::SeekFrom;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncSeekExt;
use tokio_util::io::ReaderStream;

/// Shutdown coordinator for the LAN HTTP server. Holds two things:
///
/// * `notify` — what axum's `with_graceful_shutdown` awaits. Firing this
///   stops the listener from accepting new connections and lets in-flight
///   responses drain.
/// * `handle` — the tokio `JoinHandle` of the spawned `axum::serve` task.
///   After notifying we `.await` this handle so we know the server is
///   actually done before the process exits (otherwise the runtime gets
///   dropped and the task is cancelled mid-drain).
///
/// Per `domain-web` "graceful shutdown for in-flight drain" — without
/// this an `app.exit(0)` from the tray rips the rug out from under
/// peers downloading from us.
#[derive(Default)]
pub struct LanServerShutdown {
    pub notify: Arc<tokio::sync::Notify>,
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl LanServerShutdown {
    fn install(&self, h: tokio::task::JoinHandle<()>) {
        if let Ok(mut g) = self.handle.lock() {
            *g = Some(h);
        }
    }

    /// Triggers graceful shutdown and awaits the server task. Idempotent
    /// — a second call after shutdown is a no-op. Bounded by an internal
    /// timeout so a wedged client can't keep us from exiting forever.
    pub async fn shutdown(&self) {
        self.notify.notify_waiters();
        let handle = self.handle.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            // 2 s is enough for any reasonable in-flight chunk write to
            // land; longer and we're better off ripping the connection.
            let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
        }
    }
}

/// In-memory hash cache keyed by absolute file path. Invalidated by
/// mtime — if the source file changes the hash is recomputed on the
/// next manifest fetch. Persistence across process restarts is a
/// future polish item; for now we re-hash on first manifest after
/// each launch.
///
/// `RwLock` (not `Mutex`) because reads dominate: every manifest
/// request walks every shared game and probes the cache for each
/// file; writes only happen for genuine cache misses (first time we
/// see a file, or when its mtime changes). Per `domain-web`'s
/// "read-heavy shared state → Arc<RwLock<T>>" rule, concurrent
/// manifest requests get to read in parallel.
type HashCache = Arc<std::sync::RwLock<HashMap<PathBuf, (std::time::SystemTime, String)>>>;

#[derive(Clone)]
struct ServerState {
    app: AppHandle,
    hash_cache: HashCache,
}

/// Binds the HTTP server and starts serving. Returns the actual port it
/// landed on so the announce loop can advertise it. Tries `preferred_port`
/// first; on bind failure (port already in use — common when running two
/// Spool instances on one machine in dev) falls back to an ephemeral port.
pub(super) async fn start_http_server(app: AppHandle, preferred_port: u16) -> AppResult<u16> {
    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", preferred_port)).await {
        Ok(l) => l,
        Err(e) if preferred_port != 0 => {
            tracing::warn!(
                port = preferred_port,
                error = %e,
                "preferred LAN HTTP port unavailable; falling back to ephemeral"
            );
            tokio::net::TcpListener::bind(("0.0.0.0", 0))
                .await
                .map_err(|e| AppError::Other(format!("bind ephemeral: {e}")))?
        }
        Err(e) => return Err(AppError::Other(format!("bind {preferred_port}: {e}"))),
    };
    let port = listener
        .local_addr()
        .map_err(|e| AppError::Other(format!("local_addr: {e}")))?
        .port();

    // Pull the shutdown bits off managed state before `app` moves into
    // ServerState. The Notify lives on managed state so the tray quit
    // menu (and any future "disable LAN sharing" flow) can signal
    // graceful drain.
    let notify = app.state::<LanServerShutdown>().notify.clone();
    let shutdown_app = app.clone();
    let router = build_router(ServerState {
        app,
        hash_cache: Arc::new(std::sync::RwLock::new(HashMap::new())),
    });

    // Server runs until graceful shutdown is signalled (or the listener
    // dies). `into_make_service_with_connect_info` lets the file
    // handler pull the peer's IP via the `ConnectInfo` extractor for
    // the upload ledger.
    let handle = tokio::spawn(async move {
        let svc = router.into_make_service_with_connect_info::<SocketAddr>();
        let server = axum::serve(listener, svc).with_graceful_shutdown(async move {
            notify.notified().await;
        });
        if let Err(e) = server.await {
            tracing::error!(error = %e, "LAN HTTP server exited");
        }
    });
    shutdown_app.state::<LanServerShutdown>().install(handle);

    Ok(port)
}

/// The route table, before state is attached. axum validates each path
/// pattern inside `route()` and panics on a malformed one (e.g. the 0.7
/// `:id`/`*path` syntax left behind after the 0.8 upgrade), which is what
/// silently killed LAN discovery. Returning `Router<ServerState>` — state
/// not yet bound — lets a test exercise this without constructing a
/// `ServerState` (and thus without a real `AppHandle`), so CI catches a
/// route-syntax regression at build time instead of at runtime.
fn routes() -> Router<ServerState> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/games", get(get_games_handler))
        .route("/games/{id}/manifest", get(get_manifest_handler))
        .route("/games/{id}/files/{*path}", get(get_file_handler))
        .route("/games/{id}/cover", get(get_cover_handler))
        .route("/games/{id}/hero", get(get_hero_handler))
        .route("/games/{id}/cancel-check", get(get_cancel_check_handler))
}

/// Builds the LAN file-server router with its routes bound to `state`.
fn build_router(state: ServerState) -> Router {
    routes().with_state(state)
}

/// `GET /games` — returns the local library in `PeerGame` form. Honours
/// the `lan_share_enabled` config flag: if the user has disabled LAN
/// sharing we return an empty list (200, not 403, so peers see "this
/// instance is online but sharing nothing" rather than treating it as
/// broken).
async fn get_games_handler(
    AxState(state): AxState<ServerState>,
) -> Result<Json<Vec<PeerGame>>, StatusCode> {
    let config = state.app.state::<SharedConfig>();
    let library = state.app.state::<SharedLibrary>();

    let enabled = config
        .lock()
        .map(|c| c.data.lan.share_enabled)
        .unwrap_or(false);
    if !enabled {
        return Ok(Json(Vec::new()));
    }

    // Only catalogue games the user has explicitly opted in to sharing.
    // `from_entry` already encodes the `shareable` flag — we filter the
    // wire payload to just those so non-shared games stay private. The
    // user's local library can have hundreds of entries; LAN browsing
    // should only see what was deliberately offered.
    let games: Vec<PeerGame> = library
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .iter()
        .filter(|g| g.lan_shared)
        .map(PeerGame::from_entry)
        .collect();
    Ok(Json(games))
}

/// Query string accepted by `/games/:id/manifest`. The receiver passes
/// its session UUID so we can register the upload session immediately —
/// before any file fetches arrive — letting the host UI show "fetching
/// manifest" with the game's name and total size.
#[derive(Debug, Deserialize, Default)]
struct ManifestQuery {
    #[serde(default)]
    session: String,
}

/// `GET /games/:id/manifest` — builds a transfer manifest by walking
/// the game's install folder. Returns 404 if the id isn't in our
/// library, 403 if LAN sharing is disabled, 410 if the game has no
/// `game_folder_path` configured (or it no longer exists on disk).
async fn get_manifest_handler(
    AxState(state): AxState<ServerState>,
    AxPath(id): AxPath<String>,
    AxQuery(query): AxQuery<ManifestQuery>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<PeerGameManifest>, StatusCode> {
    let config = state.app.state::<SharedConfig>();
    let library = state.app.state::<SharedLibrary>();

    let (enabled, device_id, device_name) = match config.lock() {
        Ok(cfg) => (
            cfg.data.lan.share_enabled,
            cfg.data.device_id.clone(),
            cfg.data.device_name.clone(),
        ),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    if !enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    // Snapshot the entry so we can drop the library lock before doing
    // I/O. Cloning a GameEntry is cheap relative to a recursive walk.
    let entry = library
        .find(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Per-game opt-in. Return 404 (not 403) so the existence of the id
    // doesn't leak across the lan_shared boundary.
    if !entry.lan_shared {
        return Err(StatusCode::NOT_FOUND);
    }

    let folder = match entry.game_folder_path.as_ref() {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => return Err(StatusCode::GONE),
    };
    if !folder.is_dir() {
        return Err(StatusCode::GONE);
    }

    // Hashing happens here — blake3 is fast but reads every byte on
    // disk, so move the whole walk + hash off the async runtime via
    // spawn_blocking. First request for a big game is slow (~1s/GB on
    // modern hardware); subsequent requests hit the in-memory cache.
    let cache = state.hash_cache.clone();
    let walk_folder = folder.clone();
    let files =
        tokio::task::spawn_blocking(move || walk_game_files_with_hashes(&walk_folder, cache))
            .await
            .map_err(|e| {
                tracing::warn!(game_id = %id, error = %e, "manifest walk task join failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .map_err(|e| {
                tracing::warn!(game_id = %id, error = %e, "manifest walk failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let total_bytes: u64 = files.iter().map(|f| f.size).sum();

    // Register the upload session now that we know the total byte count.
    // This fires before any `/files/*path` requests arrive, so the host
    // UI can show the game name + a progress bar from the very first
    // moment the receiver starts pulling.
    if !query.session.is_empty() {
        let uploads = state.app.state::<LanUploadsState>();
        let is_new = uploads.register_manifest(
            &query.session,
            &entry.id,
            &entry.game_name,
            &peer_addr.ip().to_string(),
            total_bytes,
        );
        if is_new {
            let _ = state.app.emit("lan:uploads-changed", &());
        }
    }

    // Compute exe_relative_path if exe lives inside the folder.
    let exe_relative_path = (!entry.exe_path.is_empty())
        .then(|| relative_unix(&PathBuf::from(&entry.exe_path), &folder))
        .flatten();

    Ok(Json(PeerGameManifest {
        game_id: entry.id.clone(),
        game_name: entry.game_name.clone(),
        safe_name: entry.safe_name.clone(),
        total_bytes,
        files,
        exe_relative_path,
        source_device_id: device_id,
        source_device_name: device_name,
        steam_id: entry.steam_id,
        gog_id: entry.gog_id,
        lutris_slug: entry.lutris_slug.clone(),
        manifest_install_dir: entry.manifest_install_dir.clone(),
        save_paths: entry.save_paths.clone(),
        developer: entry.developer.clone(),
        publisher: entry.publisher.clone(),
        genres: entry.genres.clone(),
        release_date: entry.release_date,
    }))
}

/// Query string accepted by `/games/:id/files/*path`. The receiver
/// passes a `session` UUID so we can group its parallel file fetches
/// into a single host-visible upload, plus the human-friendly
/// `game_name` so the UI doesn't have to cross-reference by id.
#[derive(Debug, Deserialize, Default)]
struct FileQuery {
    #[serde(default)]
    session: String,
    #[serde(default)]
    game_name: String,
}

/// `GET /games/:id/files/*path` — streams one file from the game's
/// install dir. The wildcard path is interpreted strictly: only
/// `Component::Normal` segments allowed, anything that could escape
/// the install root (parent dir, absolute, prefix) is rejected.
///
/// Supports HTTP `Range: bytes=N-` requests for resume — the client
/// sends the size it already has on disk, the server seeks past those
/// bytes and streams the rest. We only handle the `bytes=N-` form;
/// multi-range and suffix forms (`bytes=-N`) return 416 Range Not
/// Satisfiable. `Accept-Ranges: bytes` is set on every response so
/// clients know resume is supported even without trying.
async fn get_file_handler(
    AxState(state): AxState<ServerState>,
    AxPath((id, rel_path)): AxPath<(String, String)>,
    AxQuery(query): AxQuery<FileQuery>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let config = state.app.state::<SharedConfig>();
    let library = state.app.state::<SharedLibrary>();

    let enabled = config
        .lock()
        .map(|c| c.data.lan.share_enabled)
        .unwrap_or(false);
    if !enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let folder = {
        let entry = library
            .find(&id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
        // Re-check the opt-in — a user could flip `lan_shared` off
        // mid-transfer and we honour that on the next file request.
        if !entry.lan_shared {
            return Err(StatusCode::NOT_FOUND);
        }
        match entry.game_folder_path.as_ref() {
            Some(p) if !p.is_empty() => PathBuf::from(p),
            _ => return Err(StatusCode::GONE),
        }
    };

    let abs = safe_join(&folder, &rel_path).ok_or(StatusCode::BAD_REQUEST)?;
    // Stat asynchronously — per `domain-web`, web handlers must not
    // block. The metadata call doubles as our existence check.
    let metadata = match tokio::fs::metadata(&abs).await {
        Ok(m) if m.is_file() => m,
        _ => return Err(StatusCode::NOT_FOUND),
    };

    // Host-side cancel check — if the user clicked Cancel on this
    // session in the uploads UI, this request gets 410 Gone so the
    // receiver knows to abort cleanly.
    if !query.session.is_empty() {
        let uploads = state.app.state::<LanUploadsState>();
        if uploads.is_cancelled(&query.session) {
            return Err(StatusCode::GONE);
        }
        // Otherwise, register this fetch against the session ledger so
        // the host can see what's happening.
        let game_name = if query.game_name.is_empty() {
            id.as_str()
        } else {
            query.game_name.as_str()
        };
        let is_new = uploads.touch(&query.session, &id, game_name, &peer_addr.ip().to_string());
        // Emit only on session creation so the UI refreshes when a peer
        // starts pulling; per-file touches are already covered by the
        // 5 s "last_seen" the snapshot exposes.
        if is_new {
            let _ = state.app.emit("lan:uploads-changed", &());
        }
    }

    let total_len = metadata.len();

    // Parse a Range header if present. We accept just `bytes=N-` —
    // suffix ranges (`bytes=-N`) and multi-range stay unsupported (the
    // client never sends them; an outside caller doing so gets 416).
    let range_start = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(parse_range_start);

    let mut file = tokio::fs::File::open(&abs)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(parsed) = range_start {
        // A Range header was sent — must be a form we support and
        // must fall inside the file.
        let start = parsed.ok_or(StatusCode::RANGE_NOT_SATISFIABLE)?;
        if start >= total_len {
            return Err(StatusCode::RANGE_NOT_SATISFIABLE);
        }
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let body_len = total_len - start;
        let end = total_len - 1;
        let raw_stream = ReaderStream::new(file);
        let body = if !query.session.is_empty() {
            // Credit bytes as they are actually yielded to the socket so
            // a mid-stream disconnect or range-request retry never
            // over-counts.
            let app_h = state.app.clone();
            let session_id = query.session.clone();
            let accounting_stream = raw_stream.map(move |result| {
                if let Ok(ref chunk) = result {
                    let uploads = app_h.state::<LanUploadsState>();
                    if uploads.add_bytes_sent(&session_id, chunk.len() as u64) {
                        let _ = app_h.emit("lan:uploads-changed", &());
                    }
                }
                result
            });
            Body::from_stream(accounting_stream)
        } else {
            Body::from_stream(raw_stream)
        };
        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::PARTIAL_CONTENT;
        let h = resp.headers_mut();
        h.insert(
            header::CONTENT_TYPE,
            "application/octet-stream".parse().unwrap(),
        );
        h.insert(
            header::CONTENT_LENGTH,
            body_len.to_string().parse().unwrap(),
        );
        h.insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total_len}").parse().unwrap(),
        );
        h.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
        return Ok(resp);
    }

    let raw_stream = ReaderStream::new(file);
    let body = if !query.session.is_empty() {
        let app_h = state.app.clone();
        let session_id = query.session.clone();
        let accounting_stream = raw_stream.map(move |result| {
            if let Ok(ref chunk) = result {
                let uploads = app_h.state::<LanUploadsState>();
                if uploads.add_bytes_sent(&session_id, chunk.len() as u64) {
                    let _ = app_h.emit("lan:uploads-changed", &());
                }
            }
            result
        });
        Body::from_stream(accounting_stream)
    } else {
        Body::from_stream(raw_stream)
    };
    let mut resp = Response::new(body);
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        "application/octet-stream".parse().unwrap(),
    );
    h.insert(
        header::CONTENT_LENGTH,
        total_len.to_string().parse().unwrap(),
    );
    h.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    Ok(resp)
}

/// `GET /games/:id/cover` — serves the source's cover image so
/// receivers don't have to round-trip through SteamGridDB. Picks the
/// `cover_image_path` recorded on the local entry and ships the raw
/// bytes with a content-type sniffed from the file extension. 404 if
/// the entry doesn't share or has no cover.
async fn get_cover_handler(
    state: AxState<ServerState>,
    id: AxPath<String>,
) -> Result<Response, StatusCode> {
    serve_artwork_path(state, id, ArtworkKind::Cover).await
}

/// `GET /games/:id/hero` — counterpart of `/cover` for the wide hero
/// image. Same rules: respects opt-in, 404s when there's nothing on
/// disk to serve.
async fn get_hero_handler(
    state: AxState<ServerState>,
    id: AxPath<String>,
) -> Result<Response, StatusCode> {
    serve_artwork_path(state, id, ArtworkKind::Hero).await
}

#[derive(Copy, Clone)]
enum ArtworkKind {
    Cover,
    Hero,
}

async fn serve_artwork_path(
    AxState(state): AxState<ServerState>,
    AxPath(id): AxPath<String>,
    kind: ArtworkKind,
) -> Result<Response, StatusCode> {
    let config = state.app.state::<SharedConfig>();
    let library = state.app.state::<SharedLibrary>();

    let enabled = config
        .lock()
        .map(|c| c.data.lan.share_enabled)
        .unwrap_or(false);
    if !enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let path = {
        let entry = library
            .find(&id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
        if !entry.lan_shared {
            return Err(StatusCode::NOT_FOUND);
        }
        let p = match kind {
            ArtworkKind::Cover => entry.cover_image_path.clone(),
            ArtworkKind::Hero => entry.hero_image_path.clone(),
        };
        match p {
            Some(p) if !p.is_empty() => PathBuf::from(p),
            _ => return Err(StatusCode::NOT_FOUND),
        }
    };
    // Existence check via async stat — handlers must not block.
    // Then read async. `tokio::fs::read` itself fails on missing
    // file, but the explicit check distinguishes "not found" (404)
    // from a real I/O error (500).
    match tokio::fs::metadata(&path).await {
        Ok(m) if m.is_file() => {}
        _ => return Err(StatusCode::NOT_FOUND),
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Sniff content-type from the extension so receivers can save with
    // a sensible filename.
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_else(|| "jpg".to_string());
    let mime = match ext.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    };

    let mut resp = Response::new(Body::from(bytes));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, mime.parse().unwrap());
    Ok(resp)
}

/// Query shape for `/games/:id/cancel-check?session=<token>`. The
/// receiver polls this between file fetches so a host-initiated
/// cancel takes effect even when there's no `/files/*path` request
/// in flight.
#[derive(Debug, Deserialize, Default)]
struct CancelCheckQuery {
    #[serde(default)]
    session: String,
}

/// `GET /games/:id/cancel-check?session=<token>` — 200 if the session
/// is still allowed to keep downloading, 410 Gone if the host clicked
/// cancel. Receivers poll this from `start_peer_install`'s heartbeat
/// loop. We return 410 (rather than 200 with a `cancelled` body) so
/// older clients that don't parse the body still treat the response
/// as fatal.
async fn get_cancel_check_handler(
    AxState(state): AxState<ServerState>,
    AxPath(_id): AxPath<String>,
    AxQuery(query): AxQuery<CancelCheckQuery>,
) -> Result<&'static str, StatusCode> {
    if query.session.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let uploads = state.app.state::<LanUploadsState>();
    if uploads.is_cancelled(&query.session) {
        return Err(StatusCode::GONE);
    }
    Ok("active")
}

/// Parses `bytes=N-`. Returns `Some(N)` on match. Returns `None` for
/// any unsupported form (suffix ranges, multi-range, junk) — the
/// caller maps that to 416.
fn parse_range_start(value: &str) -> Option<u64> {
    let rest = value.strip_prefix("bytes=")?;
    // Multi-range comes as `N-,M-` — bail.
    if rest.contains(',') {
        return None;
    }
    let (start, _end) = rest.split_once('-')?;
    if start.is_empty() {
        return None; // suffix-range `bytes=-N` not supported
    }
    start.parse::<u64>().ok()
}

/// Recursive walk that turns a folder into a flat list of `PeerFile`
/// entries with blake3 hashes. Paths in the manifest are forward-slash
/// and relative to `root` so the receiver can reconstruct local paths
/// cleanly across OSes. Symlinks are followed so installs that use
/// junctions (Windows) or symlinks on Linux still ship the real bytes.
///
/// `cache` is keyed by absolute path → (mtime, hash). Files whose
/// mtime matches the cache reuse the cached hash; everything else gets
/// re-hashed and the cache updated. Empty / zero-byte files get an
/// empty hash (blake3 of zero bytes is a constant — but we skip it to
/// keep the wire smaller and the receiver's "empty hash = skip" rule
/// uniform).
///
/// This runs on `spawn_blocking` from the manifest handler — it's
/// synchronous and disk-bound by design.
fn walk_game_files_with_hashes(root: &Path, cache: HashCache) -> std::io::Result<Vec<PeerFile>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(true) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let rel_str = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if rel_str.is_empty() {
            continue;
        }
        let metadata = entry.metadata()?;
        let size = metadata.len();
        let mtime = metadata.modified().ok();

        // Cache lookup keyed on the absolute path. Mtime mismatch
        // invalidates so we always serve a hash that matches what we'd
        // stream right now. Read-side uses a shared lock so concurrent
        // manifest requests don't serialise on the probe.
        let abs = entry.path().to_path_buf();
        let cached = match (mtime, cache.read().ok()) {
            (Some(mt), Some(g)) => g
                .get(&abs)
                .filter(|(cached_mt, _)| *cached_mt == mt)
                .map(|(_, h)| h.clone()),
            _ => None,
        };

        let hash = if size == 0 {
            String::new()
        } else if let Some(h) = cached {
            tracing::debug!(
                path = %rel_str,
                size,
                hash = %h,
                "manifest: hash from cache"
            );
            h
        } else {
            let h = hash_file_blocking(&abs)?;
            tracing::debug!(
                path = %rel_str,
                size,
                hash = %h,
                "manifest: hash freshly computed"
            );
            // Exclusive lock only on the write path. Failure to acquire
            // is non-fatal — the hash still gets used for this request,
            // we just don't cache it for next time.
            if let (Some(mt), Ok(mut g)) = (mtime, cache.write()) {
                g.insert(abs.clone(), (mt, h.clone()));
            }
            h
        };

        let mtime_unix_ms = mtime
            .and_then(|mt| mt.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        out.push(PeerFile {
            path: rel_str,
            size,
            hash,
            mtime_unix_ms,
        });
    }
    Ok(out)
}

/// blake3 hex digest of a file. Reads in 64 KiB chunks; total memory
/// is a single buffer + hasher state regardless of file size.
fn hash_file_blocking(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Joins `rel` onto `root`, refusing anything that could escape (parent
/// dir, absolute path, Windows prefix). Treats both `/` and `\` as
/// separators so callers don't have to pre-normalise.
///
/// Shared with the receiver side ([`super::install`]) so inbound,
/// network-supplied manifest paths are validated the same way on both
/// ends of a transfer.
pub(super) fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel_path = PathBuf::from(rel.replace('\\', "/"));
    for comp in rel_path.components() {
        match comp {
            Component::Normal(_) => {}
            // Anything else risks escape or is meaningless inside a
            // relative path (CurDir is harmless but unexpected here).
            _ => return None,
        }
    }
    Some(root.join(rel_path))
}

/// Returns `exe` relative to `folder` as a forward-slash string, or
/// `None` if `exe` is outside `folder`. Used to record the source's
/// exe_path in a portable form for the receiver.
fn relative_unix(exe: &Path, folder: &Path) -> Option<String> {
    let rel = exe.strip_prefix(folder).ok()?;
    let parts: Vec<&str> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// Snapshot of peers currently downloading from us. Used by the host UI
/// to render the "Uploads" list; also re-fetched on `lan:uploads-changed`.
#[tauri::command]
pub fn list_active_uploads(state: State<'_, LanUploadsState>) -> Vec<UploadSnapshot> {
    state.snapshot()
}

/// Marks an upload session cancelled. The receiver's next
/// `/cancel-check` poll (or its next `/files/*` fetch) will see 410
/// Gone and abort its install. Returns `true` if a session matched.
#[tauri::command]
pub fn cancel_upload(
    state: State<'_, LanUploadsState>,
    app: AppHandle,
    session_id: String,
) -> bool {
    let ok = state.mark_cancelled(&session_id);
    if ok {
        let _ = app.emit("lan:uploads-changed", &());
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building the route table validates every path pattern. axum panics
    /// here on the pre-0.8 `:id`/`*path` capture syntax, so this is the guard
    /// against a dependency bump silently breaking the LAN server's routing
    /// the way the axum 0.7→0.8 upgrade did (no compile error, no runtime
    /// log — the spawned discovery task just died).
    #[test]
    fn routes_build_with_valid_path_syntax() {
        let _: Router<ServerState> = routes();
    }

    #[test]
    fn parse_range_accepts_open_ended() {
        assert_eq!(parse_range_start("bytes=0-"), Some(0));
        assert_eq!(parse_range_start("bytes=500-"), Some(500));
    }

    #[test]
    fn parse_range_ignores_end_bound() {
        // We seek to the start and stream to EOF, so an explicit end is fine.
        assert_eq!(parse_range_start("bytes=500-999"), Some(500));
    }

    #[test]
    fn parse_range_rejects_unsupported_forms() {
        assert_eq!(parse_range_start("bytes=-500"), None, "suffix range");
        assert_eq!(parse_range_start("bytes=0-,100-"), None, "multi range");
        assert_eq!(parse_range_start("bytes=abc-"), None, "non-numeric start");
        assert_eq!(parse_range_start("items=0-"), None, "wrong unit");
        assert_eq!(parse_range_start("0-"), None, "missing prefix");
    }

    #[test]
    fn safe_join_keeps_normal_segments() {
        let root = Path::new("/games/Hades");
        assert_eq!(
            safe_join(root, "saves/profile.sav"),
            Some(PathBuf::from("/games/Hades/saves/profile.sav"))
        );
    }

    #[test]
    fn safe_join_normalizes_backslashes() {
        let root = Path::new("/games/Hades");
        assert_eq!(
            safe_join(root, "dir\\sub\\file.dat"),
            Some(PathBuf::from("/games/Hades/dir/sub/file.dat"))
        );
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let root = Path::new("/games/Hades");
        assert_eq!(safe_join(root, ".."), None, "parent dir");
        assert_eq!(safe_join(root, "a/../../b"), None, "nested parent escape");
        assert_eq!(safe_join(root, "/etc/passwd"), None, "absolute path");
    }

    #[test]
    fn relative_unix_strips_folder_prefix() {
        let folder = Path::new("/games/Hades");
        assert_eq!(
            relative_unix(Path::new("/games/Hades/bin/game.exe"), folder),
            Some("bin/game.exe".to_string())
        );
    }

    #[test]
    fn relative_unix_rejects_outside_and_self() {
        let folder = Path::new("/games/Hades");
        assert_eq!(
            relative_unix(Path::new("/elsewhere/game.exe"), folder),
            None,
            "exe outside the folder"
        );
        assert_eq!(
            relative_unix(folder, folder),
            None,
            "exe equal to the folder yields no segments"
        );
    }
}

//! LAN peer discovery + library exchange.
//!
//! **Phase A** (already shipped): every 5 s we broadcast a small JSON
//! announce packet over UDP multicast (`239.255.83.83:47631`) and
//! collect everyone else's announces into a peer registry that stales
//! out after 30 s.
//!
//! **Phase B** (this module's new half): each Spool instance also
//! runs a tiny HTTP server that exposes its game library. The announce
//! packet's `file_server_port` field carries the live TCP port so other
//! peers can `GET http://<addr>:<port>/games` and browse what we'd
//! share. Bind tries the user's preferred port from config first, then
//! falls back to an ephemeral port if it's taken (so multiple Spool
//! instances on the same box still come up clean).
//!
//! Future phases will add the download flow (Phase C — actual file
//! transfer) and the settings UI + per-game share toggles (Phase D).
//!
//! Multicast group `239.255.83.83` is in the admin-scoped range
//! (`239.255.0.0/16`) which routers won't forward beyond the local
//! network — exactly the scope we want.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::{make_safe_filename, GameEntry, SharedLibrary};
use crate::paths;
use axum::{
    body::Body,
    extract::{ConnectInfo, Path as AxPath, Query as AxQuery, State as AxState},
    http::{header, HeaderMap, StatusCode},
    response::{Json, Response},
    routing::get,
    Router,
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use std::io::SeekFrom;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio_util::io::ReaderStream;

const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 83, 83);
const MULTICAST_PORT: u16 = 47631;
const PROTOCOL_VERSION: u32 = 1;
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(5);
const PEER_STALE_AFTER: Duration = Duration::from_secs(30);
const REAPER_INTERVAL: Duration = Duration::from_secs(5);
/// Cap on how long an outgoing peer-games fetch is allowed to take. Peers
/// on the same LAN should respond in milliseconds; anything past this is
/// almost certainly a dropped peer or a firewall hole.
const PEER_FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// How many files to stream from a peer at once. 4 is a sweet spot:
/// enough to keep gigabit pipes full when games are full of tiny files,
/// few enough that a peer's HTTP server (or a residential router) isn't
/// drowning in concurrent sockets.
const LAN_PARALLEL_FILES: usize = 4;
/// Minimum gap between `lan:download` event emissions. The download
/// loop fires every chunk; without throttling that's hundreds of
/// events per second on a fast transfer.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

/// Wire format for the UDP announce packet. Stays small — fits in a
/// single MTU comfortably and is forwards-compatible (extra fields are
/// ignored by older clients via `#[serde(default)]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct AnnouncePacket {
    /// Magic prefix so we can ignore unrelated UDP traffic on the
    /// multicast port quickly.
    magic: String,
    version: u32,
    device_id: String,
    device_name: String,
    game_count: u32,
    /// HTTP file server port on the announcing peer. `0` = "discovery
    /// only, not accepting transfers" (e.g. server failed to bind).
    file_server_port: u16,
}

impl Default for AnnouncePacket {
    fn default() -> Self {
        Self {
            magic: "spool".to_string(),
            version: PROTOCOL_VERSION,
            device_id: String::new(),
            device_name: String::new(),
            game_count: 0,
            file_server_port: 0,
        }
    }
}

/// Snapshot of one upload session — i.e., a peer currently downloading
/// from us. Surfaced to the host UI so they can see and (optionally)
/// cancel in-flight uploads. Multiple parallel file fetches from one
/// receiver share a single session id (and therefore a single row).
///
/// `last_seen_ago_secs` is the freshness signal — the UI treats anything
/// under ~2 s as "actively transferring", older as "winding down". The
/// reaper drops sessions ~8 s after the last touch so cancelled /
/// finished transfers fall off naturally.
#[derive(Debug, Clone, Serialize)]
pub struct UploadSnapshot {
    pub session_id: String,
    pub game_id: String,
    pub game_name: String,
    pub peer_addr: String,
    pub last_seen_ago_secs: u64,
    /// Set when the host has hit cancel; the next `cancel-check` poll
    /// from the receiver will see this and propagate.
    pub cancelled: bool,
}

struct UploadSession {
    session_id: String,
    game_id: String,
    game_name: String,
    peer_addr: String,
    last_active: Instant,
    cancelled: bool,
}

/// Shared state for the active-uploads ledger. Same lock discipline as
/// `LanState` — never hold the guard across an `.await`.
#[derive(Default)]
pub struct LanUploadsState {
    sessions: Arc<Mutex<HashMap<String, UploadSession>>>,
}

impl LanUploadsState {
    fn snapshot(&self) -> Vec<UploadSnapshot> {
        let now = Instant::now();
        let g = match self.sessions.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        g.values()
            .map(|s| UploadSnapshot {
                session_id: s.session_id.clone(),
                game_id: s.game_id.clone(),
                game_name: s.game_name.clone(),
                peer_addr: s.peer_addr.clone(),
                last_seen_ago_secs: now.saturating_duration_since(s.last_active).as_secs(),
                cancelled: s.cancelled,
            })
            .collect()
    }

    fn touch(
        &self,
        session_id: &str,
        game_id: &str,
        game_name: &str,
        peer_addr: &str,
    ) -> bool {
        let mut g = match self.sessions.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let now = Instant::now();
        let is_new = !g.contains_key(session_id);
        let entry = g
            .entry(session_id.to_string())
            .or_insert_with(|| UploadSession {
                session_id: session_id.to_string(),
                game_id: game_id.to_string(),
                game_name: game_name.to_string(),
                peer_addr: peer_addr.to_string(),
                last_active: now,
                cancelled: false,
            });
        entry.last_active = now;
        is_new
    }

    fn is_cancelled(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .ok()
            .and_then(|g| g.get(session_id).map(|s| s.cancelled))
            .unwrap_or(false)
    }

    /// Marks the named session cancelled. Returns true if found.
    fn mark_cancelled(&self, session_id: &str) -> bool {
        let mut g = match self.sessions.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match g.get_mut(session_id) {
            Some(s) => {
                s.cancelled = true;
                s.last_active = Instant::now();
                true
            }
            None => false,
        }
    }
}

/// One known peer (us excluded) — serialised to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct LanPeer {
    pub device_id: String,
    pub device_name: String,
    /// `"192.168.1.42"` form (port stripped — that's our own announce-port).
    pub addr: String,
    pub game_count: u32,
    pub version: u32,
    pub file_server_port: u16,
    /// Seconds since the peer was last heard from (0 = just now).
    pub last_seen_ago_secs: u64,
}

struct PeerEntry {
    peer: LanPeer,
    last_seen: Instant,
}

/// Shared LAN state. Held in Tauri state; spawned tasks clone the Arc.
pub struct LanState {
    peers: Arc<Mutex<HashMap<String, PeerEntry>>>,
}

impl LanState {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn snapshot(&self) -> Vec<LanPeer> {
        let now = Instant::now();
        let peers = match self.peers.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        peers
            .values()
            .map(|e| {
                let mut p = e.peer.clone();
                p.last_seen_ago_secs = now.saturating_duration_since(e.last_seen).as_secs();
                p
            })
            .collect()
    }
}

impl Default for LanState {
    fn default() -> Self {
        Self::new()
    }
}

// ── HTTP server (Phase B) ───────────────────────────────────────────────────

/// Subset of `GameEntry` we share over the wire. Excludes local filesystem
/// paths (`exe_path`, `game_folder_path`, image paths) — those are meaningless
/// to a peer and would leak local layout. The fields we keep are the ones a
/// browsing UI on the other side needs to display the game and decide whether
/// they want it. Phase C will add a separate endpoint to actually fetch
/// bytes; this is the catalogue, not the payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerGame {
    pub id: String,
    pub catalog_number: u32,
    pub game_name: String,
    pub developer: String,
    pub publisher: String,
    pub genres: Vec<String>,
    pub install_size_mb: f64,
    pub has_cloud_save: bool,
    pub release_date: Option<DateTime<Utc>>,
    pub steam_id: Option<u64>,
    pub gog_id: Option<u64>,
    pub lutris_slug: Option<String>,
    /// True if the source has a usable `game_folder_path` we can stream
    /// from. Non-shareable entries still appear in the list so the user
    /// understands why they can't install them — the receiver disables
    /// the Install button.
    pub shareable: bool,
}

impl PeerGame {
    fn from_entry(g: &GameEntry) -> Self {
        // Shareable iff the user opted the game in AND we have a real
        // folder on disk to stream from. `lan_shared` is set via the
        // Edit dialog's Sharing tab; default is `false` so newly-added
        // games are private until the user explicitly flips them on.
        let has_folder = g
            .game_folder_path
            .as_ref()
            .map(|p| !p.is_empty() && Path::new(p).is_dir())
            .unwrap_or(false);
        let shareable = g.lan_shared && has_folder;
        Self {
            id: g.id.clone(),
            catalog_number: g.catalog_number,
            game_name: g.game_name.clone(),
            developer: g.developer.clone(),
            publisher: g.publisher.clone(),
            genres: g.genres.clone(),
            install_size_mb: g.install_size_mb,
            has_cloud_save: g.has_cloud_save,
            release_date: g.release_date,
            steam_id: g.steam_id,
            gog_id: g.gog_id,
            lutris_slug: g.lutris_slug.clone(),
            shareable,
        }
    }
}

/// File entry in a peer-game manifest. `path` is `/`-separated and
/// relative to the install root — peers reconstruct local paths by
/// joining onto their own install dir.
///
/// `hash` is the blake3 hex digest of the source file. Empty when the
/// source hasn't computed one (older peers, or zero-byte files); the
/// receiver skips verification in that case rather than failing closed.
///
/// `mtime_unix_ms` is the source file's mtime in unix milliseconds. The
/// receiver restamps the destination to match so repeated installs
/// across machines stay consistent and tooling that keys off mtime
/// (build systems, sync utilities) doesn't see spurious "changed"
/// every time. `0` means "no mtime info" (older peers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerFile {
    pub path: String,
    pub size: u64,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub mtime_unix_ms: u64,
}

/// Full transfer manifest for one game. The receiver fetches this
/// before starting the file stream so it knows the total byte count
/// (for progress) and how to register the entry in its local library
/// once the bytes land.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerGameManifest {
    pub game_id: String,
    pub game_name: String,
    pub safe_name: String,
    pub total_bytes: u64,
    pub files: Vec<PeerFile>,
    /// Path inside the install root to the launchable exe — used by the
    /// receiver to populate `exe_path` after install. `None` if the
    /// source's `exe_path` lives outside `game_folder_path` (in which
    /// case the receiver leaves `exe_path` empty and the user must set
    /// it manually).
    pub exe_relative_path: Option<String>,
    pub source_device_id: String,
    pub source_device_name: String,
    // Manifest-derived metadata so the receiver can register the game
    // with the same shape as a locally-added entry.
    pub steam_id: Option<u64>,
    pub gog_id: Option<u64>,
    pub lutris_slug: Option<String>,
    pub has_cloud_save: bool,
    pub manifest_install_dir: Option<String>,
    pub save_paths: Vec<String>,
    pub developer: String,
    pub publisher: String,
    pub genres: Vec<String>,
    pub release_date: Option<DateTime<Utc>>,
}

/// Shutdown coordinator for the LAN HTTP server. Holds two things:
///
///   * `notify`  — what axum's `with_graceful_shutdown` awaits. Firing
///                 this stops the listener from accepting new
///                 connections and lets in-flight responses drain.
///   * `handle`  — the tokio `JoinHandle` of the spawned `axum::serve`
///                 task. After notifying we `.await` this handle so we
///                 know the server is actually done before the process
///                 exits (otherwise the runtime gets dropped and the
///                 task is cancelled mid-drain).
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
async fn start_http_server(app: AppHandle, preferred_port: u16) -> AppResult<u16> {
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

    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/games", get(get_games_handler))
        .route("/games/:id/manifest", get(get_manifest_handler))
        .route("/games/:id/files/*path", get(get_file_handler))
        .route("/games/:id/cover", get(get_cover_handler))
        .route("/games/:id/hero", get(get_hero_handler))
        .route("/games/:id/cancel-check", get(get_cancel_check_handler));

    // Pull the shutdown bits off managed state before `app` moves into
    // ServerState. The Notify lives on managed state so the tray quit
    // menu (and any future "disable LAN sharing" flow) can signal
    // graceful drain.
    let notify = app.state::<LanServerShutdown>().notify.clone();
    let shutdown_app = app.clone();
    let router = router.with_state(ServerState {
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

/// `GET /games` — returns the local library in `PeerGame` form. Honours
/// the `lan_share_enabled` config flag: if the user has disabled LAN
/// sharing we return an empty list (200, not 403, so peers see "this
/// instance is online but sharing nothing" rather than treating it as
/// broken). Phase D will add per-game `lan_shared` filtering.
async fn get_games_handler(
    AxState(state): AxState<ServerState>,
) -> Result<Json<Vec<PeerGame>>, StatusCode> {
    let config = state.app.state::<SharedConfig>();
    let library = state.app.state::<SharedLibrary>();

    let enabled = config
        .lock()
        .map(|c| c.data.lan_share_enabled)
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
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .entries
        .iter()
        .filter(|g| g.lan_shared)
        .map(PeerGame::from_entry)
        .collect();
    Ok(Json(games))
}

/// `GET /games/:id/manifest` — builds a transfer manifest by walking
/// the game's install folder. Returns 404 if the id isn't in our
/// library, 403 if LAN sharing is disabled, 410 if the game has no
/// `game_folder_path` configured (or it no longer exists on disk).
async fn get_manifest_handler(
    AxState(state): AxState<ServerState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<PeerGameManifest>, StatusCode> {
    let config = state.app.state::<SharedConfig>();
    let library = state.app.state::<SharedLibrary>();

    let (enabled, device_id, device_name) = match config.lock() {
        Ok(cfg) => (
            cfg.data.lan_share_enabled,
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
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .find(&id)
        .cloned()
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
    let files = tokio::task::spawn_blocking(move || {
        walk_game_files_with_hashes(&walk_folder, cache)
    })
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
        has_cloud_save: entry.has_cloud_save,
        manifest_install_dir: entry.manifest_install_dir.clone(),
        save_paths: entry.save_paths.clone(),
        developer: entry.developer.clone(),
        publisher: entry.publisher.clone(),
        genres: entry.genres.clone(),
        release_date: entry.release_date,
    }))
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
        .map(|c| c.data.lan_share_enabled)
        .unwrap_or(false);
    if !enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let folder = {
        let lib = library
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let entry = lib.find(&id).ok_or(StatusCode::NOT_FOUND)?;
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
        let is_new = uploads.touch(
            &query.session,
            &id,
            game_name,
            &peer_addr.ip().to_string(),
        );
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
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);
        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::PARTIAL_CONTENT;
        let h = resp.headers_mut();
        h.insert(header::CONTENT_TYPE, "application/octet-stream".parse().unwrap());
        h.insert(header::CONTENT_LENGTH, body_len.to_string().parse().unwrap());
        h.insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total_len}").parse().unwrap(),
        );
        h.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
        return Ok(resp);
    }

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let mut resp = Response::new(body);
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, "application/octet-stream".parse().unwrap());
    h.insert(header::CONTENT_LENGTH, total_len.to_string().parse().unwrap());
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
        .map(|c| c.data.lan_share_enabled)
        .unwrap_or(false);
    if !enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let path = {
        let lib = library
            .lock()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let entry = lib.find(&id).ok_or(StatusCode::NOT_FOUND)?;
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
fn walk_game_files_with_hashes(
    root: &Path,
    cache: HashCache,
) -> std::io::Result<Vec<PeerFile>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(true) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
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
            h
        } else {
            let h = hash_file_blocking(&abs)?;
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
fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
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

// ── Spawning ────────────────────────────────────────────────────────────────

/// Spawns the HTTP server, announce, listen, and reaper background tasks.
/// Called once from `lib.rs::run`'s setup hook. Failures during socket
/// setup are logged but non-fatal — the app continues without LAN
/// discovery / sharing.
pub fn spawn_discovery(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_discovery(app).await {
            tracing::warn!(error = %e, "LAN discovery exited; peers will not be listed");
        }
    });
}

async fn run_discovery(app: AppHandle) -> AppResult<()> {
    // Snapshot device identity + preferred port from config — used by
    // both announces and self-loop filtering.
    let (device_id, device_name, preferred_port, lan_enabled) = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        (
            cfg.data.device_id.clone(),
            cfg.data.device_name.clone(),
            cfg.data.lan_share_port,
            cfg.data.lan_share_enabled,
        )
    };

    if device_id.is_empty() {
        return Err(AppError::Other(
            "device_id not assigned yet — skipping LAN discovery".into(),
        ));
    }

    let socket = make_multicast_socket()?;
    let socket = Arc::new(socket);

    // Start the HTTP server first so we can advertise its real port in
    // every announce. If it fails to bind we still run discovery — peers
    // see file_server_port=0 and know we're browse-only from their POV.
    let server_port = Arc::new(AtomicU16::new(0));
    if lan_enabled {
        match start_http_server(app.clone(), preferred_port).await {
            Ok(port) => {
                server_port.store(port, Ordering::Relaxed);
                tracing::info!(port, "LAN HTTP server listening");
            }
            Err(e) => {
                tracing::warn!(error = %e, "LAN HTTP server failed to start; discovery-only");
            }
        }
    } else {
        tracing::info!("LAN sharing disabled in config; running discovery-only");
    }

    tracing::info!(
        device_id = %device_id,
        device_name = %device_name,
        "LAN discovery started on {MULTICAST_ADDR}:{MULTICAST_PORT}"
    );

    let lan_state = app.state::<LanState>().peers.clone();

    let announce_handle = {
        let socket = socket.clone();
        let app = app.clone();
        let device_id = device_id.clone();
        let device_name = device_name.clone();
        let port = server_port.clone();
        tokio::spawn(async move {
            announce_loop(socket, app, device_id, device_name, port).await;
        })
    };
    let listen_handle = {
        let socket = socket.clone();
        let app = app.clone();
        let device_id = device_id.clone();
        let peers = lan_state.clone();
        tokio::spawn(async move {
            listen_loop(socket, app, device_id, peers).await;
        })
    };
    let reaper_handle = {
        let app = app.clone();
        let peers = lan_state;
        tokio::spawn(async move {
            reaper_loop(app, peers).await;
        })
    };
    let upload_reaper_handle = {
        let app = app.clone();
        let uploads = app.state::<LanUploadsState>().sessions.clone();
        tokio::spawn(async move {
            upload_reaper_loop(app, uploads).await;
        })
    };

    let _ = tokio::try_join!(
        announce_handle,
        listen_handle,
        reaper_handle,
        upload_reaper_handle
    );
    Ok(())
}

/// Configures the UDP socket: bind to `0.0.0.0:MULTICAST_PORT` with
/// SO_REUSEADDR (multiple processes can listen — useful in dev), join
/// the multicast group on all interfaces. Tokio's UdpSocket can't set
/// these pre-bind, so we go through socket2 then convert.
fn make_multicast_socket() -> AppResult<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| AppError::Other(format!("socket create: {e}")))?;
    socket
        .set_reuse_address(true)
        .map_err(|e| AppError::Other(format!("set SO_REUSEADDR: {e}")))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| AppError::Other(format!("set nonblocking: {e}")))?;
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, MULTICAST_PORT)).into())
        .map_err(|e| AppError::Other(format!("bind {MULTICAST_PORT}: {e}")))?;
    socket
        .join_multicast_v4(&MULTICAST_ADDR, &Ipv4Addr::UNSPECIFIED)
        .map_err(|e| AppError::Other(format!("join multicast: {e}")))?;
    // Loopback ON so two Spool instances on the same machine see each
    // other; we filter by device_id rather than dropping our own loop.
    socket
        .set_multicast_loop_v4(true)
        .map_err(|e| AppError::Other(format!("set multicast loopback: {e}")))?;

    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket).map_err(|e| AppError::Other(format!("tokio from_std: {e}")))
}

async fn announce_loop(
    socket: Arc<UdpSocket>,
    app: AppHandle,
    device_id: String,
    device_name: String,
    server_port: Arc<AtomicU16>,
) {
    let target = SocketAddr::from((MULTICAST_ADDR, MULTICAST_PORT));
    loop {
        // Read current game count fresh each tick so peers see growth as
        // the user adds games.
        let game_count = {
            match app.state::<SharedLibrary>().lock() {
                Ok(lib) => lib.entries.len() as u32,
                Err(_) => 0,
            }
        };
        let packet = AnnouncePacket {
            magic: "spool".into(),
            version: PROTOCOL_VERSION,
            device_id: device_id.clone(),
            device_name: device_name.clone(),
            game_count,
            file_server_port: server_port.load(Ordering::Relaxed),
        };
        if let Ok(payload) = serde_json::to_vec(&packet) {
            if let Err(e) = socket.send_to(&payload, target).await {
                tracing::warn!(error = %e, "LAN announce send failed");
            }
        }
        tokio::time::sleep(ANNOUNCE_INTERVAL).await;
    }
}

async fn listen_loop(
    socket: Arc<UdpSocket>,
    app: AppHandle,
    our_device_id: String,
    peers: Arc<Mutex<HashMap<String, PeerEntry>>>,
) {
    let mut buf = vec![0u8; 4096];
    loop {
        let (len, src) = match socket.recv_from(&mut buf).await {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!(error = %e, "LAN listen recv failed");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        let packet: AnnouncePacket = match serde_json::from_slice(&buf[..len]) {
            Ok(p) => p,
            Err(_) => continue, // ignore stray UDP / unrelated multicast traffic
        };
        if packet.magic != "spool" {
            continue;
        }
        if packet.device_id == our_device_id || packet.device_id.is_empty() {
            continue;
        }
        let addr = match src {
            SocketAddr::V4(v4) => v4.ip().to_string(),
            SocketAddr::V6(v6) => v6.ip().to_string(),
        };
        let changed = {
            let mut peers = match peers.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };
            let entry = peers.get(&packet.device_id);
            let is_new = entry.is_none();
            // Notify the frontend if anything observable shifted — new peer,
            // updated game count, or the file server transitioned in/out of
            // "available" (port 0 ↔ nonzero).
            let count_changed = entry
                .map(|e| e.peer.game_count != packet.game_count)
                .unwrap_or(false);
            let port_changed = entry
                .map(|e| {
                    (e.peer.file_server_port == 0) != (packet.file_server_port == 0)
                })
                .unwrap_or(false);
            peers.insert(
                packet.device_id.clone(),
                PeerEntry {
                    peer: LanPeer {
                        device_id: packet.device_id.clone(),
                        device_name: packet.device_name.clone(),
                        addr,
                        game_count: packet.game_count,
                        version: packet.version,
                        file_server_port: packet.file_server_port,
                        last_seen_ago_secs: 0,
                    },
                    last_seen: Instant::now(),
                },
            );
            is_new || count_changed || port_changed
        };
        if changed {
            let _ = app.emit("lan:peers-changed", &());
        }
    }
}

async fn reaper_loop(app: AppHandle, peers: Arc<Mutex<HashMap<String, PeerEntry>>>) {
    loop {
        tokio::time::sleep(REAPER_INTERVAL).await;
        let removed = {
            let mut peers = match peers.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };
            let before = peers.len();
            peers.retain(|_, e| e.last_seen.elapsed() < PEER_STALE_AFTER);
            peers.len() != before
        };
        if removed {
            let _ = app.emit("lan:peers-changed", &());
        }
    }
}

/// Drops upload sessions whose last file-fetch is more than ~8 s old.
/// Each parallel file request from the receiver touches `last_active`,
/// so an in-flight transfer keeps refreshing the entry; a session
/// that's truly done (no more requests coming) falls off after a brief
/// grace window so the host UI doesn't flap.
async fn upload_reaper_loop(app: AppHandle, uploads: Arc<Mutex<HashMap<String, UploadSession>>>) {
    const UPLOAD_STALE: Duration = Duration::from_secs(8);
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let removed = {
            let mut g = match uploads.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };
            let before = g.len();
            g.retain(|_, s| s.last_active.elapsed() < UPLOAD_STALE);
            g.len() != before
        };
        if removed {
            let _ = app.emit("lan:uploads-changed", &());
        }
    }
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_lan_peers(state: State<'_, LanState>) -> Vec<LanPeer> {
    state.snapshot()
}

// ── Download flow (Phase C) ─────────────────────────────────────────────────

/// Snapshot of an in-flight (or just-finished) peer install. Emitted as
/// `lan:download` events and also held in `LanDownloadState` so the UI
/// can pick up mid-transfer on a late mount.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub install_token: String,
    pub source_device_id: String,
    pub source_device_name: String,
    pub source_game_id: String,
    pub game_name: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: String,
    pub status: String, // "starting" | "transferring" | "done" | "error" | "canceled"
    pub message: Option<String>,
    /// Set when status == "done": the id of the freshly-created
    /// library entry so the UI can jump straight to it.
    pub new_game_id: Option<String>,
    /// Average download throughput in bytes per second since the
    /// install started. Set by `LanDownloadState::update` after the
    /// caller's mutation runs. 0 during the first half-second so
    /// the UI doesn't flash a silly "9999 GB/s" off the first chunk.
    pub bytes_per_second: f64,
}

/// Single-slot in-flight install tracker. Same model as `RunState` —
/// one transfer at a time keeps the UX (and bandwidth) predictable,
/// and the next phase can lift this to a HashMap if multi-download
/// becomes a real ask.
///
/// The `cancel_flag` lets the user abort an in-flight install. The
/// download loop polls it between chunks and between files, so cancel
/// is cooperative — the partial dir gets cleaned up on the way out
/// rather than left as orphan junk.
#[derive(Default)]
pub struct LanDownloadState {
    current: Mutex<Option<DownloadProgress>>,
    cancel_flag: AtomicBool,
    /// Set by the heartbeat task when the source returned 410 Gone
    /// from `/cancel-check`. Tells the chunk loop to surface
    /// `AppError::HostCanceled` rather than the generic `Canceled`,
    /// so we can log + display the right reason. Always implies
    /// `cancel_flag` is also set.
    host_cancel_flag: AtomicBool,
    /// Wall-clock anchor for computing `bytes_per_second`. Set in
    /// `try_start`, cleared (implicitly) when a new install replaces
    /// it. Stored separately from `current` because `Instant` isn't
    /// serializable and doesn't belong in the wire-format DTO.
    start_instant: Mutex<Option<Instant>>,
}

impl LanDownloadState {
    fn try_start(&self, p: DownloadProgress) -> AppResult<DownloadGuard<'_>> {
        let mut guard = self.current.lock().map_err(|_| AppError::LockPoisoned)?;
        if guard.is_some() {
            return Err(AppError::Other(
                "Another LAN install is already in progress".into(),
            ));
        }
        // Reset the cancel flags for the fresh install — any lingering
        // `true` from a previous cancelled run would otherwise abort us
        // immediately.
        self.cancel_flag.store(false, Ordering::Relaxed);
        self.host_cancel_flag.store(false, Ordering::Relaxed);
        // Anchor the throughput clock here, not at command-receive
        // time, so the first few hundred ms of manifest-fetch don't
        // skew the average down.
        if let Ok(mut g) = self.start_instant.lock() {
            *g = Some(Instant::now());
        }
        *guard = Some(p);
        Ok(DownloadGuard { state: self })
    }

    /// Marks the current install as cancelled iff `token` matches. The
    /// download loop will notice on its next poll and abort cleanly.
    /// Returns true if a cancel was actually requested (token matched
    /// an in-flight install).
    fn request_cancel(&self, token: &str) -> bool {
        let guard = match self.current.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match guard.as_ref() {
            Some(p) if p.install_token == token => {
                self.cancel_flag.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    fn is_canceled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }

    /// Signals "cancelled by host" — sets both flags so the chunk
    /// loop bails on its next poll and the eventual error variant
    /// reflects who initiated the cancel.
    fn request_host_cancel(&self) {
        self.host_cancel_flag.store(true, Ordering::Relaxed);
        self.cancel_flag.store(true, Ordering::Relaxed);
    }

    /// Returns the right `AppError` variant for the current cancel
    /// state — `HostCanceled` if the heartbeat detected a remote
    /// kick, otherwise `Canceled` for user-initiated.
    fn cancel_error(&self) -> AppError {
        if self.host_cancel_flag.load(Ordering::Relaxed) {
            AppError::HostCanceled
        } else {
            AppError::Canceled
        }
    }

    /// Computes how long the caller should sleep to drag the
    /// aggregate transfer rate back under `max_bps`. Each parallel
    /// file task shares the same `bytes_done` atomic and the same
    /// `start_instant` anchor, so they collectively converge on the
    /// cap.
    ///
    /// Returns `None` when no throttling is needed (rate is under
    /// the cap, no cap configured, or first 100 ms of the install
    /// where the average is noisy). Sleep is capped at 500 ms so
    /// cancellation stays responsive.
    pub fn throttle_required(&self, bytes_done: u64, max_bps: f64) -> Option<Duration> {
        if max_bps <= 0.0 {
            return None;
        }
        let start = self.start_instant.lock().ok()?.as_ref().copied()?;
        let actual_secs = start.elapsed().as_secs_f64();
        if actual_secs < 0.1 {
            return None;
        }
        let bd = bytes_done as f64;
        if bd / actual_secs <= max_bps {
            return None;
        }
        let target_secs = bd / max_bps;
        let sleep_secs = (target_secs - actual_secs).min(0.5);
        if sleep_secs <= 0.0 {
            return None;
        }
        Some(Duration::from_millis((sleep_secs * 1000.0) as u64))
    }

    fn snapshot(&self) -> Option<DownloadProgress> {
        self.current.lock().ok().and_then(|g| g.clone())
    }

    fn update<F: FnOnce(&mut DownloadProgress)>(&self, f: F) -> Option<DownloadProgress> {
        let mut guard = self.current.lock().ok()?;
        if let Some(p) = guard.as_mut() {
            f(p);
            // Refresh derived throughput after the caller's mutation
            // so callers don't have to remember to set it. Suppress
            // the value for the first half-second — a single 64 KB
            // chunk in 5 ms otherwise reads as "13 MB/s" before the
            // average smooths out.
            if let Ok(start_g) = self.start_instant.lock() {
                if let Some(start) = *start_g {
                    let elapsed = start.elapsed().as_secs_f64();
                    if elapsed > 0.5 {
                        p.bytes_per_second = (p.bytes_done as f64) / elapsed;
                    }
                }
            }
            return Some(p.clone());
        }
        None
    }

    /// Overwrite the slot wholesale. Used by the install task to publish
    /// the final "done" / "error" state. Wrapped as a method so callers
    /// don't have to touch the private `current` field across a State
    /// deref (which the borrow checker objects to when the State is a
    /// temporary).
    fn set(&self, value: Option<DownloadProgress>) {
        if let Ok(mut g) = self.current.lock() {
            *g = value;
        }
    }

    /// Clear the slot iff the in-flight install matches `token`. The
    /// guard against clearing the wrong install protects the case where
    /// the user kicked off a second install during the 2 s grace period
    /// after the first one finished.
    fn clear_if_token(&self, token: &str) {
        if let Ok(mut g) = self.current.lock() {
            if let Some(p) = g.as_ref() {
                if p.install_token == token {
                    *g = None;
                }
            }
        }
    }
}

/// RAII guard — clears the slot when the install task ends, even if it
/// panics. Mirrors `runner::RunGuard`. Without this a crashed transfer
/// would jam the slot until restart.
struct DownloadGuard<'a> {
    state: &'a LanDownloadState,
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut g) = self.state.current.lock() {
            *g = None;
        }
    }
}

/// Resolves where new LAN installs land. Defaults to
/// `<app_data>/lan-games` when the user hasn't set `lan_install_dir`
/// in config — matches the convention of every other Spool path.
fn install_root_from(app: &AppHandle) -> AppResult<PathBuf> {
    let config = app.state::<SharedConfig>();
    let configured = {
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        cfg.data.lan_install_dir.clone()
    };
    if configured.is_empty() {
        Ok(paths::app_data_dir().join("lan-games"))
    } else {
        Ok(PathBuf::from(configured))
    }
}

/// Picks an install directory inside the LAN root that doesn't collide
/// with an existing install. Adds `" (2)"`, `" (3)"` etc. as needed.
fn allocate_install_dir(root: &Path, safe_name: &str) -> PathBuf {
    let base = if safe_name.is_empty() {
        "Game".to_string()
    } else {
        make_safe_filename(safe_name)
    };
    let first = root.join(&base);
    if !first.exists() {
        return first;
    }
    for n in 2u32..=999 {
        let candidate = root.join(format!("{base} ({n})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // Pathological collision — append timestamp.
    root.join(format!("{base}-{}", Utc::now().timestamp()))
}

/// Resolves the `(final_dir, partial_dir, resuming)` triple for a new
/// install. If a `<base>.partial` directory already exists from a
/// previous interrupted attempt — and the would-be final dir is still
/// free — we resume into it. Otherwise allocate a fresh non-colliding
/// pair.
///
/// We deliberately only check the *preferred* base name (no scanning
/// for `Name (2).partial`, `Name (3).partial`, …): keeping the rule
/// simple means a user who genuinely wants a fresh install can get
/// one by deleting the leftover `.partial` folder.
fn resolve_install_dirs(root: &Path, safe_name: &str) -> (PathBuf, PathBuf, bool) {
    let base = if safe_name.is_empty() {
        "Game".to_string()
    } else {
        make_safe_filename(safe_name)
    };
    let preferred_final = root.join(&base);
    let preferred_partial = root.join(format!("{base}.partial"));
    if preferred_partial.is_dir() && !preferred_final.exists() {
        return (preferred_final, preferred_partial, true);
    }
    let final_dir = allocate_install_dir(root, safe_name);
    let partial = final_dir.with_extension("partial");
    (final_dir, partial, false)
}

fn emit_progress(app: &AppHandle, progress: &DownloadProgress) {
    if let Err(e) = app.emit("lan:download", progress) {
        tracing::warn!(error = %e, "failed to emit lan:download");
    }
}

/// Fetches the game catalogue from a peer's HTTP server. Frontend calls
/// this when the user opens a peer's row in the LAN popover. Times out
/// quickly so a stale peer in the registry can't hang the UI.
#[tauri::command]
pub async fn fetch_peer_games(
    app: AppHandle,
    addr: String,
    port: u16,
) -> AppResult<Vec<PeerGame>> {
    if port == 0 {
        return Err(AppError::Other(
            "peer is discovery-only (no file server)".into(),
        ));
    }
    let url = format!("http://{addr}:{port}/games");
    let resp = app
        .state::<reqwest::Client>()
        .get(&url)
        .timeout(PEER_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "peer responded {} to /games",
            resp.status()
        )));
    }
    resp.json::<Vec<PeerGame>>()
        .await
        .map_err(|e| AppError::Other(format!("parse peer /games: {e}")))
}

/// Snapshot of the active LAN install (if any). The frontend uses this
/// on mount to catch up after a navigation that lost in-memory state —
/// otherwise it tracks live via the `lan:download` event stream.
#[tauri::command]
pub fn current_peer_download(state: State<'_, LanDownloadState>) -> Option<DownloadProgress> {
    state.snapshot()
}

/// Requests cancellation of an in-flight install. The download task
/// polls the cancel flag between chunks, cleans up its `.partial` dir,
/// then emits a final `lan:download` with `status: "canceled"`. Returns
/// `true` if the token matched an active install, `false` if there was
/// nothing to cancel (no in-flight transfer, or different token).
#[tauri::command]
pub fn cancel_peer_install(
    state: State<'_, LanDownloadState>,
    install_token: String,
) -> bool {
    state.request_cancel(&install_token)
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

/// Kicks off a peer install. Acquires the single-slot guard, fetches
/// the manifest, streams every file to a `.partial` staging dir, then
/// renames into place and registers a new library entry. Progress is
/// emitted continuously as `lan:download` events.
///
/// Returns the install_token (uuid) once the transfer has been queued —
/// the heavy work runs in a spawned task so the command returns
/// immediately and the UI can render an in-flight row right away.
#[tauri::command]
pub async fn start_peer_install(
    app: AppHandle,
    state: State<'_, LanDownloadState>,
    peer_addr: String,
    peer_port: u16,
    game_id: String,
) -> AppResult<String> {
    if peer_port == 0 {
        return Err(AppError::Other(
            "peer is discovery-only (no file server)".into(),
        ));
    }

    // Fetch the manifest synchronously so we can fail fast with a clean
    // error message if the peer 404s or the entry isn't shareable.
    let manifest_url = format!("http://{peer_addr}:{peer_port}/games/{game_id}/manifest");
    let resp = app
        .state::<reqwest::Client>()
        .get(&manifest_url)
        .timeout(PEER_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET manifest: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "peer responded {} to /manifest",
            resp.status()
        )));
    }
    let manifest: PeerGameManifest = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("parse manifest: {e}")))?;

    let install_token = uuid::Uuid::new_v4().to_string();
    let return_token = install_token.clone();
    let progress = DownloadProgress {
        install_token: install_token.clone(),
        source_device_id: manifest.source_device_id.clone(),
        source_device_name: manifest.source_device_name.clone(),
        source_game_id: manifest.game_id.clone(),
        game_name: manifest.game_name.clone(),
        bytes_done: 0,
        bytes_total: manifest.total_bytes,
        current_file: String::new(),
        status: "starting".into(),
        message: None,
        new_game_id: None,
        bytes_per_second: 0.0,
    };

    // Reserve the slot up front — if someone else is mid-install, fail
    // here rather than spawning a doomed task.
    let _check = state.try_start(progress.clone())?;
    drop(_check); // we'll re-acquire inside the task so the guard owns the task lifetime
    // ^ this opens a tiny race window where another caller could slip in;
    // the spawned task re-acquires immediately below, and on the rare
    // collision the second caller gets the same "already in progress"
    // error one tick later. Worth the simpler ownership story.

    emit_progress(&app, &progress);

    let app_clone = app.clone();
    let state_handle: tauri::State<'_, LanDownloadState> = app.state::<LanDownloadState>();
    // We can't move a `State` across an `await`, but `LanDownloadState`
    // lives on the AppHandle's managed map for the whole process — so
    // re-fetching inside the task is the idiomatic move.
    let _ = state_handle;

    tauri::async_runtime::spawn(async move {
        let result =
            run_install(app_clone.clone(), peer_addr.clone(), peer_port, manifest.clone()).await;
        // Final event. On error, surface the message; on success, point
        // at the freshly-created library entry.
        let final_progress = match result {
            Ok(new_id) => DownloadProgress {
                install_token: install_token.clone(),
                source_device_id: manifest.source_device_id.clone(),
                source_device_name: manifest.source_device_name.clone(),
                source_game_id: manifest.game_id.clone(),
                game_name: manifest.game_name.clone(),
                bytes_done: manifest.total_bytes,
                bytes_total: manifest.total_bytes,
                current_file: String::new(),
                status: "done".into(),
                message: None,
                new_game_id: Some(new_id),
                bytes_per_second: 0.0,
            },
            Err(e) => {
                // Cancellation is a typed variant on `AppError` so this
                // branch is exact rather than string-matched.
                if e.is_canceled() {
                    tracing::info!(
                        game = %manifest.game_name,
                        by_host = matches!(e, AppError::HostCanceled),
                        "LAN install cancelled",
                    );
                    DownloadProgress {
                        install_token: install_token.clone(),
                        source_device_id: manifest.source_device_id.clone(),
                        source_device_name: manifest.source_device_name.clone(),
                        source_game_id: manifest.game_id.clone(),
                        game_name: manifest.game_name.clone(),
                        bytes_done: 0,
                        bytes_total: manifest.total_bytes,
                        current_file: String::new(),
                        status: "canceled".into(),
                        message: None,
                        new_game_id: None,
                        bytes_per_second: 0.0,
                    }
                } else {
                    tracing::warn!(game = %manifest.game_name, error = %e, "LAN install failed");
                    DownloadProgress {
                        install_token: install_token.clone(),
                        source_device_id: manifest.source_device_id.clone(),
                        source_device_name: manifest.source_device_name.clone(),
                        source_game_id: manifest.game_id.clone(),
                        game_name: manifest.game_name.clone(),
                        bytes_done: 0,
                        bytes_total: manifest.total_bytes,
                        current_file: String::new(),
                        status: "error".into(),
                        message: Some(e.to_string()),
                        new_game_id: None,
                        bytes_per_second: 0.0,
                    }
                }
            }
        };
        // Publish the final state. `State<'_, T>` is borrowed from
        // `app_clone` and the lock guard's lifetime ties back to it —
        // so we delegate to a method on the state that takes ownership
        // of the lock internally and avoids holding the borrow.
        app_clone
            .state::<LanDownloadState>()
            .set(Some(final_progress.clone()));
        emit_progress(&app_clone, &final_progress);
        // Brief grace period so the UI can pick up the terminal state
        // via snapshot before we clear it. 2 s feels right — long enough
        // for the toast to settle, short enough that a fresh popover
        // open doesn't see stale data.
        tokio::time::sleep(Duration::from_secs(2)).await;
        app_clone
            .state::<LanDownloadState>()
            .clear_if_token(&install_token);
    });

    Ok(return_token)
}

/// Streams one file from the peer. Honours resume (probes the on-disk
/// remnant and sends a Range header if needed), polls the cancel flag
/// between chunks, and bumps the shared `bytes_done` counter as bytes
/// land. Progress event emission is throttled by `last_emit` so
/// thousands of tiny chunks don't drown the IPC channel.
/// One file's worth of LAN install. `max_bps` is the configured
/// bandwidth cap in bytes/s (0 = unlimited).
async fn download_one_file(
    file: PeerFile,
    partial_dir: PathBuf,
    url: String,
    client: reqwest::Client,
    app: AppHandle,
    bytes_done: Arc<AtomicU64>,
    last_emit: Arc<Mutex<Instant>>,
    max_bps: f64,
) -> AppResult<()> {
    {
        let state = app.state::<LanDownloadState>();
        if state.is_canceled() {
            return Err(state.cancel_error());
        }
    }

    let target = partial_dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Other(format!("mkdir {parent:?}: {e}")))?;
    }

    // Resume support: if a leftover from a previous run sits at
    // `target`, ask the server for just the tail. Three branches:
    //   - already complete (size == expected): skip the GET entirely
    //   - partial (0 < existing < expected): Range request, append
    //   - oversized: corrupt remnant, truncate and re-fetch
    let existing_size = match tokio::fs::metadata(&target).await {
        Ok(m) if m.is_file() => m.len(),
        _ => 0,
    };
    if existing_size == file.size {
        bytes_done.fetch_add(file.size, Ordering::Relaxed);
        maybe_emit_progress(&app, &bytes_done, &last_emit, &file.path);
        return Ok(());
    }
    let resume_from = if existing_size < file.size {
        existing_size
    } else {
        0
    };

    let mut request = client.get(&url);
    if resume_from > 0 {
        request = request.header(header::RANGE, format!("bytes={resume_from}-"));
    }
    let resp = request
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET {url}: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::Other(format!(
            "peer returned {} for {}",
            status, file.path
        )));
    }
    let server_served_range = status == reqwest::StatusCode::PARTIAL_CONTENT;
    let appending = resume_from > 0 && server_served_range;

    let mut out = if appending {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&target)
            .await
            .map_err(|e| AppError::Other(format!("open append {target:?}: {e}")))?
    } else {
        tokio::fs::File::create(&target)
            .await
            .map_err(|e| AppError::Other(format!("create {target:?}: {e}")))?
    };
    if appending {
        bytes_done.fetch_add(resume_from, Ordering::Relaxed);
    }

    // Hasher running in parallel with disk writes. When the source
    // didn't include a hash (older peer, empty file), `expected` stays
    // empty and we skip verification on the way out.
    let expected = file.hash.clone();
    let verify = !expected.is_empty();
    let mut hasher = blake3::Hasher::new();
    if verify && appending && resume_from > 0 {
        // Pre-seed the hasher with the already-on-disk prefix so the
        // final digest covers the whole file (not just the tail we
        // just downloaded). One sequential read; modest cost vs. the
        // alternative of re-downloading everything from byte 0.
        let mut existing = tokio::fs::File::open(&target)
            .await
            .map_err(|e| AppError::Other(format!("open existing {target:?}: {e}")))?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = existing
                .read(&mut buf)
                .await
                .map_err(|e| AppError::Other(format!("read existing {target:?}: {e}")))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }

    // Surface "now starting this file" — racy with sibling tasks; that's
    // fine, the UI just shows one representative file name.
    if let Some(snap) = app.state::<LanDownloadState>().update(|p| {
        p.status = "transferring".into();
        p.current_file = file.path.clone();
        p.bytes_done = bytes_done.load(Ordering::Relaxed);
    }) {
        emit_progress(&app, &snap);
    }

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        {
            let state = app.state::<LanDownloadState>();
            if state.is_canceled() {
                // Drop the file before any directory cleanup — Windows
                // refuses to remove a dir that still has open handles.
                drop(out);
                return Err(state.cancel_error());
            }
        }
        let chunk = chunk.map_err(|e| AppError::Other(format!("recv chunk: {e}")))?;
        if verify {
            hasher.update(&chunk);
        }
        out.write_all(&chunk)
            .await
            .map_err(|e| AppError::Other(format!("write {target:?}: {e}")))?;
        let bd_after = bytes_done.fetch_add(chunk.len() as u64, Ordering::Relaxed)
            + chunk.len() as u64;
        maybe_emit_progress(&app, &bytes_done, &last_emit, &file.path);

        // Bandwidth throttle. Each parallel task consults the same
        // shared `bytes_done` + start_instant, so they collectively
        // drag the aggregate rate under the cap rather than each
        // policing themselves into a too-low rate. No-op when the
        // user hasn't configured a cap (max_bps == 0).
        if let Some(sleep) =
            app.state::<LanDownloadState>().throttle_required(bd_after, max_bps)
        {
            tokio::time::sleep(sleep).await;
        }
    }
    out.flush()
        .await
        .map_err(|e| AppError::Other(format!("flush {target:?}: {e}")))?;
    drop(out);

    // Verify the digest. On mismatch we delete the corrupt file so the
    // next attempt re-fetches from scratch — leaving a wrong-bytes
    // file in `.partial` would just trigger the same failure forever.
    if verify {
        let actual = hasher.finalize().to_hex().to_string();
        if actual != expected {
            let _ = tokio::fs::remove_file(&target).await;
            return Err(AppError::ChecksumMismatch {
                path: file.path.clone(),
                expected,
                actual,
            });
        }
    }

    // Restamp mtime so the destination matches the source. Best-effort:
    // a failure here is cosmetic (the file is fine), don't fail the
    // install over it.
    if file.mtime_unix_ms > 0 {
        let mtime = filetime::FileTime::from_unix_time(
            (file.mtime_unix_ms / 1000) as i64,
            ((file.mtime_unix_ms % 1000) * 1_000_000) as u32,
        );
        let target_for_blocking = target.clone();
        let _ = tokio::task::spawn_blocking(move || {
            filetime::set_file_mtime(&target_for_blocking, mtime)
        })
        .await;
    }

    Ok(())
}

/// Throttled progress emit. Multiple parallel tasks race for the lock;
/// whichever task wins the "last_emit too old?" check fires the event,
/// the rest silently skip. The brief `std::sync::Mutex<Instant>` lock
/// is dropped before any work — we never hold a sync mutex across an
/// await.
fn maybe_emit_progress(
    app: &AppHandle,
    bytes_done: &AtomicU64,
    last_emit: &Mutex<Instant>,
    current_file: &str,
) {
    let should_emit = {
        match last_emit.lock() {
            Ok(mut le) if le.elapsed() >= PROGRESS_EMIT_INTERVAL => {
                *le = Instant::now();
                true
            }
            _ => false,
        }
    };
    if !should_emit {
        return;
    }
    let bd = bytes_done.load(Ordering::Relaxed);
    if let Some(snap) = app.state::<LanDownloadState>().update(|p| {
        p.bytes_done = bd;
        p.current_file = current_file.to_string();
    }) {
        emit_progress(app, &snap);
    }
}

/// Heavy lifting for `start_peer_install` — runs in the spawned task.
/// Returns the new library entry's id on success.
async fn run_install(
    app: AppHandle,
    peer_addr: String,
    peer_port: u16,
    manifest: PeerGameManifest,
) -> AppResult<String> {
    let root = install_root_from(&app)?;
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|e| AppError::Other(format!("create install root: {e}")))?;

    // Resume detection: if a `.partial` exists at the preferred name
    // we pick up where we left off rather than allocating a fresh
    // `<name> (2)` install.
    let (final_dir, partial_dir, resuming) =
        resolve_install_dirs(&root, &manifest.safe_name);
    if resuming {
        tracing::info!(
            partial = %partial_dir.display(),
            "resuming previous LAN install"
        );
    } else {
        tokio::fs::create_dir_all(&partial_dir)
            .await
            .map_err(|e| AppError::Other(format!("create partial dir: {e}")))?;
    }

    // Reuse the process-wide shared client. The shared client has no
    // top-level timeout so multi-GB transfers can run as long as they
    // need; the heartbeat uses RequestBuilder::timeout for the short
    // poll. (Per `m07` + `domain-web`: one client per process, share
    // its connection pool + DNS cache.)
    let client: reqwest::Client = (*app.state::<reqwest::Client>()).clone();

    // Shared counters for the parallel file downloads. `bytes_done`
    // accumulates across all tasks; `last_emit` throttles the progress
    // event firehose to ~5 Hz instead of the per-chunk rate (which on
    // a gigabit transfer is thousands per second).
    let bytes_done = Arc::new(AtomicU64::new(0));
    let last_emit = Arc::new(Mutex::new(
        Instant::now() - PROGRESS_EMIT_INTERVAL * 2,
    ));

    // Build the per-file futures. We stream them through
    // `buffer_unordered(LAN_PARALLEL_FILES)` so the slot keeps full
    // even when individual files vary wildly in size. A first-error
    // short-circuit drops the rest cooperatively.
    let manifest_game_id = manifest.game_id.clone();
    // The install_token doubles as the upload session id seen by the
    // source — its host UI groups all 4 of our parallel file fetches
    // into a single row, and host-side cancel keys off it. Reach for
    // it via the public `snapshot()` so we don't touch the private
    // `current` field through a temporary `State<'_, _>`.
    let session_id_for_url = app
        .state::<LanDownloadState>()
        .snapshot()
        .map(|p| p.install_token)
        .unwrap_or_default();
    let game_name_for_url = manifest.game_name.clone();

    // Snapshot the bandwidth cap once at install start. Mid-install
    // setting changes won't take effect until the next install —
    // simpler than threading config through every chunk loop. Convert
    // MB/s → bytes/s here so the chunk loop doesn't repeat the math.
    let max_bps = {
        let cfg = app.state::<SharedConfig>();
        let mbps = cfg
            .lock()
            .map(|c| c.data.lan_download_max_mbps)
            .unwrap_or(0.0);
        mbps * 1024.0 * 1024.0
    };

    let file_futures = manifest.files.clone().into_iter().map(|file| {
        let partial_dir = partial_dir.clone();
        let client = client.clone();
        let app = app.clone();
        let bytes_done = bytes_done.clone();
        let last_emit = last_emit.clone();
        let peer_addr = peer_addr.clone();
        let game_id = manifest_game_id.clone();
        let session = session_id_for_url.clone();
        let game_name = game_name_for_url.clone();
        async move {
            // URL-encode each segment so spaces / special chars survive.
            let encoded = file
                .path
                .split('/')
                .map(|seg| urlencoding::encode(seg).into_owned())
                .collect::<Vec<_>>()
                .join("/");
            // Session + game_name query params let the source group us
            // into a single "uploads" row and show a friendly title.
            let url = format!(
                "http://{peer_addr}:{peer_port}/games/{game_id}/files/{encoded}?session={}&game_name={}",
                urlencoding::encode(&session),
                urlencoding::encode(&game_name),
            );
            download_one_file(
                file,
                partial_dir,
                url,
                client,
                app,
                bytes_done,
                last_emit,
                max_bps,
            )
            .await
        }
    });

    // Heartbeat: poll the source's /cancel-check every ~3 s so a
    // host-initiated cancel takes effect promptly even between file
    // fetches. On 410 we set the same cancel_flag the user-initiated
    // path uses, so the rest of the code converges to a clean abort.
    let heartbeat = {
        let app_for_hb = app.clone();
        let session = session_id_for_url.clone();
        let game_id = manifest_game_id.clone();
        let peer_addr = peer_addr.clone();
        // Reuse the shared client; per-request timeout via RequestBuilder.
        let hb_client: reqwest::Client = (*app.state::<reqwest::Client>()).clone();
        tokio::spawn(async move {
            let url = format!(
                "http://{peer_addr}:{peer_port}/games/{game_id}/cancel-check?session={}",
                urlencoding::encode(&session)
            );
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let state = app_for_hb.state::<LanDownloadState>();
                if state.is_canceled() {
                    return;
                }
                // GONE (410) is the "host cancelled" signal.
                if let Ok(resp) = hb_client
                    .get(&url)
                    .timeout(Duration::from_secs(3))
                    .send()
                    .await
                {
                    if resp.status() == reqwest::StatusCode::GONE {
                        tracing::info!("LAN install: host cancelled the upload");
                        state.request_host_cancel();
                        return;
                    }
                }
            }
        })
    };

    let mut stream = futures_util::stream::iter(file_futures)
        .buffer_unordered(LAN_PARALLEL_FILES);
    // Drain until cancel or first error. We capture the terminal state
    // into `maybe_err` so we can finish cleanup (drop stream → cancel
    // in-flight tasks; abort heartbeat) before propagating up.
    let mut maybe_err: Option<AppError> = None;
    while let Some(result) = stream.next().await {
        {
            let state = app.state::<LanDownloadState>();
            if state.is_canceled() {
                maybe_err = Some(state.cancel_error());
                break;
            }
        }
        if let Err(e) = result {
            maybe_err = Some(e);
            break;
        }
    }
    drop(stream);
    heartbeat.abort();
    if let Some(e) = maybe_err {
        // Any flavour of cancel wipes the partial dir so a fresh
        // attempt doesn't pick up half-written state. Other errors
        // keep the partial dir so the user can retry with resume.
        if e.is_canceled() {
            let _ = tokio::fs::remove_dir_all(&partial_dir).await;
        }
        return Err(e);
    }
    // Final progress flush — make sure the UI shows 100% before we
    // emit the terminal "done" event.
    let final_bd = bytes_done.load(Ordering::Relaxed);
    if let Some(snap) = app
        .state::<LanDownloadState>()
        .update(|p| p.bytes_done = final_bd)
    {
        emit_progress(&app, &snap);
    }

    // All files landed — flip the staging dir into its real location.
    tokio::fs::rename(&partial_dir, &final_dir)
        .await
        .map_err(|e| AppError::Other(format!("finalise install dir: {e}")))?;

    // Build the library entry. exe_path is the manifest-supplied
    // relative path joined to our final install dir; if the source
    // didn't have one we leave it empty and the user wires it up.
    let exe_path = manifest
        .exe_relative_path
        .as_ref()
        .map(|rel| {
            final_dir
                .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default();

    let new_id = uuid::Uuid::new_v4().to_string();
    let library = app.state::<SharedLibrary>();
    let entry = {
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = GameEntry {
            id: new_id.clone(),
            catalog_number: lib.next_catalog_number(),
            game_name: manifest.game_name.clone(),
            exe_path,
            safe_name: manifest.safe_name.clone(),
            added_at: Some(Utc::now()),
            game_folder_path: Some(final_dir.to_string_lossy().to_string()),
            steam_id: manifest.steam_id,
            gog_id: manifest.gog_id,
            lutris_slug: manifest.lutris_slug.clone(),
            has_cloud_save: manifest.has_cloud_save,
            manifest_install_dir: manifest.manifest_install_dir.clone(),
            save_paths: manifest.save_paths.clone(),
            developer: manifest.developer.clone(),
            publisher: manifest.publisher.clone(),
            genres: manifest.genres.clone(),
            release_date: manifest.release_date,
            install_size_mb: (manifest.total_bytes as f64) / (1024.0 * 1024.0),
            install_source: "lan".to_string(),
            lan_install_source_device_id: Some(manifest.source_device_id.clone()),
            lan_install_source_device_name: Some(manifest.source_device_name.clone()),
            ..GameEntry::default()
        };
        lib.entries.push(entry.clone());
        lib.save()?;
        entry
    };

    if let Err(e) = app.emit("library:changed", &entry.id) {
        tracing::warn!(error = %e, "failed to emit library:changed after LAN install");
    }

    // Background artwork fetch. Try the peer's `/cover` and `/hero`
    // first — that gives us pixel-identical art with no SteamGridDB
    // API key requirement and works for games SGDB doesn't index.
    // If the peer 404s the cover (older Spool, no local cover), fall
    // back to the regular SteamGridDB fetch.
    let app_for_art = app.clone();
    let id_for_art = entry.id.clone();
    let safe_name_for_art = entry.safe_name.clone();
    let peer_addr_for_art = peer_addr.clone();
    let source_id_for_art = manifest.game_id.clone();
    tauri::async_runtime::spawn(async move {
        let got_cover = fetch_peer_artwork(
            &app_for_art,
            &id_for_art,
            &safe_name_for_art,
            &peer_addr_for_art,
            peer_port,
            &source_id_for_art,
        )
        .await;
        if !got_cover {
            if let Err(e) =
                crate::steamgriddb::fetch_and_save_cover(&app_for_art, &id_for_art).await
            {
                tracing::warn!(
                    game_id = %id_for_art,
                    error = %e,
                    "cover fetch failed (peer 404 + SteamGridDB fallback)"
                );
            }
        }
    });

    Ok(new_id)
}

/// Fetches cover + hero artwork from a peer and writes them into the
/// covers/ dir, then updates the library entry's image paths +
/// accent_color. Best-effort: each fetch (cover and hero) is
/// independent — if hero 404s we still keep the cover, and vice
/// versa. Returns `true` if a cover landed, which is what the caller
/// uses to decide whether to fall back to SteamGridDB.
async fn fetch_peer_artwork(
    app: &AppHandle,
    new_game_id: &str,
    safe_name: &str,
    peer_addr: &str,
    peer_port: u16,
    source_game_id: &str,
) -> bool {
    // Shared client; the 30s budget is applied per request below.
    let client: reqwest::Client = (*app.state::<reqwest::Client>()).clone();

    let covers_dir = paths::covers_dir();
    if tokio::fs::create_dir_all(&covers_dir).await.is_err() {
        return false;
    }

    let cover_url =
        format!("http://{peer_addr}:{peer_port}/games/{source_game_id}/cover");
    let hero_url =
        format!("http://{peer_addr}:{peer_port}/games/{source_game_id}/hero");

    // Fetch both in parallel — they're tiny relative to the game
    // bytes and there's no point serialising them.
    let (cover_path, hero_path) = tokio::join!(
        fetch_and_save_peer_image(&client, &cover_url, &covers_dir, safe_name, ""),
        fetch_and_save_peer_image(&client, &hero_url, &covers_dir, safe_name, "-hero"),
    );

    if cover_path.is_none() && hero_path.is_none() {
        return false;
    }

    // Accent extraction is best-effort and only meaningful from the
    // portrait cover. Heroes are wide and would skew the colour.
    // Image decode + histogram is sync CPU/disk work (~10ms for a
    // typical cover), so per `m07-concurrency` it lives on
    // `spawn_blocking` rather than blocking the async runtime.
    let accent = if let Some(p) = cover_path.as_ref() {
        let p = p.clone();
        tokio::task::spawn_blocking(move || crate::steamgriddb::extract_vibrant_color(&p))
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    // Update the library entry. Same shape as the pattern in
    // `run_install` above (line ~1825): bind State to a local first
    // so the MutexGuard's borrow has a stable anchor — Tauri's
    // `State<'_, T>` lifetime + a chained `.lock()` confuses the
    // borrow checker otherwise.
    let library = app.state::<SharedLibrary>();
    if let Ok(mut lib) = library.lock() {
        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == new_game_id) {
            if let Some(p) = &cover_path {
                entry.cover_image_path = Some(p.to_string_lossy().to_string());
            }
            if let Some(p) = &hero_path {
                entry.hero_image_path = Some(p.to_string_lossy().to_string());
            }
            if let Some(a) = accent {
                entry.accent_color = Some(a);
            }
        }
        let _ = lib.save();
    }
    drop(library);
    let _ = app.emit("library:changed", &new_game_id.to_string());
    cover_path.is_some()
}

/// Downloads one image from `url` and saves it as
/// `<dir>/<safe_name><suffix>.<ext>` where the extension is sniffed
/// from the response's Content-Type. Returns the path on success,
/// `None` on any failure (404, network, write error).
async fn fetch_and_save_peer_image(
    client: &reqwest::Client,
    url: &str,
    dir: &Path,
    safe_name: &str,
    suffix: &str,
) -> Option<PathBuf> {
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_default();
    let ext = crate::steamgriddb::mime_to_ext(&mime).unwrap_or("jpg");
    let bytes = resp.bytes().await.ok()?;
    let path = dir.join(format!("{safe_name}{suffix}.{ext}"));
    tokio::fs::write(&path, &bytes).await.ok()?;
    Some(path)
}

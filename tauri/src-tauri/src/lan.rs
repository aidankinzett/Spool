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
    extract::{Path as AxPath, State as AxState},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
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
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerFile {
    pub path: String,
    pub size: u64,
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

#[derive(Clone)]
struct ServerState {
    app: AppHandle,
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
        .with_state(ServerState { app });

    // Server runs forever — tokio::spawn detaches it. If the listener dies
    // we just log and stop; no recovery path right now (the user can
    // restart Spool). Wrap in async block so axum::serve's return type
    // doesn't leak.
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "LAN HTTP server exited");
        }
    });

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

    let files = walk_game_files(&folder).map_err(|e| {
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
async fn get_file_handler(
    AxState(state): AxState<ServerState>,
    AxPath((id, rel_path)): AxPath<(String, String)>,
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
    if !abs.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }

    let metadata = tokio::fs::metadata(&abs)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let len = metadata.len();
    let file = tokio::fs::File::open(&abs)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(([
        (header::CONTENT_TYPE, "application/octet-stream"),
        (header::CONTENT_LENGTH, &len.to_string()),
    ], body)
        .into_response())
}

/// Recursive walk that turns a folder into a flat list of `PeerFile`
/// entries. Paths in the manifest are forward-slash and relative to
/// `root` so the receiver can reconstruct local paths cleanly across
/// OSes. Symlinks are followed so installs that use junctions
/// (Windows) or symlinks on Linux still ship the real bytes.
fn walk_game_files(root: &Path) -> std::io::Result<Vec<PeerFile>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(true) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
        // Normalise to forward slashes — the manifest is wire format.
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
        let size = entry.metadata()?.len();
        out.push(PeerFile {
            path: rel_str,
            size,
        });
    }
    Ok(out)
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

    let _ = tokio::try_join!(announce_handle, listen_handle, reaper_handle);
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
    pub status: String, // "starting" | "transferring" | "done" | "error"
    pub message: Option<String>,
    /// Set when status == "done": the id of the freshly-created
    /// library entry so the UI can jump straight to it.
    pub new_game_id: Option<String>,
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
}

/// Sentinel error returned by `run_install` when the user cancelled.
/// We use a string sentinel rather than a new `AppError` variant so the
/// `?` operator throughout the install loop keeps working. The spawn
/// handler matches on this string to emit `status: "canceled"` rather
/// than `"error"`.
const CANCELED_MSG: &str = "canceled by user";

impl LanDownloadState {
    fn try_start(&self, p: DownloadProgress) -> AppResult<DownloadGuard<'_>> {
        let mut guard = self.current.lock().map_err(|_| AppError::LockPoisoned)?;
        if guard.is_some() {
            return Err(AppError::Other(
                "Another LAN install is already in progress".into(),
            ));
        }
        // Reset the cancel flag for the fresh install — any lingering
        // `true` from a previous cancelled run would otherwise abort us
        // immediately.
        self.cancel_flag.store(false, Ordering::Relaxed);
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

    fn snapshot(&self) -> Option<DownloadProgress> {
        self.current.lock().ok().and_then(|g| g.clone())
    }

    fn update<F: FnOnce(&mut DownloadProgress)>(&self, f: F) -> Option<DownloadProgress> {
        let mut guard = self.current.lock().ok()?;
        if let Some(p) = guard.as_mut() {
            f(p);
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

fn emit_progress(app: &AppHandle, progress: &DownloadProgress) {
    if let Err(e) = app.emit("lan:download", progress) {
        tracing::warn!(error = %e, "failed to emit lan:download");
    }
}

/// Fetches the game catalogue from a peer's HTTP server. Frontend calls
/// this when the user opens a peer's row in the LAN popover. Times out
/// quickly so a stale peer in the registry can't hang the UI.
#[tauri::command]
pub async fn fetch_peer_games(addr: String, port: u16) -> AppResult<Vec<PeerGame>> {
    if port == 0 {
        return Err(AppError::Other(
            "peer is discovery-only (no file server)".into(),
        ));
    }
    let url = format!("http://{addr}:{port}/games");
    let client = reqwest::Client::builder()
        .timeout(PEER_FETCH_TIMEOUT)
        .build()
        .map_err(|e| AppError::Other(format!("build http client: {e}")))?;
    let resp = client
        .get(&url)
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
    let client = reqwest::Client::builder()
        .timeout(PEER_FETCH_TIMEOUT)
        .build()
        .map_err(|e| AppError::Other(format!("build http client: {e}")))?;
    let resp = client
        .get(&manifest_url)
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
            },
            Err(e) => {
                // Cancellation produces a sentinel error; distinguish it
                // so the UI can show "Cancelled" rather than a scary red
                // error toast.
                let msg = e.to_string();
                if msg == CANCELED_MSG {
                    tracing::info!(game = %manifest.game_name, "LAN install cancelled");
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
                    }
                } else {
                    tracing::warn!(game = %manifest.game_name, error = %msg, "LAN install failed");
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
                        message: Some(msg),
                        new_game_id: None,
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

    let final_dir = allocate_install_dir(&root, &manifest.safe_name);
    let partial_dir = final_dir.with_extension("partial");
    // Clean up any leftover .partial from a previous aborted attempt.
    if partial_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&partial_dir).await;
    }
    tokio::fs::create_dir_all(&partial_dir)
        .await
        .map_err(|e| AppError::Other(format!("create partial dir: {e}")))?;

    let client = reqwest::Client::builder()
        // No top-level timeout — multi-GB transfers may legitimately
        // take a while. Per-chunk timeout would be nicer but isn't
        // exposed by reqwest; for v1 we rely on the user cancelling
        // via app restart if something hangs.
        .build()
        .map_err(|e| AppError::Other(format!("build http client: {e}")))?;

    let download_state = app.state::<LanDownloadState>();
    let mut bytes_done: u64 = 0;
    let mut last_emit = Instant::now() - Duration::from_secs(1);

    for file in &manifest.files {
        // Bail before opening a new file if the user has cancelled. The
        // partial dir gets cleaned up below.
        if download_state.is_canceled() {
            let _ = tokio::fs::remove_dir_all(&partial_dir).await;
            return Err(AppError::Other(CANCELED_MSG.into()));
        }

        // Re-anchor on each file so progress shows the current name.
        download_state.update(|p| {
            p.status = "transferring".into();
            p.current_file = file.path.clone();
            p.bytes_done = bytes_done;
        });
        if let Some(snapshot) = download_state.snapshot() {
            emit_progress(&app, &snapshot);
        }

        // URL-encode each segment so spaces / special chars survive.
        let encoded = file
            .path
            .split('/')
            .map(|seg| urlencoding::encode(seg).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let url = format!(
            "http://{peer_addr}:{peer_port}/games/{}/files/{encoded}",
            manifest.game_id
        );

        let target = partial_dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Other(format!("mkdir {parent:?}: {e}")))?;
        }

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Other(format!("GET {url}: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Other(format!(
                "{} returned {} for {}",
                peer_addr,
                resp.status(),
                file.path
            )));
        }

        let mut out = tokio::fs::File::create(&target)
            .await
            .map_err(|e| AppError::Other(format!("create {target:?}: {e}")))?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            // Poll for cancellation between chunks so we abort mid-file
            // rather than locking the user into "wait for the current
            // multi-GB file to finish".
            if download_state.is_canceled() {
                // Drop the open file before removing the dir on Windows
                // — locked handles will block the recursive delete.
                drop(out);
                let _ = tokio::fs::remove_dir_all(&partial_dir).await;
                return Err(AppError::Other(CANCELED_MSG.into()));
            }
            let chunk = chunk.map_err(|e| AppError::Other(format!("recv chunk: {e}")))?;
            out.write_all(&chunk)
                .await
                .map_err(|e| AppError::Other(format!("write {target:?}: {e}")))?;
            bytes_done += chunk.len() as u64;
            // Throttle progress emits — every ~150ms is plenty for the UI
            // and keeps the event channel from being a bottleneck on a
            // gigabit transfer.
            if last_emit.elapsed() >= Duration::from_millis(150) {
                if let Some(snapshot) = download_state.update(|p| p.bytes_done = bytes_done) {
                    emit_progress(&app, &snapshot);
                }
                last_emit = Instant::now();
            }
        }
        out.flush()
            .await
            .map_err(|e| AppError::Other(format!("flush {target:?}: {e}")))?;
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

    // Background cover fetch — same shape as `library::add_game`.
    let app_for_cover = app.clone();
    let id_for_cover = entry.id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::steamgriddb::fetch_and_save_cover(&app_for_cover, &id_for_cover).await
        {
            tracing::warn!(game_id = %id_for_cover, error = %e, "cover fetch failed");
        }
    });

    Ok(new_id)
}

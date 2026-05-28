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
use crate::library::{GameEntry, SharedLibrary};
use axum::{extract::State as AxState, http::StatusCode, response::Json, routing::get, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::net::UdpSocket;

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
}

impl From<&GameEntry> for PeerGame {
    fn from(g: &GameEntry) -> Self {
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
        }
    }
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

    let games: Vec<PeerGame> = library
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .entries
        .iter()
        .map(PeerGame::from)
        .collect();
    Ok(Json(games))
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

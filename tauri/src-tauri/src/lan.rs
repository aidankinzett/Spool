//! LAN peer discovery — Phase A.
//!
//! Every 5 s the running Spool instance broadcasts a small JSON
//! announce packet over UDP multicast (`239.255.83.83:47631`). All
//! other Spool instances on the same broadcast domain are listening on
//! the same group; each remembers who they've seen recently. Peers go
//! stale and drop from the registry 30 s after their last sighting.
//!
//! Future phases will add an HTTP file server (Phase B) and the
//! download flow (Phase C). The announce packet already carries a
//! `file_server_port` field — currently `0` (server not implemented)
//! so peers know we don't accept transfers yet.
//!
//! Multicast group `239.255.83.83` is in the admin-scoped range
//! (`239.255.0.0/16`) which routers won't forward beyond the local
//! network — exactly the scope we want.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
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
    /// HTTP file server port on the announcing peer. `0` = "not yet
    /// running a file server, discovery-only".
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

// ── Spawning ────────────────────────────────────────────────────────────────

/// Spawns the announce, listen, and reaper background tasks. Called once
/// from `lib.rs::run`'s setup hook. Failures during socket setup are
/// logged but non-fatal — the app continues without LAN discovery.
pub fn spawn_discovery(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_discovery(app).await {
            tracing::warn!(error = %e, "LAN discovery exited; peers will not be listed");
        }
    });
}

async fn run_discovery(app: AppHandle) -> AppResult<()> {
    // Snapshot device identity from config — used by both announces and
    // self-loop filtering.
    let (device_id, device_name) = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        (cfg.data.device_id.clone(), cfg.data.device_name.clone())
    };

    if device_id.is_empty() {
        return Err(AppError::Other(
            "device_id not assigned yet — skipping LAN discovery".into(),
        ));
    }

    let socket = make_multicast_socket()?;
    let socket = Arc::new(socket);
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
        tokio::spawn(async move {
            announce_loop(socket, app, device_id, device_name).await;
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

async fn announce_loop(socket: Arc<UdpSocket>, app: AppHandle, device_id: String, device_name: String) {
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
            file_server_port: 0,
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
            let count_changed = entry
                .map(|e| e.peer.game_count != packet.game_count)
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
            is_new || count_changed
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

// ── Tauri command ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_lan_peers(state: State<'_, LanState>) -> Vec<LanPeer> {
    state.snapshot()
}

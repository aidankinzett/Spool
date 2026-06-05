//! UDP broadcast discovery + the background-task orchestration that wires
//! the announce/listen/reaper loops together with the HTTP server.

use super::server::start_http_server;
use super::{LanUploadsState, UploadSession};
use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::net::UdpSocket;

const BROADCAST_ADDR: Ipv4Addr = Ipv4Addr::BROADCAST;
const DISCOVERY_PORT: u16 = 47631;
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
    /// discovery port quickly. Broadcast means anything on the LAN
    /// sending to `:47631` lands in our recv buffer, so this check is
    /// the first line of "is this even ours?" filtering.
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

    pub fn snapshot(&self) -> Vec<LanPeer> {
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
            cfg.data.lan.share_port,
            cfg.data.lan.share_enabled,
        )
    };

    if device_id.is_empty() {
        return Err(AppError::Other(
            "device_id not assigned yet — skipping LAN discovery".into(),
        ));
    }

    let socket = make_discovery_socket()?;
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
        "LAN discovery started on {BROADCAST_ADDR}:{DISCOVERY_PORT}"
    );

    let lan_state = app.state::<LanState>().peers.clone();

    // GUI side: notify the frontend via Tauri events whenever the peer set
    // changes. The headless plugin server reuses these same loops with a
    // no-op callback (it polls instead of pushing).
    let on_change: Arc<dyn Fn() + Send + Sync> = {
        let app = app.clone();
        Arc::new(move || {
            let _ = app.emit("lan:peers-changed", &());
        })
    };

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
        let device_id = device_id.clone();
        let peers = lan_state.clone();
        let on_change = on_change.clone();
        tokio::spawn(async move {
            listen_loop(socket, device_id, peers, on_change).await;
        })
    };
    let reaper_handle = {
        let peers = lan_state;
        let on_change = on_change.clone();
        tokio::spawn(async move {
            reaper_loop(peers, on_change).await;
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

/// Configures the UDP socket: bind to `0.0.0.0:DISCOVERY_PORT` with
/// SO_REUSEADDR (multiple processes can listen — useful in dev) and
/// SO_BROADCAST (required on Windows/Linux to send to
/// `255.255.255.255`). Tokio's UdpSocket can't set these pre-bind, so
/// we go through socket2 then convert.
///
/// Two Spool instances on the same machine still see each other: a
/// broadcast sent locally is delivered back to every socket bound to
/// `0.0.0.0:DISCOVERY_PORT`, and `device_id`-self-suppression in
/// `listen_loop` drops our own announces. No explicit loopback toggle
/// needed — that was a multicast-specific concern.
pub(crate) fn make_discovery_socket() -> AppResult<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| AppError::Other(format!("socket create: {e}")))?;
    socket
        .set_reuse_address(true)
        .map_err(|e| AppError::Other(format!("set SO_REUSEADDR: {e}")))?;
    socket
        .set_broadcast(true)
        .map_err(|e| AppError::Other(format!("set SO_BROADCAST: {e}")))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| AppError::Other(format!("set nonblocking: {e}")))?;
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)).into())
        .map_err(|e| AppError::Other(format!("bind {DISCOVERY_PORT}: {e}")))?;

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
    let target = SocketAddr::from((BROADCAST_ADDR, DISCOVERY_PORT));
    loop {
        // Read current game count fresh each tick so peers see growth as
        // the user adds games.
        let game_count = app
            .state::<SharedLibrary>()
            .count()
            .await
            .unwrap_or(0) as u32;
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
    our_device_id: String,
    peers: Arc<Mutex<HashMap<String, PeerEntry>>>,
    on_change: Arc<dyn Fn() + Send + Sync>,
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
            Err(_) => continue, // ignore stray UDP / unrelated broadcast traffic
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
                .map(|e| (e.peer.file_server_port == 0) != (packet.file_server_port == 0))
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
            on_change();
        }
    }
}

async fn reaper_loop(
    peers: Arc<Mutex<HashMap<String, PeerEntry>>>,
    on_change: Arc<dyn Fn() + Send + Sync>,
) {
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
            on_change();
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

#[tauri::command]
pub fn list_lan_peers(state: State<'_, LanState>) -> Vec<LanPeer> {
    state.snapshot()
}

/// Headless entry point: spawn just the discovery **listener** (no announce,
/// no file server) so a consumer process — the Decky plugin's
/// `--headless-server` — can list nearby peers without the full Tauri app
/// running. Updates the shared `LanState` in place; callers read it via
/// `LanState::snapshot`. There's no event bus in the headless server, so the
/// change callback is a no-op (the Decky UI polls `/lan/peers`).
///
/// Unix-only: its sole caller is `plugin_server.rs`, which is itself
/// `#![cfg(unix)]`. Without this gate it's dead code on Windows.
#[cfg(unix)]
pub fn spawn_peer_listener(state: Arc<LanState>, our_device_id: String) -> AppResult<()> {
    let socket = Arc::new(make_discovery_socket()?);
    let peers = state.peers.clone();
    let noop: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    {
        let socket = socket.clone();
        let peers = peers.clone();
        let id = our_device_id;
        let on_change = noop.clone();
        tokio::spawn(async move {
            listen_loop(socket, id, peers, on_change).await;
        });
    }
    {
        let on_change = noop;
        tokio::spawn(async move {
            reaper_loop(peers, on_change).await;
        });
    }
    Ok(())
}

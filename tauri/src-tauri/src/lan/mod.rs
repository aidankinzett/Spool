//! LAN peer discovery + library exchange.
//!
//! **Discovery** ([`discovery`]): every 5 s we send a small JSON announce
//! packet over UDP broadcast (`255.255.255.255:47631`) and collect
//! everyone else's announces into a peer registry that stales out after
//! 30 s.
//!
//! **Serving** ([`server`]): each Spool instance also runs a tiny HTTP
//! server that exposes its game library. The announce packet's
//! `file_server_port` field carries the live TCP port so other peers can
//! `GET http://<addr>:<port>/games` and browse what we'd share, then pull
//! a manifest and stream files. Bind tries the user's preferred port from
//! config first, then falls back to an ephemeral port if it's taken (so
//! multiple Spool instances on the same box still come up clean).
//!
//! **Installing** ([`install`]): the receiving half — fetches a peer's
//! manifest, streams every file to a `.partial` staging dir with blake3
//! verification + resume, then renames into place and registers a new
//! library entry.
//!
//! Why broadcast (`255.255.255.255`) and not multicast: consumer mesh
//! routers — notably Google / Nest Wi-Fi — aggressively filter
//! arbitrary admin-scoped multicast groups while still flooding limited
//! broadcasts normally. Broadcast is "ruder" (every host on the link
//! sees the packet) but reliably traverses Wi-Fi ↔ Ethernet bridges
//! where multicast quietly disappears. The packet is tiny and the
//! `magic` + `device_id` checks make junk easy to ignore. Routers
//! won't forward `255.255.255.255` beyond the local network either, so
//! the scope is the same as the previous multicast design. Matches
//! what the original C# build did.

// Submodules are `pub(crate)` rather than re-exporting their commands:
// `#[tauri::command]` emits helper macros (`__cmd__*`) alongside each fn
// in its defining module, and `generate_handler!` needs to reach those by
// path — so `lib.rs` references the commands as `lan::<module>::<command>`.
pub(crate) mod discovery;
pub(crate) mod install;
pub(crate) mod server;

pub use discovery::{spawn_discovery, LanState};
pub use install::LanDownloadState;
pub use server::LanServerShutdown;

use crate::library::GameEntry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    /// Total bytes in the transfer (from the manifest). Zero until the
    /// manifest has been fetched.
    pub bytes_total: u64,
    /// Bytes served to the peer so far (optimistic — credited at request
    /// time). Used by the host UI's progress bar.
    pub bytes_sent: u64,
}

struct UploadSession {
    session_id: String,
    game_id: String,
    game_name: String,
    peer_addr: String,
    last_active: Instant,
    cancelled: bool,
    /// Total bytes in the transfer, populated when the receiver fetches
    /// the manifest. Zero until the manifest request arrives (i.e., while
    /// the sender is still hashing the game folder).
    bytes_total: u64,
    /// Bytes served so far (optimistic — credited at request time, not
    /// on TCP ACK). Good enough for a progress indicator.
    bytes_sent: u64,
    /// Wall-clock anchor for throttling `lan:uploads-changed` emissions.
    last_progress_emit: Instant,
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
                bytes_total: s.bytes_total,
                bytes_sent: if s.bytes_total > 0 {
                    std::cmp::min(s.bytes_sent, s.bytes_total)
                } else {
                    s.bytes_sent
                },
            })
            .collect()
    }

    fn touch(&self, session_id: &str, game_id: &str, game_name: &str, peer_addr: &str) -> bool {
        let mut g = match self.sessions.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let now = Instant::now();
        // Seed emit timer in the past so the very first add_bytes_sent call
        // is never dropped by the throttle.
        let emit_base = now
            .checked_sub(Duration::from_millis(200))
            .unwrap_or(now);
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
                bytes_total: 0,
                bytes_sent: 0,
                last_progress_emit: emit_base,
            });
        entry.last_active = now;
        is_new
    }

    /// Called when the receiver fetches the manifest. Registers the session
    /// (or updates an existing one) with the game's total byte count so the
    /// host UI can show a progress percentage from the very first request.
    /// Returns `true` if this is the first time we've seen this session_id
    /// (caller should emit `lan:uploads-changed`).
    fn register_manifest(
        &self,
        session_id: &str,
        game_id: &str,
        game_name: &str,
        peer_addr: &str,
        bytes_total: u64,
    ) -> bool {
        let mut g = match self.sessions.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let now = Instant::now();
        let emit_base = now
            .checked_sub(Duration::from_millis(200))
            .unwrap_or(now);
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
                bytes_total: 0,
                bytes_sent: 0,
                last_progress_emit: emit_base,
            });
        entry.last_active = now;
        entry.bytes_total = bytes_total;
        is_new
    }

    /// Credits `bytes` to the session's `bytes_sent` counter. Returns
    /// `true` when enough time has passed since the last progress emit
    /// to warrant a new `lan:uploads-changed` event (throttled to ~5 Hz).
    fn add_bytes_sent(&self, session_id: &str, bytes: u64) -> bool {
        let mut g = match self.sessions.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match g.get_mut(session_id) {
            Some(s) => {
                s.bytes_sent = s.bytes_sent.saturating_add(bytes);
                s.last_active = Instant::now();
                let should_emit = s.last_progress_emit.elapsed() >= Duration::from_millis(200);
                if should_emit {
                    s.last_progress_emit = Instant::now();
                }
                should_emit
            }
            None => false,
        }
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

/// Subset of `GameEntry` we share over the wire. Excludes local filesystem
/// paths (`exe_path`, `game_folder_path`, image paths) — those are meaningless
/// to a peer and would leak local layout. The fields we keep are the ones a
/// browsing UI on the other side needs to display the game and decide whether
/// they want it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PeerGame {
    pub id: String,
    pub catalog_number: u32,
    pub game_name: String,
    pub developer: String,
    pub publisher: String,
    pub genres: Vec<String>,
    pub install_size_mb: f64,
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
        // folder on disk to stream from. `lan_shared` defaults to `true`
        // for games added through the Add flow (which also auto-detects
        // game_folder_path); the user can flip it off in the Edit dialog's
        // Sharing tab.
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
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
    pub manifest_install_dir: Option<String>,
    pub save_paths: Vec<String>,
    pub developer: String,
    pub publisher: String,
    pub genres: Vec<String>,
    pub release_date: Option<DateTime<Utc>>,
}

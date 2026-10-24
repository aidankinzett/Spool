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
pub(crate) mod manifest_cache;
pub(crate) mod pairing;
pub(crate) mod server;

pub use discovery::{spawn_discovery, LanState};
pub use install::LanDownloadState;
pub use server::{spawn_manifest_prep, spawn_manifest_warm, LanManifests, LanServerShutdown};

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
    /// True when a peer is actively downloading `game_id` from this host.
    /// Sessions are reaped shortly after going idle, so presence ≈ in flight.
    /// Used by the move-install flow to refuse relocating files mid-transfer.
    pub fn has_active_upload(&self, game_id: &str) -> bool {
        self.sessions
            .lock()
            .map(|g| g.values().any(|s| s.game_id == game_id && !s.cancelled))
            .unwrap_or(false)
    }

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
        let emit_base = now.checked_sub(Duration::from_millis(200)).unwrap_or(now);
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
        let emit_base = now.checked_sub(Duration::from_millis(200)).unwrap_or(now);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> LanUploadsState {
        LanUploadsState::default()
    }

    #[test]
    fn touch_reports_new_session_only_once() {
        let s = state();
        assert!(s.touch("s1", "g1", "Game", "10.0.0.2"));
        assert!(!s.touch("s1", "g1", "Game", "10.0.0.2"));
        assert!(s.touch("s2", "g1", "Game", "10.0.0.3"));
        assert_eq!(s.snapshot().len(), 2);
    }

    #[test]
    fn register_manifest_sets_total_on_existing_session() {
        let s = state();
        assert!(s.touch("s1", "g1", "Game", "10.0.0.2"));
        // The session already exists, so this is not "new", but the total is applied.
        assert!(!s.register_manifest("s1", "g1", "Game", "10.0.0.2", 500));
        let snap = s.snapshot();
        assert_eq!(snap[0].bytes_total, 500);
    }

    #[test]
    fn register_manifest_creates_missing_session() {
        let s = state();
        assert!(s.register_manifest("s1", "g1", "Game", "10.0.0.2", 42));
        assert_eq!(s.snapshot()[0].bytes_total, 42);
    }

    #[test]
    fn first_progress_credit_is_never_throttled() {
        let s = state();
        s.register_manifest("s1", "g1", "Game", "p", 1000);
        assert!(s.add_bytes_sent("s1", 10));
        // Whether the second credit is throttled depends on elapsed time, so
        // only its byte count is asserted here.
        s.add_bytes_sent("s1", 10);
        assert_eq!(s.snapshot()[0].bytes_sent, 20);
    }

    #[test]
    fn add_bytes_for_unknown_session_is_ignored() {
        let s = state();
        assert!(!s.add_bytes_sent("nope", 10));
        assert!(s.snapshot().is_empty());
    }

    #[test]
    fn snapshot_clamps_sent_to_total_once_total_is_known() {
        let s = state();
        s.register_manifest("s1", "g1", "Game", "p", 100);
        s.add_bytes_sent("s1", 250);
        let snap = s.snapshot();
        assert_eq!(snap[0].bytes_sent, 100);
    }

    #[test]
    fn snapshot_does_not_clamp_before_manifest() {
        let s = state();
        s.touch("s1", "g1", "Game", "p");
        s.add_bytes_sent("s1", 250);
        assert_eq!(s.snapshot()[0].bytes_sent, 250);
    }

    #[test]
    fn bytes_sent_saturates_instead_of_overflowing() {
        let s = state();
        s.touch("s1", "g1", "Game", "p");
        s.add_bytes_sent("s1", u64::MAX);
        s.add_bytes_sent("s1", 5);
        assert_eq!(s.snapshot()[0].bytes_sent, u64::MAX);
    }

    #[test]
    fn cancel_flags_only_the_named_session() {
        let s = state();
        s.touch("s1", "g1", "Game", "p");
        s.touch("s2", "g1", "Game", "p");
        assert!(s.mark_cancelled("s1"));
        assert!(!s.mark_cancelled("missing"));
        assert!(s.is_cancelled("s1"));
        assert!(!s.is_cancelled("s2"));
        assert!(!s.is_cancelled("missing"));
    }

    #[test]
    fn has_active_upload_ignores_cancelled_and_other_games() {
        let s = state();
        assert!(!s.has_active_upload("g1"));
        s.touch("s1", "g1", "Game", "p");
        assert!(s.has_active_upload("g1"));
        assert!(!s.has_active_upload("g2"));
        s.mark_cancelled("s1");
        assert!(!s.has_active_upload("g1"));
    }

    fn entry(folder: Option<String>, shared: bool) -> GameEntry {
        GameEntry {
            id: "id".into(),
            game_name: "Game".into(),
            game_folder_path: folder,
            lan_shared: shared,
            exe_path: "/secret/local/game.exe".into(),
            ..Default::default()
        }
    }

    #[test]
    fn peer_game_is_shareable_only_when_opted_in_with_real_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        assert!(PeerGame::from_entry(&entry(Some(path.clone()), true)).shareable);
        assert!(!PeerGame::from_entry(&entry(Some(path), false)).shareable);
        assert!(!PeerGame::from_entry(&entry(None, true)).shareable);
        assert!(!PeerGame::from_entry(&entry(Some(String::new()), true)).shareable);
        let missing = dir.path().join("gone").to_string_lossy().to_string();
        assert!(!PeerGame::from_entry(&entry(Some(missing), true)).shareable);
    }

    #[test]
    fn peer_game_is_not_shareable_when_folder_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, b"x").unwrap();
        let g = entry(Some(file.to_string_lossy().to_string()), true);
        assert!(!PeerGame::from_entry(&g).shareable);
    }

    #[test]
    fn peer_game_does_not_serialise_local_paths() {
        let dir = tempfile::tempdir().unwrap();
        let g = entry(Some(dir.path().to_string_lossy().to_string()), true);
        let json = serde_json::to_string(&PeerGame::from_entry(&g)).unwrap();
        assert!(!json.contains("/secret/local"));
        assert!(!json.contains("exe_path"));
        assert!(!json.contains("game_folder_path"));
    }

    #[test]
    fn peer_file_from_older_peer_without_hash_or_mtime_parses() {
        let f: PeerFile = serde_json::from_str(r#"{"path":"a/b.bin","size":7}"#).unwrap();
        assert_eq!(f.path, "a/b.bin");
        assert_eq!(f.size, 7);
        assert!(f.hash.is_empty());
        assert_eq!(f.mtime_unix_ms, 0);
    }

    #[test]
    fn peer_manifest_tolerates_missing_and_unknown_fields() {
        let m: PeerGameManifest =
            serde_json::from_str(r#"{"game_id":"g","total_bytes":3,"future_field":true}"#).unwrap();
        assert_eq!(m.game_id, "g");
        assert_eq!(m.total_bytes, 3);
        assert!(m.files.is_empty());
    }
}

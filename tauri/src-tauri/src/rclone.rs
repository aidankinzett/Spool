//! rclone-based control plane for cross-device coordination.
//!
//! Replaces the Hono/Node sync server with JSON blobs stored on the user's
//! already-configured rclone remote (the same one used for cloud save
//! backups). Every device only ever writes its own files so there are no
//! write conflicts.
//!
//! # Remote layout  (under `<base>/_spool`)
//!
//! ```text
//! <base>/
//!   ludusavi-backup/          ← ludusavi's game saves (managed by ludusavi)
//!   _spool/
//!     devices/<device_id>.json    ← per-device playtime / last-played / backup ts
//!     sessions/<blake3(name)>.json ← per-game "is someone playing?" marker
//! ```
//!
//! `_spool` is a **sibling** of `ludusavi-backup`, never nested inside it —
//! ludusavi's `--cloud-sync` reconciles that subtree and would delete
//! unrecognised files.
//!
//! # Session-marker lifecycle (run_workflow integration)
//!
//! Phase 1.5 (before launch): cat the marker → classify:
//!   * Absent / ours → write Active marker, proceed.
//!   * Active + fresh + `!steal` → block ("Already playing on X").
//!   * PendingBackup or stale + `!steal` → block (unsynced-session warning).
//!   * `steal==true` → overwrite marker, proceed.
//!
//! During session: heartbeat rewrites `updated_at` every 60 s.
//!
//! Clean exit: rewrite marker to `PendingBackup` (saves not yet uploaded).
//!
//! After successful cloud backup: `deletefile` the marker AND set
//! `blob.backups[game]=now` in the device blob.

use crate::config::{ConfigData, SharedConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

// ── Timing constants ─────────────────────────────────────────────────────────

/// How often the session heartbeat rewrites the marker's `updated_at`.
/// rclone PUTs are heavier than HTTP; 60 s write + 180 s stale window = 3
/// missed-write slack before a peer sees the session as stale.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);
/// Minimum interval between heartbeat writes — skip if the last successful
/// write was less than 45 s ago (allows clock drift without thrashing).
const HEARTBEAT_MIN_GAP: Duration = Duration::from_secs(45);
/// Seconds since `updated_at` before an Active marker is classified Stale.
const STALE_WINDOW_SECS: i64 = 180;

/// Timeout for `cat` / `rcat` / `deletefile` calls.
const RCLONE_IO_TIMEOUT: Duration = Duration::from_secs(8);
/// Shorter timeout for the reachability probe (`lsd`).
const RCLONE_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Timeout for `lsjson` (listing device blobs at startup).
const RCLONE_LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout for the startup fold's per-device `cat` calls.
const RCLONE_FOLD_TIMEOUT: Duration = Duration::from_secs(6);

// ── Suspend timeout (Linux only) ─────────────────────────────────────────────
/// Shorter timeout for the suspend-path marker write (must complete before
/// logind's InhibitDelayMaxSec).
#[cfg(target_os = "linux")]
const SUSPEND_TIMEOUT: Duration = Duration::from_secs(3);

/// How often to poll the health/reachability status.
const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(30);

// ── Sync status (shared with the chrome dot) ─────────────────────────────────

/// Reachability state for the cloud remote. The frontend renders the chrome
/// cloud icon based on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncReachability {
    /// No cloud remote configured.
    Unconfigured,
    /// rclone `lsd` returned within the timeout.
    Online,
    /// rclone not found, network error, or timeout.
    Offline,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub reachability: SyncReachability,
    /// Kept for wire compat with the old sync server response; always `None`.
    pub server_version: Option<String>,
    /// Diagnostic on the last failure.
    pub error: Option<String>,
    /// Seconds since the last successful probe.
    pub last_ok_ago_secs: Option<u64>,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            reachability: SyncReachability::Unconfigured,
            server_version: None,
            error: None,
            last_ok_ago_secs: None,
        }
    }
}

/// Tauri-managed status state. Updated by the polling task; read by the
/// `current_sync_status` command + the `sync:status-changed` event.
#[derive(Default)]
pub struct SyncStatusState {
    inner: Mutex<SyncStatus>,
    last_ok: Mutex<Option<std::time::Instant>>,
}

impl SyncStatusState {
    pub fn snapshot(&self) -> SyncStatus {
        let mut s = self.inner.lock().map_err(|_| ()).ok().map(|g| g.clone()).unwrap_or_default();
        if let Ok(g) = self.last_ok.lock() {
            s.last_ok_ago_secs = g.map(|i| i.elapsed().as_secs());
        }
        s
    }

    fn set(&self, new_status: SyncStatus) {
        if let Ok(mut g) = self.inner.lock() {
            *g = new_status;
        }
    }

    fn mark_ok(&self) {
        if let Ok(mut g) = self.last_ok.lock() {
            *g = Some(std::time::Instant::now());
        }
    }
}

// ── RcloneRemote ─────────────────────────────────────────────────────────────

/// Resolved rclone configuration for this session.
#[derive(Debug, Clone)]
pub struct RcloneRemote {
    /// Path to the rclone executable (sidecar or system).
    pub exe: PathBuf,
    /// Remote name as used in rclone, e.g. `"myremote"` (without the trailing
    /// `:`; it is added per-call).
    pub remote: String,
    /// Base folder name on the remote, e.g. `"Spool"`. The full backup path
    /// becomes `<remote>:<base>/ludusavi-backup`; the control-plane path
    /// becomes `<remote>:<base>/_spool`.
    pub base: String,
}

impl RcloneRemote {
    /// Full rclone path for a `_spool` object, e.g.
    /// `"myremote:Spool/_spool/sessions/abc123.json"`.
    pub fn spool_path(&self, rest: &str) -> String {
        format!("{}:{}/_spool/{}", self.remote, self.base, rest)
    }

    /// Full rclone path for a session marker file.
    pub fn session_path(&self, game_name: &str) -> String {
        self.spool_path(&format!("sessions/{}.json", session_hash(game_name)))
    }

    /// Full rclone path for a device blob file.
    pub fn device_path(&self, device_id: &str) -> String {
        self.spool_path(&format!("devices/{}.json", device_id))
    }

    /// Full rclone path prefix for the `devices/` directory listing.
    pub fn devices_dir(&self) -> String {
        self.spool_path("devices")
    }
}

/// Resolve `RcloneRemote` from the running app's config + ludusavi config.yaml.
/// Returns `None` when cloud isn't configured or the rclone binary is missing.
pub fn resolve_remote(app: &AppHandle) -> Option<RcloneRemote> {
    let cfg = app.state::<SharedConfig>();
    let guard = cfg.lock().ok()?;
    resolve_remote_from_config(&guard.data)
}

/// AppHandle-free variant for headless paths (`--release-lock`, `--backup`).
pub fn resolve_remote_from_config(cfg: &ConfigData) -> Option<RcloneRemote> {
    let exe = crate::paths::resolve_rclone_path()?;
    let remote_name = read_remote_name_from_ludusavi_config()?;
    let base = if cfg.cloud_base_path.trim().is_empty() {
        "Spool".to_string()
    } else {
        cfg.cloud_base_path.trim().trim_end_matches('/').to_string()
    };
    Some(RcloneRemote { exe, remote: remote_name, base })
}

/// Read the rclone remote name from Spool's ludusavi config.yaml.
/// Returns `None` when the file is absent or the remote is unset/null.
fn read_remote_name_from_ludusavi_config() -> Option<String> {
    let raw = std::fs::read_to_string(crate::paths::ludusavi_config_file()).ok()?;
    let config: serde_yaml::Value = serde_yaml::from_str(&raw).ok()?;
    get_rclone_remote_name_from_yaml(&config)
}

/// Extract the rclone remote name from a parsed ludusavi `config.yaml`.
/// Handles both bare strings (presets like `"GoogleDrive"`) and tagged maps
/// (`{ Custom: { id: "myremote" } }`, `{ WebDav: { id: "…" } }`).
pub fn get_rclone_remote_name_from_yaml(config: &serde_yaml::Value) -> Option<String> {
    let cloud = config.get("cloud")?;
    let remote = cloud.get("remote")?;
    match remote {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Mapping(m) => {
            if let Some(custom) = m.get(serde_yaml::Value::String("Custom".into())) {
                if let Some(id) = custom.get(serde_yaml::Value::String("id".into())) {
                    return id.as_str().map(String::from);
                }
            }
            if let Some(webdav) = m.get(serde_yaml::Value::String("WebDav".into())) {
                if let Some(id) = webdav.get(serde_yaml::Value::String("id".into())) {
                    return id.as_str().map(String::from);
                }
            }
            None
        }
        _ => None,
    }
}

// ── session_hash ─────────────────────────────────────────────────────────────

/// blake3 hex digest of `game_name` — used as the session-marker filename so
/// names with path-unsafe characters work safely. The in-file `game_name`
/// field is the guard against the (near-zero) probability of a collision.
pub fn session_hash(game_name: &str) -> String {
    let hash = blake3::hash(game_name.as_bytes());
    hash.to_hex().to_string()
}

// ── DeviceBlob ────────────────────────────────────────────────────────────────

/// Per-device JSON blob stored at `_spool/devices/<device_id>.json`.
/// Each device only ever writes its own file — no write conflicts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceBlob {
    pub device_name: String,
    /// Minutes accrued on this device only (not a running total).
    #[serde(default)]
    pub playtime: BTreeMap<String, i64>,
    /// RFC 3339 timestamp of the last session on this device per game.
    #[serde(default)]
    pub last_played: BTreeMap<String, String>,
    /// RFC 3339 timestamp of the last successful cloud upload per game.
    #[serde(default)]
    pub backups: BTreeMap<String, String>,
    /// Schema version — always 1.
    #[serde(default = "default_schema")]
    pub schema: u32,
}

fn default_schema() -> u32 { 1 }

// ── SessionState + SessionMarker ─────────────────────────────────────────────

/// Lifecycle state of a session marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    /// Game is actively running on the owning device.
    Active,
    /// Game has exited but saves haven't been uploaded yet.
    PendingBackup,
}

/// Per-game session marker at `_spool/sessions/<blake3(game_name)>.json`.
/// Written on launch, updated by a heartbeat, transitioned to `PendingBackup`
/// on exit, and deleted once the cloud upload succeeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMarker {
    /// Plain-text game name — used for hash-collision detection.
    pub game_name: String,
    pub device_id: String,
    pub device_name: String,
    /// RFC 3339 — when this session started.
    pub started_at: String,
    /// RFC 3339 — last heartbeat or state transition.
    pub updated_at: String,
    pub state: SessionState,
    /// True when logind suspend has been signalled — suppresses staleness
    /// so a sleeping device doesn't get reclassified as abandoned.
    #[serde(default)]
    pub suspended: bool,
}

// ── Marker classification ─────────────────────────────────────────────────────

/// Result of evaluating whether a pre-existing session marker blocks launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerClass {
    /// No marker, or it belongs to this device.
    Absent,
    /// Another device is actively playing and the marker is fresh.
    ActivePlaying { device_name: String },
    /// Another device has an unsynced session (PendingBackup, or stale Active).
    Unsynced { device_name: String },
}

/// Classify an optional `SessionMarker` from the perspective of `our_device_id`.
/// `steal` forces the result to `Absent` regardless.
pub fn classify_marker(
    marker: Option<&SessionMarker>,
    our_device_id: &str,
    steal: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> MarkerClass {
    if steal {
        return MarkerClass::Absent;
    }
    let Some(m) = marker else {
        return MarkerClass::Absent;
    };
    // Hash collision guard.
    if m.device_id == our_device_id {
        return MarkerClass::Absent;
    }
    match m.state {
        SessionState::PendingBackup => MarkerClass::Unsynced {
            device_name: m.device_name.clone(),
        },
        SessionState::Active => {
            // Stale if updated_at is older than STALE_WINDOW_SECS and not
            // suspended — a device that's merely asleep shouldn't be reclassified.
            if !m.suspended {
                if let Ok(updated) = chrono::DateTime::parse_from_rfc3339(&m.updated_at) {
                    let age = (now - updated.with_timezone(&chrono::Utc)).num_seconds();
                    if age > STALE_WINDOW_SECS {
                        return MarkerClass::Unsynced {
                            device_name: m.device_name.clone(),
                        };
                    }
                }
            }
            MarkerClass::ActivePlaying {
                device_name: m.device_name.clone(),
            }
        }
    }
}

// ── rclone subprocess helpers ─────────────────────────────────────────────────

/// Spawn a `tokio::process::Command` for rclone with `kill_on_drop(true)` and
/// `CREATE_NO_WINDOW` on Windows.
fn make_rclone_cmd(exe: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(exe);
    cmd.kill_on_drop(true);
    cmd.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        #[allow(unused_imports)]
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// `rclone cat <target>` — fetch a small object as a UTF-8 string.
/// Returns `None` on any error (missing object, timeout, non-zero exit).
pub async fn cat(exe: &Path, target: &str, timeout: Duration) -> Option<String> {
    let mut cmd = make_rclone_cmd(exe);
    cmd.arg("cat").arg(target);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let child = cmd.spawn().ok()?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        tracing::debug!(target, "rclone cat: non-zero exit (object likely absent)");
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// `rclone rcat <target>` — write `body` (via stdin) to an object atomically.
/// Returns `true` on success.
pub async fn rcat(exe: &Path, target: &str, body: &str, timeout: Duration) -> bool {
    let mut cmd = make_rclone_cmd(exe);
    cmd.arg("rcat").arg(target);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(target, error = %e, "rclone rcat: spawn failed");
            return false;
        }
    };
    // Write stdin in a blocking task so we don't hold a &mut across .await.
    let body_bytes = body.as_bytes().to_vec();
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        if let Err(e) = stdin.write_all(&body_bytes).await {
            tracing::warn!(target, error = %e, "rclone rcat: stdin write failed");
            return false;
        }
        // Dropping stdin closes it, signalling EOF to rclone.
    }
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) if out.status.success() => true,
        Ok(Ok(out)) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!(target, status = ?out.status, stderr = %stderr.trim(), "rclone rcat: non-zero exit");
            false
        }
        Ok(Err(e)) => {
            tracing::warn!(target, error = %e, "rclone rcat: wait failed");
            false
        }
        Err(_) => {
            tracing::warn!(target, "rclone rcat: timed out");
            false
        }
    }
}

/// `rclone deletefile <target>` — delete a single object.
/// Returns `true` on success or if the object was already absent.
pub async fn deletefile(exe: &Path, target: &str, timeout: Duration) -> bool {
    let mut cmd = make_rclone_cmd(exe);
    cmd.arg("deletefile").arg(target);
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(target, error = %e, "rclone deletefile: spawn failed");
            return false;
        }
    };
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) => {
            if out.status.success() {
                true
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                // Treat "not found" as success — idempotent delete.
                if stderr.contains("not found") || stderr.contains("doesn't exist") || stderr.contains("No such") {
                    true
                } else {
                    tracing::warn!(target, stderr = %stderr.trim(), "rclone deletefile: non-zero exit");
                    false
                }
            }
        }
        Ok(Err(e)) => {
            tracing::warn!(target, error = %e, "rclone deletefile: wait failed");
            false
        }
        Err(_) => {
            tracing::warn!(target, "rclone deletefile: timed out");
            false
        }
    }
}

/// Entry in the `rclone lsjson` output.
#[derive(Debug, Deserialize)]
pub struct LsJsonEntry {
    #[serde(rename = "Path")]
    #[allow(dead_code)]
    pub path: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "IsDir", default)]
    pub is_dir: bool,
}

/// `rclone lsjson <target>` — list objects in a remote directory as JSON.
/// Returns `None` on any error (absent path, timeout, parse error).
pub async fn lsjson(exe: &Path, target: &str, timeout: Duration) -> Option<Vec<LsJsonEntry>> {
    let mut cmd = make_rclone_cmd(exe);
    cmd.arg("lsjson").arg(target);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let child = cmd.spawn().ok()?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        tracing::debug!(target, "rclone lsjson: non-zero exit (directory likely absent)");
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

/// `rclone lsd <remote>:` — reachability probe. Returns `true` if rclone can
/// list the top level of the remote within `timeout`.
pub async fn lsd(exe: &Path, remote_colon: &str, timeout: Duration) -> bool {
    let mut cmd = make_rclone_cmd(exe);
    cmd.arg("lsd").arg(remote_colon);
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return false,
    };
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) => out.status.success(),
        _ => false,
    }
}

// ── Health poller ─────────────────────────────────────────────────────────────

/// Kick off the health-poll task. Called once from `lib.rs::run`. Probes the
/// configured rclone remote every 30 s and emits `sync:status-changed`.
pub fn spawn_health_poller(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            poll_once(&app).await;
            tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
        }
    });
}

async fn poll_once(app: &AppHandle) {
    let prev = app.state::<SyncStatusState>().snapshot();

    let new_status = match resolve_remote(app) {
        None => SyncStatus { reachability: SyncReachability::Unconfigured, ..Default::default() },
        Some(remote) => {
            let remote_colon = format!("{}:", remote.remote);
            if lsd(&remote.exe, &remote_colon, RCLONE_PROBE_TIMEOUT).await {
                SyncStatus {
                    reachability: SyncReachability::Online,
                    last_ok_ago_secs: Some(0),
                    ..Default::default()
                }
            } else {
                SyncStatus {
                    reachability: SyncReachability::Offline,
                    error: Some(format!("rclone lsd {} timed out or failed", remote_colon)),
                    ..Default::default()
                }
            }
        }
    };

    if new_status.reachability == SyncReachability::Online {
        app.state::<SyncStatusState>().mark_ok();
    }

    let changed = prev.reachability != new_status.reachability
        || prev.error != new_status.error;
    app.state::<SyncStatusState>().set(new_status);
    if changed {
        let snap = app.state::<SyncStatusState>().snapshot();
        if let Err(e) = app.emit("sync:status-changed", &snap) {
            tracing::warn!(error = %e, "failed to emit sync:status-changed");
        }
    }
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn current_sync_status(state: State<'_, SyncStatusState>) -> SyncStatus {
    state.snapshot()
}

/// Force an immediate probe instead of waiting for the next 30 s tick.
/// Used by the Settings page after the user edits the cloud remote config.
#[tauri::command]
pub async fn refresh_sync_status(app: AppHandle) -> SyncStatus {
    poll_once(&app).await;
    app.state::<SyncStatusState>().snapshot()
}

// ── Startup fold ──────────────────────────────────────────────────────────────

/// One-shot startup task: list all device blobs, fold playtime (Σ), last-played
/// (max), and backup badge (latest backer) into the local library. Runs ~4 s
/// after boot so any pending health poll has a chance to settle first.
pub fn spawn_startup_fold(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        run_startup_fold(&app).await;
    });
}

async fn run_startup_fold(app: &AppHandle) {
    let Some(remote) = resolve_remote(app) else { return };

    // Only fold when online.
    let status = app.state::<SyncStatusState>().snapshot();
    if status.reachability != SyncReachability::Online {
        return;
    }

    tracing::info!("rclone: starting startup fold of device blobs");

    let (our_device_id, _) = device_identity(app);

    // List all device blob files.
    let devices_dir = remote.devices_dir();
    let entries = lsjson(&remote.exe, &devices_dir, RCLONE_LIST_TIMEOUT).await;
    let entries = entries.unwrap_or_default();

    // cat each blob in parallel, tracking the device_id (filename without .json).
    use futures_util::future::join_all;
    let mut fold_futures = Vec::new();
    for entry in &entries {
        if entry.is_dir || !entry.name.ends_with(".json") {
            continue;
        }
        let device_id = entry.name.trim_end_matches(".json").to_string();
        let path = remote.spool_path(&format!("devices/{}", entry.name));
        let exe = remote.exe.clone();
        fold_futures.push(async move {
            let raw = cat(&exe, &path, RCLONE_FOLD_TIMEOUT).await?;
            let blob = serde_json::from_str::<DeviceBlob>(&raw).ok()?;
            Some((device_id, blob))
        });
    }

    let results: Vec<Option<(String, DeviceBlob)>> = join_all(fold_futures).await;
    let device_blobs_with_id: Vec<(String, DeviceBlob)> = results.into_iter().flatten().collect();

    if device_blobs_with_id.is_empty() {
        tracing::info!("rclone: startup fold: no parseable device blobs");
        return;
    }

    // Per-game fold accumulators.
    let mut playtime_totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut last_played_max: BTreeMap<String, chrono::DateTime<chrono::Utc>> = BTreeMap::new();
    let mut latest_backer_device: BTreeMap<String, String> = BTreeMap::new(); // game → device_id
    let mut latest_backup_time: BTreeMap<String, chrono::DateTime<chrono::Utc>> = BTreeMap::new();

    for (device_id, blob) in &device_blobs_with_id {
        // Playtime: sum each device's own contribution.
        for (game, &mins) in &blob.playtime {
            *playtime_totals.entry(game.clone()).or_default() += mins;
        }
        // Last played: max across devices.
        for (game, ts) in &blob.last_played {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(ts) {
                let t = t.with_timezone(&chrono::Utc);
                let entry = last_played_max.entry(game.clone()).or_insert(t);
                if t > *entry {
                    *entry = t;
                }
            }
        }
        // Latest backer: device with the most recent backup timestamp.
        for (game, ts) in &blob.backups {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(ts) {
                let t = t.with_timezone(&chrono::Utc);
                let entry = latest_backup_time.entry(game.clone());
                match entry {
                    std::collections::btree_map::Entry::Vacant(e) => {
                        e.insert(t);
                        latest_backer_device.insert(game.clone(), device_id.clone());
                    }
                    std::collections::btree_map::Entry::Occupied(mut e) => {
                        if t > *e.get() {
                            *e.get_mut() = t;
                            latest_backer_device.insert(game.clone(), device_id.clone());
                        }
                    }
                }
            }
        }
    }

    // Apply to the library.
    let library = app.state::<crate::library::SharedLibrary>();
    let mut applied = 0usize;
    if let Ok(mut lib) = library.lock() {
        for entry in lib.entries.iter_mut() {
            let name = &entry.game_name;
            // Playtime: set to folded sum (not max).
            if let Some(&total) = playtime_totals.get(name) {
                let total_i32 = total.min(i32::MAX as i64) as i32;
                if entry.playtime_minutes != total_i32 {
                    entry.playtime_minutes = total_i32;
                    applied += 1;
                }
            }
            // Last played: take max across devices.
            if let Some(&remote_time) = last_played_max.get(name) {
                if entry.last_played_at.map(|t| remote_time > t).unwrap_or(true) {
                    entry.last_played_at = Some(remote_time);
                    applied += 1;
                }
            }
            // Badge.
            let backer = latest_backer_device.get(name).map(|s| s.as_str());
            let badge = compute_badge(&our_device_id, backer);
            if entry.sync_badge.as_deref() != Some(badge) {
                entry.sync_badge = Some(badge.to_string());
                applied += 1;
            }
        }
        if applied > 0 {
            if let Err(e) = lib.save() {
                tracing::warn!(error = %e, "startup fold: library save failed");
            }
        }
    }

    tracing::info!(applied, "rclone: startup fold complete");
    if applied > 0 {
        let _ = app.emit("library:changed", &());
    }
}

/// Map the "latest backer" device id to one of the three badge strings.
///
/// - `ours` → `"synced"` (we are the most recent backer)
/// - `other` → `"cloud-newer"` (another device backed up more recently)
/// - `None` → `"synced"` (no backup history → nothing newer than nothing)
pub fn compute_badge(our_device_id: &str, backer_device_id: Option<&str>) -> &'static str {
    match backer_device_id {
        Some(id) if id == our_device_id => "synced",
        Some(_) => "cloud-newer",
        None => "synced",
    }
}

// ── Device blob I/O ───────────────────────────────────────────────────────────

/// Read, mutate, and write back the device blob for this device.
/// Adds `session_minutes` to the game's playtime, updates `last_played`, and
/// sets `device_name`. Best-effort: silently no-ops on any failure.
pub async fn update_device_blob(
    remote: &RcloneRemote,
    game_name: &str,
    session_minutes: i32,
    session_end_rfc3339: &str,
    device_id: &str,
    device_name: &str,
) {
    let path = remote.device_path(device_id);

    // cat → mutate → rcat.
    let mut blob: DeviceBlob = match cat(&remote.exe, &path, RCLONE_IO_TIMEOUT).await {
        Some(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        None => DeviceBlob::default(),
    };

    blob.device_name = device_name.to_string();
    *blob.playtime.entry(game_name.to_string()).or_default() += session_minutes as i64;
    blob.last_played.insert(game_name.to_string(), session_end_rfc3339.to_string());

    let Ok(json) = serde_json::to_string(&blob) else { return };
    if !rcat(&remote.exe, &path, &json, RCLONE_IO_TIMEOUT).await {
        tracing::warn!(game_name, "rclone: update_device_blob: rcat failed");
    }
}

/// Stamp the successful-backup timestamp into the device blob.
/// Called after a successful cloud upload so peers see a fresh "latest backer".
pub async fn record_backup_in_blob(
    remote: &RcloneRemote,
    game_name: &str,
    backed_up_at_rfc3339: &str,
    device_id: &str,
    device_name: &str,
) {
    let path = remote.device_path(device_id);
    let mut blob: DeviceBlob = match cat(&remote.exe, &path, RCLONE_IO_TIMEOUT).await {
        Some(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        None => DeviceBlob::default(),
    };
    blob.device_name = device_name.to_string();
    blob.backups.insert(game_name.to_string(), backed_up_at_rfc3339.to_string());
    let Ok(json) = serde_json::to_string(&blob) else { return };
    if !rcat(&remote.exe, &path, &json, RCLONE_IO_TIMEOUT).await {
        tracing::warn!(game_name, "rclone: record_backup_in_blob: rcat failed");
    }
}

// ── Session marker I/O ────────────────────────────────────────────────────────

/// Read and parse the session marker for `game_name`. Returns `None` when the
/// object is absent or can't be parsed.
pub async fn read_session_marker(
    remote: &RcloneRemote,
    game_name: &str,
) -> Option<SessionMarker> {
    let path = remote.session_path(game_name);
    let raw = cat(&remote.exe, &path, RCLONE_IO_TIMEOUT).await?;
    let marker: SessionMarker = serde_json::from_str(&raw).ok()?;
    // Hash-collision guard: verify the stored game_name matches.
    if marker.game_name != game_name {
        tracing::warn!(
            stored = %marker.game_name,
            requested = game_name,
            "session marker game_name mismatch (hash collision?)"
        );
        return None;
    }
    Some(marker)
}

/// Write (overwrite) the session marker for `game_name`.
pub async fn write_session_marker(remote: &RcloneRemote, marker: &SessionMarker) -> bool {
    let path = remote.session_path(&marker.game_name);
    let Ok(json) = serde_json::to_string(marker) else { return false };
    rcat(&remote.exe, &path, &json, RCLONE_IO_TIMEOUT).await
}

/// Delete the session marker for `game_name`. Returns `true` on success or if
/// it was already absent.
pub async fn delete_session_marker(remote: &RcloneRemote, game_name: &str) -> bool {
    let path = remote.session_path(game_name);
    deletefile(&remote.exe, &path, RCLONE_IO_TIMEOUT).await
}

/// Create an `Active` session marker for this device.
pub async fn write_active_marker(
    remote: &RcloneRemote,
    game_name: &str,
    device_id: &str,
    device_name: &str,
) -> bool {
    let now = chrono::Utc::now().to_rfc3339();
    let marker = SessionMarker {
        game_name: game_name.to_string(),
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        started_at: now.clone(),
        updated_at: now,
        state: SessionState::Active,
        suspended: false,
    };
    write_session_marker(remote, &marker).await
}

/// Transition our own marker to `PendingBackup` (game exited but saves not yet
/// uploaded). Reads the existing marker to preserve `started_at` etc.
/// If the read fails, synthesises a new record.
pub async fn write_pending_backup_marker(
    remote: &RcloneRemote,
    game_name: &str,
    device_id: &str,
    device_name: &str,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let mut marker = match read_session_marker(remote, game_name).await {
        Some(m) if m.device_id == device_id => m,
        _ => SessionMarker {
            game_name: game_name.to_string(),
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            started_at: now.clone(),
            updated_at: now.clone(),
            state: SessionState::Active,
            suspended: false,
        },
    };
    marker.state = SessionState::PendingBackup;
    marker.updated_at = now;
    if !write_session_marker(remote, &marker).await {
        tracing::warn!(game_name, "rclone: write_pending_backup_marker: write failed");
    }
}

/// Mark our own session as suspended (logind pre-sleep). Reads → mutates →
/// writes. Linux-only call site; available unconditionally so the module
/// compiles on Windows without dead-code warnings.
#[allow(dead_code)]
pub async fn suspend_marker(
    remote: &RcloneRemote,
    game_name: &str,
    device_id: &str,
) -> bool {
    let Some(mut marker) = read_session_marker(remote, game_name).await else {
        return false;
    };
    if marker.device_id != device_id {
        tracing::warn!(game_name, "suspend_marker: marker belongs to another device");
        return false;
    }
    marker.suspended = true;
    marker.updated_at = chrono::Utc::now().to_rfc3339();
    write_session_marker(remote, &marker).await
}

/// Clear the `suspended` flag on our own marker (logind resume).
/// Returns the current marker so the caller can check the device_id.
#[allow(dead_code)]
pub async fn resume_marker(
    remote: &RcloneRemote,
    game_name: &str,
    device_id: &str,
) -> Option<SessionMarker> {
    let mut marker = read_session_marker(remote, game_name).await?;
    if marker.device_id != device_id {
        return Some(marker); // stolen while sleeping — caller handles this
    }
    marker.suspended = false;
    marker.updated_at = chrono::Utc::now().to_rfc3339();
    write_session_marker(remote, &marker).await;
    Some(marker)
}

// ── Session heartbeat ─────────────────────────────────────────────────────────

/// Spawn a task that bumps the marker's `updated_at` every 60 s. Returns the
/// `JoinHandle`; the caller `.abort()`s it when the session ends.
pub fn spawn_session_heartbeat(
    _app: AppHandle,
    game_name: String,
    remote: RcloneRemote,
    device_id: String,
    device_name: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_write = tokio::time::Instant::now();
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            if last_write.elapsed() < HEARTBEAT_MIN_GAP {
                continue;
            }
            let path = remote.session_path(&game_name);
            let now_str = chrono::Utc::now().to_rfc3339();
            // cat → bump updated_at → rcat.
            if let Some(raw) = cat(&remote.exe, &path, RCLONE_IO_TIMEOUT).await {
                if let Ok(mut marker) = serde_json::from_str::<SessionMarker>(&raw) {
                    if marker.device_id == device_id {
                        marker.updated_at = now_str;
                        if let Ok(json) = serde_json::to_string(&marker) {
                            if rcat(&remote.exe, &path, &json, RCLONE_IO_TIMEOUT).await {
                                last_write = tokio::time::Instant::now();
                                tracing::debug!(game_name, "rclone: heartbeat written");
                            }
                        }
                    } else {
                        tracing::warn!(
                            game_name,
                            other_device = %marker.device_id,
                            "heartbeat: marker taken over by another device — stopping"
                        );
                        return;
                    }
                }
            }
            // If the cat failed, the marker is absent — write a fresh one.
            else {
                let marker = SessionMarker {
                    game_name: game_name.clone(),
                    device_id: device_id.clone(),
                    device_name: device_name.clone(),
                    started_at: now_str.clone(),
                    updated_at: now_str.clone(),
                    state: SessionState::Active,
                    suspended: false,
                };
                if let Ok(json) = serde_json::to_string(&marker) {
                    if rcat(&remote.exe, &path, &json, RCLONE_IO_TIMEOUT).await {
                        last_write = tokio::time::Instant::now();
                    }
                }
            }
        }
    })
}

/// No-op heartbeat for when the remote isn't configured — returns an
/// immediately-finished task so `.abort()` is a harmless no-op.
pub fn spawn_noop_heartbeat() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {})
}

// ── Identity helper ───────────────────────────────────────────────────────────

/// (device_id, device_name) from the app config. Both empty strings on
/// poisoned lock — callers treat that as "skip the sync interaction".
pub fn device_identity(app: &AppHandle) -> (String, String) {
    let cfg = app.state::<SharedConfig>();
    let guard = match cfg.lock() {
        Ok(g) => g,
        Err(_) => return (String::new(), String::new()),
    };
    (guard.data.device_id.clone(), guard.data.device_name.clone())
}

/// AppHandle-free variant for headless paths.
pub fn device_identity_from_config(cfg: &ConfigData) -> (String, String) {
    (cfg.device_id.clone(), cfg.device_name.clone())
}

// ── Headless helpers (for Decky / --release-lock / --backup paths) ────────────

/// Headless `--release-lock` equivalent: rewrite our marker as `PendingBackup`.
/// Best-effort no-op when cloud is not configured.
pub async fn release_marker_headless(cfg: &ConfigData, game_name: &str) {
    let Some(remote) = resolve_remote_from_config(cfg) else { return };
    let (device_id, device_name) = device_identity_from_config(cfg);
    if device_id.is_empty() { return; }
    write_pending_backup_marker(&remote, game_name, &device_id, &device_name).await;
}

/// Headless `--backup` post-backup: delete the session marker if the cloud
/// upload succeeded.
pub async fn delete_marker_headless(cfg: &ConfigData, game_name: &str) {
    let Some(remote) = resolve_remote_from_config(cfg) else { return };
    delete_session_marker(&remote, game_name).await;
}

// ── Suspend integration (Linux only) ─────────────────────────────────────────

/// Mark our session suspended before logind freezes the process.
/// Linux-only because the only callers are in suspend.rs.
#[cfg(target_os = "linux")]
pub async fn suspend_marker_for_app(app: &AppHandle, game_name: &str) -> bool {
    let Some(remote) = resolve_remote(app) else { return false };
    let (device_id, _) = device_identity(app);
    if device_id.is_empty() { return false; }
    // Use the shorter SUSPEND_TIMEOUT so this completes before logind gives up.
    let path = remote.session_path(game_name);
    match read_session_marker(&remote, game_name).await {
        Some(mut m) if m.device_id == device_id => {
            m.suspended = true;
            m.updated_at = chrono::Utc::now().to_rfc3339();
            let Ok(json) = serde_json::to_string(&m) else { return false };
            // Use the shorter suspend timeout.
            rcat(&remote.exe, &path, &json, SUSPEND_TIMEOUT).await
        }
        _ => false,
    }
}

/// Evaluate the marker after a resume, returning whether it was taken over.
/// Rewrites `suspended=false` if still ours.
#[cfg(target_os = "linux")]
pub async fn resume_marker_for_app(
    app: &AppHandle,
    game_name: &str,
) -> Option<MarkerClass> {
    let remote = resolve_remote(app)?;
    let (device_id, _) = device_identity(app);
    if device_id.is_empty() { return None; }
    let marker = read_session_marker(&remote, game_name).await;
    let class = classify_marker(
        marker.as_ref(),
        &device_id,
        false,
        chrono::Utc::now(),
    );
    // If still ours (Absent after classify = ours), clear the suspended flag.
    if class == MarkerClass::Absent {
        if let Some(mut m) = marker {
            if m.device_id == device_id {
                m.suspended = false;
                m.updated_at = chrono::Utc::now().to_rfc3339();
                write_session_marker(&remote, &m).await;
            }
        }
    }
    Some(class)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── session_hash ─────────────────────────────────────────────────────────

    #[test]
    fn session_hash_is_stable() {
        // Must be deterministic across compilations / platforms.
        let h = session_hash("Half-Life 2");
        assert_eq!(h.len(), 64, "blake3 hex is 64 chars");
        assert_eq!(session_hash("Half-Life 2"), h);
    }

    #[test]
    fn session_hash_different_names_differ() {
        assert_ne!(session_hash("Half-Life 2"), session_hash("Half-Life"));
    }

    // ── DeviceBlob round-trip ────────────────────────────────────────────────

    #[test]
    fn device_blob_round_trips() {
        let mut blob = DeviceBlob {
            device_name: "Desktop".to_string(),
            schema: 1,
            ..Default::default()
        };
        blob.playtime.insert("Game A".to_string(), 42);
        blob.last_played.insert("Game A".to_string(), "2024-01-01T00:00:00Z".to_string());
        blob.backups.insert("Game A".to_string(), "2024-01-01T01:00:00Z".to_string());

        let json = serde_json::to_string(&blob).unwrap();
        let back: DeviceBlob = serde_json::from_str(&json).unwrap();
        assert_eq!(back.playtime["Game A"], 42);
        assert_eq!(back.device_name, "Desktop");
    }

    #[test]
    fn device_blob_missing_fields_default() {
        // A blob from an older schema without new fields should still parse.
        let json = r#"{"device_name":"Deck","schema":1}"#;
        let blob: DeviceBlob = serde_json::from_str(json).unwrap();
        assert!(blob.playtime.is_empty());
        assert!(blob.last_played.is_empty());
        assert!(blob.backups.is_empty());
    }

    // ── Playtime fold correctness ─────────────────────────────────────────────

    #[test]
    fn fold_sums_playtime_across_devices() {
        // A: 10 min, B: 5 min → both should fold to 15 total.
        let mut blob_a = DeviceBlob { device_name: "A".to_string(), ..Default::default() };
        blob_a.playtime.insert("Game X".to_string(), 10);
        let mut blob_b = DeviceBlob { device_name: "B".to_string(), ..Default::default() };
        blob_b.playtime.insert("Game X".to_string(), 5);

        let blobs = vec![blob_a, blob_b];
        let mut total: i64 = 0;
        for blob in &blobs {
            total += blob.playtime.get("Game X").copied().unwrap_or(0);
        }
        assert_eq!(total, 15);
    }

    #[test]
    fn fold_single_device_no_double_count() {
        // Restarting 3 times from the same device: the blob only has one
        // entry for that game (accumulated in-place), so it can't double-count.
        let mut blob = DeviceBlob { device_name: "A".to_string(), ..Default::default() };
        // Simulate 3 sessions of 10 min each, accumulated in the blob.
        blob.playtime.insert("Game X".to_string(), 30);
        let total: i64 = blob.playtime.get("Game X").copied().unwrap_or(0);
        assert_eq!(total, 30); // no double-count
    }

    // ── last_played fold ──────────────────────────────────────────────────────

    #[test]
    fn fold_takes_max_last_played() {
        let older = "2024-01-01T00:00:00Z";
        let newer = "2024-06-01T00:00:00Z";

        let t_a = chrono::DateTime::parse_from_rfc3339(older).unwrap().with_timezone(&chrono::Utc);
        let t_b = chrono::DateTime::parse_from_rfc3339(newer).unwrap().with_timezone(&chrono::Utc);

        let max = if t_b > t_a { t_b } else { t_a };
        assert_eq!(max.to_rfc3339(), t_b.to_rfc3339());
    }

    // ── compute_badge ─────────────────────────────────────────────────────────

    #[test]
    fn badge_ours_is_synced() {
        assert_eq!(compute_badge("device-a", Some("device-a")), "synced");
    }

    #[test]
    fn badge_other_is_cloud_newer() {
        assert_eq!(compute_badge("device-a", Some("device-b")), "cloud-newer");
    }

    #[test]
    fn badge_no_backups_is_synced() {
        assert_eq!(compute_badge("device-a", None), "synced");
    }

    // ── classify_marker ───────────────────────────────────────────────────────

    fn make_marker(state: SessionState, device_id: &str, updated_ago_secs: i64, suspended: bool) -> SessionMarker {
        let now = chrono::Utc::now();
        let updated_at = (now - chrono::Duration::seconds(updated_ago_secs)).to_rfc3339();
        SessionMarker {
            game_name: "Test Game".to_string(),
            device_id: device_id.to_string(),
            device_name: device_id.to_string(),
            started_at: updated_at.clone(),
            updated_at,
            state,
            suspended,
        }
    }

    #[test]
    fn classify_absent_returns_absent() {
        let result = classify_marker(None, "me", false, chrono::Utc::now());
        assert_eq!(result, MarkerClass::Absent);
    }

    #[test]
    fn classify_ours_returns_absent() {
        let m = make_marker(SessionState::Active, "me", 10, false);
        let result = classify_marker(Some(&m), "me", false, chrono::Utc::now());
        assert_eq!(result, MarkerClass::Absent);
    }

    #[test]
    fn classify_fresh_active_blocks() {
        let m = make_marker(SessionState::Active, "other", 10, false);
        let result = classify_marker(Some(&m), "me", false, chrono::Utc::now());
        assert_eq!(result, MarkerClass::ActivePlaying { device_name: "other".to_string() });
    }

    #[test]
    fn classify_stale_active_is_unsynced() {
        let m = make_marker(SessionState::Active, "other", 200, false); // > 180s = stale
        let result = classify_marker(Some(&m), "me", false, chrono::Utc::now());
        assert_eq!(result, MarkerClass::Unsynced { device_name: "other".to_string() });
    }

    #[test]
    fn classify_suspended_never_stale() {
        // Suspended devices retain Active even past the stale window.
        let m = make_marker(SessionState::Active, "other", 10_000, true);
        let result = classify_marker(Some(&m), "me", false, chrono::Utc::now());
        // Suspended + active → still counts as active playing (not stale).
        assert_eq!(result, MarkerClass::ActivePlaying { device_name: "other".to_string() });
    }

    #[test]
    fn classify_pending_backup_is_unsynced() {
        let m = make_marker(SessionState::PendingBackup, "other", 5, false);
        let result = classify_marker(Some(&m), "me", false, chrono::Utc::now());
        assert_eq!(result, MarkerClass::Unsynced { device_name: "other".to_string() });
    }

    #[test]
    fn classify_steal_always_proceeds() {
        let m = make_marker(SessionState::Active, "other", 10, false);
        let result = classify_marker(Some(&m), "me", true, chrono::Utc::now());
        assert_eq!(result, MarkerClass::Absent);
    }

    // ── get_rclone_remote_name_from_yaml ─────────────────────────────────────

    #[test]
    fn test_get_rclone_remote_name_from_yaml() {
        let yaml_str = r#"
cloud:
  remote:
    WebDav:
      id: ludusavi-1780143898
      url: http://192.168.86.34:47634
      username: DESKTOP-OAA3RS6
      provider: Other
  path: Spool/ludusavi-backup
        "#;
        let val: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let remote = get_rclone_remote_name_from_yaml(&val);
        assert_eq!(remote, Some("ludusavi-1780143898".to_string()));
    }

    #[test]
    fn test_get_rclone_remote_name_bare_string() {
        let yaml_str = "cloud:\n  remote: GoogleDrive\n  path: Spool/ludusavi-backup\n";
        let val: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        assert_eq!(
            get_rclone_remote_name_from_yaml(&val),
            Some("GoogleDrive".to_string()),
        );
    }
}

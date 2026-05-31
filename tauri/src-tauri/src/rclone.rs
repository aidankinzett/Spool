//! rclone-backed cross-device control plane.
//!
//! Replaces the old self-hosted HTTP sync server. Everything the sync server
//! used to do — the per-game play "lock" (now an advisory *unsynced-session
//! marker*), cross-device playtime + last-played, and the save-backup badge —
//! is stored as small JSON blobs in the **same rclone remote** already used for
//! cloud saves. No accounts, no auth, no health endpoint: the remote itself is
//! the trust boundary and rclone is already bundled + invoked at launch.
//!
//! ## Layout (under `<base>/_spool`, a sibling of `<base>/ludusavi-backup`)
//!
//! Kept a *sibling* of the ludusavi backup dir, never nested inside it —
//! ludusavi's `--cloud-sync` reconciles that subtree and would delete files it
//! doesn't recognise.
//!
//!   * `_spool/devices/<device_id>.json` — one [`DeviceBlob`] per device. Each
//!     device only ever writes its *own* file, so the store is conflict-free.
//!     The cross-device fold (at startup) sums playtime, takes the max
//!     last-played, and picks the newest backer for the badge.
//!   * `_spool/sessions/<blake3(game_name)>.json` — a [`SessionMarker`] written
//!     while a game is being played. It exists ⇔ "this device has a session
//!     whose saves aren't in the cloud yet", which is exactly the warning the
//!     user wants on another device.
//!
//! ## Reads use `cat`, not `lsjson`
//!
//! Session markers are read with `rclone cat <exact-path>` rather than a
//! directory listing: a specific-object read is read-after-write consistent on
//! far more backends than a listing (Drive/S3 list caches + `--fast-list` lag).
//! `lsjson` is reserved for the device-file fold, where staleness only delays a
//! stat sync, never correctness.

use crate::config::{ConfigData, SharedConfig};
use crate::library::SharedLibrary;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// Background reachability poll interval.
const POLL_INTERVAL: Duration = Duration::from_secs(60);
/// Timeout for a control-plane op (cat / rcat / deletefile / lsjson). The
/// blobs are tiny; this only needs to cover connect + a small transfer.
const OP_TIMEOUT: Duration = Duration::from_secs(8);
/// Timeout for the reachability probe (`rclone lsd <remote>:`).
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// How often the session heartbeat rewrites the marker's `updated_at`.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);
/// A live (Active, non-suspended) marker older than this is treated as stale —
/// the holder crashed or went offline without releasing. 3× the heartbeat
/// interval so a couple of missed writes don't trip it.
const ACTIVE_STALE_SECS: i64 = 180;
/// Timeout for the suspend-path marker write. Must be shorter than logind's
/// InhibitDelayMaxSec (default 5 s) so the write either completes or fails
/// before the inhibitor expires and the kernel freezes the process mid-upload.
#[cfg(target_os = "linux")]
const SUSPEND_TIMEOUT: Duration = Duration::from_secs(3);

/// Fast-fail flags folded into every control-plane rclone call so an
/// unreachable remote (classic SteamOS Game-Mode boot before Wi-Fi is up)
/// fails in seconds instead of blocking the launch for minutes.
pub const FAST_FLAGS: &[&str] = &[
    "--contimeout", "5s",
    "--timeout", "30s",
    "--retries", "1",
    "--low-level-retries", "1",
];

// ── Reachability status (unchanged shape, now driven by rclone) ─────────────

/// Reachability state. The frontend renders the chrome cloud icon from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncReachability {
    /// Cloud saves aren't configured — nothing to check.
    Unconfigured,
    /// The remote answered an `rclone lsd` within the timeout.
    Online,
    /// Network error, missing remote, or timeout.
    Offline,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub reachability: SyncReachability,
    /// Kept for JSON-shape compatibility with the old server status; always
    /// `None` now (there's no server version to report).
    pub server_version: Option<String>,
    /// Diagnostic on the last failure so the UI can show "Couldn't reach …".
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

/// Tauri-managed wrapper around the current reachability status. Updated by the
/// polling task; read by `current_sync_status` + the `sync:status-changed`
/// listener on the frontend.
#[derive(Default)]
pub struct SyncStatusState {
    inner: Mutex<SyncStatus>,
    last_ok: Mutex<Option<std::time::Instant>>,
}

impl SyncStatusState {
    fn snapshot(&self) -> SyncStatus {
        let mut s = self.inner.lock().map(|g| g.clone()).unwrap_or_default();
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

// ── Remote resolution ───────────────────────────────────────────────────────

/// A resolved rclone remote: the binary, the remote name (from ludusavi's
/// `config.yaml` `cloud.remote`), and the user's base folder (`cloud_base_path`).
#[derive(Debug, Clone)]
pub struct RcloneRemote {
    pub exe: PathBuf,
    pub remote: String,
    pub base: String,
}

impl RcloneRemote {
    /// `<remote>:<base>/_spool` — root of Spool's control-plane blobs.
    fn spool_dir(&self) -> String {
        format!("{}:{}/_spool", self.remote, self.base.trim_end_matches('/'))
    }
    fn device_target(&self, device_id: &str) -> String {
        format!("{}/devices/{}.json", self.spool_dir(), device_id)
    }
    fn devices_dir(&self) -> String {
        format!("{}/devices", self.spool_dir())
    }
    fn session_target(&self, game_name: &str) -> String {
        format!("{}/sessions/{}.json", self.spool_dir(), session_hash(game_name))
    }
}

/// blake3 hex digest of a game name — the session-marker filename. Stable for a
/// given name; the marker also stores the plaintext name so a (vanishingly
/// unlikely) collision is caught on read.
pub fn session_hash(game_name: &str) -> String {
    blake3::hash(game_name.as_bytes()).to_hex().to_string()
}

/// Parse the rclone remote name out of ludusavi's `config.yaml` `cloud.remote`.
/// Presets are bare strings (`Dropbox`); `Custom` / `WebDav` are tagged maps
/// carrying an `id`. Moved here from `runner.rs` so the runner's direct rclone
/// calls and the control plane share one parser.
pub fn remote_name_from_yaml(config: &serde_yaml::Value) -> Option<String> {
    let remote = config.get("cloud")?.get("remote")?;
    match remote {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Mapping(m) => {
            for tag in ["Custom", "WebDav"] {
                if let Some(inner) = m.get(serde_yaml::Value::String(tag.into())) {
                    if let Some(id) = inner.get(serde_yaml::Value::String("id".into())) {
                        return id.as_str().map(String::from);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Resolve the remote from app state. `None` when cloud isn't configured (no
/// `cloud.remote` in ludusavi's config) or the rclone binary can't be found.
pub fn resolve_remote(app: &AppHandle) -> Option<RcloneRemote> {
    let base = {
        let cfg = app.state::<SharedConfig>();
        let g = cfg.lock().ok()?;
        base_path(&g.data)
    };
    resolve_remote_inner(base)
}

/// Resolve the remote from a plain [`ConfigData`] — for headless paths that
/// have no Tauri-managed state (`spool --backup` / `--release-lock`).
pub fn resolve_remote_from_config(cfg: &ConfigData) -> Option<RcloneRemote> {
    resolve_remote_inner(base_path(cfg))
}

/// The configured base folder, defaulting to `Spool` if the user cleared it.
fn base_path(cfg: &ConfigData) -> String {
    let b = cfg.cloud_base_path.trim().trim_end_matches('/');
    if b.is_empty() { "Spool".to_string() } else { b.to_string() }
}

fn resolve_remote_inner(base: String) -> Option<RcloneRemote> {
    let raw = std::fs::read_to_string(crate::paths::ludusavi_config_file()).ok()?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&raw).ok()?;
    let remote = remote_name_from_yaml(&yaml)?;
    let exe = crate::paths::resolve_rclone_path()?;
    Some(RcloneRemote { exe, remote, base })
}

/// Reads (device_id, device_name) from config. Empty strings when the config
/// lock is poisoned — callers treat that as "skip the control-plane op".
fn device_identity(app: &AppHandle) -> (String, String) {
    let cfg = app.state::<SharedConfig>();
    let result = match cfg.lock() {
        Ok(g) => (g.data.device_id.clone(), g.data.device_name.clone()),
        Err(_) => (String::new(), String::new()),
    };
    result
}

// ── Low-level rclone helpers ────────────────────────────────────────────────

fn base_command(exe: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(exe);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);
    cmd.args(FAST_FLAGS);
    #[cfg(windows)]
    {
        #[allow(unused_imports)]
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// `rclone cat <target>` → stdout as a String. `None` on any failure (missing
/// file, network error, timeout).
pub async fn cat(exe: &Path, target: &str) -> Option<String> {
    let mut cmd = base_command(exe);
    cmd.arg("cat").arg(target);
    let child = cmd.spawn().ok()?;
    let out = tokio::time::timeout(OP_TIMEOUT, child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `rclone rcat <target>` reading `body` from stdin → object. rclone creates
/// intermediate dirs. Returns `true` on success.
async fn rcat(exe: &Path, target: &str, body: &[u8]) -> bool {
    use tokio::io::AsyncWriteExt;
    let mut cmd = base_command(exe);
    cmd.arg("rcat").arg(target);
    cmd.stdin(std::process::Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, target, "rclone rcat spawn failed");
            return false;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(body).await {
            tracing::warn!(error = %e, "rclone rcat: write stdin failed");
            return false;
        }
        drop(stdin); // close so rclone sees EOF
    }
    match tokio::time::timeout(OP_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(out)) if out.status.success() => true,
        Ok(Ok(out)) => {
            tracing::warn!(
                target,
                stderr = %String::from_utf8_lossy(&out.stderr),
                "rclone rcat non-zero exit"
            );
            false
        }
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "rclone rcat run error");
            false
        }
        Err(_) => {
            tracing::warn!(target, "rclone rcat timed out");
            false
        }
    }
}

/// `rclone deletefile <target>`. Best-effort; a missing file is fine.
async fn deletefile(exe: &Path, target: &str) -> bool {
    let mut cmd = base_command(exe);
    cmd.arg("deletefile").arg(target);
    match cmd.spawn() {
        Ok(child) => matches!(
            tokio::time::timeout(OP_TIMEOUT, child.wait_with_output()).await,
            Ok(Ok(out)) if out.status.success()
        ),
        Err(_) => false,
    }
}

/// One entry from `rclone lsjson`.
#[derive(Debug, Deserialize)]
struct LsEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "IsDir", default)]
    is_dir: bool,
}

/// `rclone lsjson <target>` → entries. `None` on failure (incl. missing dir).
async fn lsjson(exe: &Path, target: &str) -> Option<Vec<LsEntry>> {
    let mut cmd = base_command(exe);
    cmd.arg("lsjson").arg("--no-mimetype").arg(target);
    let child = cmd.spawn().ok()?;
    let out = tokio::time::timeout(OP_TIMEOUT, child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// `rclone lsd <remote>:` — cheap reachability probe (lists top-level dirs).
async fn lsd(exe: &Path, remote: &str) -> Result<(), String> {
    let mut cmd = base_command(exe);
    cmd.arg("lsd").arg(format!("{remote}:"));
    let child = cmd.spawn().map_err(|e| format!("spawn rclone: {e}"))?;
    match tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(out)) if out.status.success() => Ok(()),
        Ok(Ok(out)) => Err(format!(
            "rclone lsd failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Ok(Err(e)) => Err(format!("rclone lsd run error: {e}")),
        Err(_) => Err("rclone lsd timed out".to_string()),
    }
}

// ── Session markers (the unsynced-session warning) ──────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    /// A game is being played right now on the owning device.
    Active,
    /// The session ended but its saves haven't been uploaded to the cloud yet.
    PendingBackup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMarker {
    /// Plaintext name — guards against a hash collision on read.
    pub game_name: String,
    pub device_id: String,
    pub device_name: String,
    pub started_at: String,
    /// Bumped by the heartbeat; staleness is measured from this.
    pub updated_at: String,
    pub state: SessionState,
    /// Set by the logind suspend watcher; a suspended marker never goes stale.
    #[serde(default)]
    pub suspended: bool,
}

/// What a peer's marker means for *our* launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionClass {
    /// Proceed: no marker, our own marker, an override (`steal`), or cloud off.
    Free,
    /// Another device is actively playing right now.
    ActiveElsewhere { device_name: String },
    /// Another device has a session whose saves aren't in the cloud yet.
    UnsyncedElsewhere { device_name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Proceed,
    ActiveElsewhere,
    UnsyncedElsewhere,
}

fn parse_ts(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc).timestamp())
}

/// Pure classification of a peer's marker (no IO) — the heart of the warning.
fn classify(
    marker: Option<&SessionMarker>,
    our_device_id: &str,
    now_ts: i64,
    steal: bool,
) -> Decision {
    let Some(m) = marker else {
        return Decision::Proceed;
    };
    if steal || m.device_id == our_device_id {
        return Decision::Proceed;
    }
    let fresh = !m.suspended
        && parse_ts(&m.updated_at)
            .map(|t| now_ts - t < ACTIVE_STALE_SECS)
            .unwrap_or(false);
    match m.state {
        // A live, actively-heartbeating session on another device.
        SessionState::Active if fresh => Decision::ActiveElsewhere,
        // PendingBackup, suspended, or a stale Active (crashed/offline holder):
        // its saves aren't safely in the cloud — warn, but allow override.
        _ => Decision::UnsyncedElsewhere,
    }
}

/// Read a peer's session marker. `None` when absent, unreadable, or the stored
/// game name doesn't match (hash collision guard).
async fn read_session_marker(remote: &RcloneRemote, game_name: &str) -> Option<SessionMarker> {
    let body = cat(&remote.exe, &remote.session_target(game_name)).await?;
    let marker: SessionMarker = serde_json::from_str(&body).ok()?;
    (marker.game_name == game_name).then_some(marker)
}

fn build_marker(
    game_name: &str,
    device_id: &str,
    device_name: &str,
    state: SessionState,
    suspended: bool,
) -> SessionMarker {
    let now = Utc::now().to_rfc3339();
    SessionMarker {
        game_name: game_name.to_string(),
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        started_at: now.clone(),
        updated_at: now,
        state,
        suspended,
    }
}

async fn write_marker(remote: &RcloneRemote, marker: &SessionMarker) -> bool {
    let Ok(body) = serde_json::to_vec(marker) else {
        return false;
    };
    rcat(&remote.exe, &remote.session_target(&marker.game_name), &body).await
}

/// Phase 1.5: classify any existing marker and, when clear to proceed, claim
/// the session by writing our own `Active` marker. `steal` overrides a peer's
/// marker (the user's explicit "Play here instead"). No-op → `Free` when cloud
/// isn't configured.
pub async fn claim_session(app: &AppHandle, game_name: &str, steal: bool) -> SessionClass {
    let Some(remote) = resolve_remote(app) else {
        return SessionClass::Free;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return SessionClass::Free;
    }
    let marker = read_session_marker(&remote, game_name).await;
    match classify(marker.as_ref(), &device_id, Utc::now().timestamp(), steal) {
        Decision::Proceed => {
            let ours = build_marker(game_name, &device_id, &device_name, SessionState::Active, false);
            if !write_marker(&remote, &ours).await {
                tracing::warn!(game_name, "claim_session: failed to write Active marker — advisory locking disabled for this session");
            }
            SessionClass::Free
        }
        Decision::ActiveElsewhere => SessionClass::ActiveElsewhere {
            device_name: marker.map(|m| m.device_name).unwrap_or_else(|| "another device".into()),
        },
        Decision::UnsyncedElsewhere => SessionClass::UnsyncedElsewhere {
            device_name: marker.map(|m| m.device_name).unwrap_or_else(|| "another device".into()),
        },
    }
}

/// Spawn the session heartbeat: rewrites our `Active` marker's `updated_at`
/// every 60 s so peers see the session as live. `started_at` is the real
/// session-start timestamp captured at claim time — preserved on every tick
/// so peers always see accurate session age. Returns the JoinHandle; the
/// runner `.abort()`s it on session end.
pub fn start_heartbeat(app: AppHandle, game_name: String, started_at: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            let Some(remote) = resolve_remote(&app) else {
                return; // cloud disabled mid-session — stop
            };
            let (device_id, device_name) = device_identity(&app);
            if device_id.is_empty() {
                continue;
            }
            let marker = SessionMarker {
                game_name: game_name.clone(),
                device_id,
                device_name,
                started_at: started_at.clone(),
                updated_at: Utc::now().to_rfc3339(),
                state: SessionState::Active,
                suspended: false,
            };
            write_marker(&remote, &marker).await;
        }
    })
}

/// Clean session end: flip our marker to `PendingBackup` so peers keep warning
/// until the saves actually reach the cloud. Best-effort.
pub async fn mark_session_pending_backup(app: &AppHandle, game_name: &str) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    let marker = build_marker(game_name, &device_id, &device_name, SessionState::PendingBackup, false);
    write_marker(&remote, &marker).await;
}

/// AppHandle-free `PendingBackup` flip for `spool --release-lock` (the Decky
/// forced-close fallback). No-op when cloud isn't configured.
pub async fn mark_session_pending_backup_from_config(cfg: &ConfigData, game_name: &str) {
    let Some(remote) = resolve_remote_from_config(cfg) else {
        return;
    };
    let device_id = cfg.device_id.trim();
    if device_id.is_empty() {
        return;
    }
    let marker = build_marker(game_name, device_id, cfg.device_name.trim(), SessionState::PendingBackup, false);
    write_marker(&remote, &marker).await;
}

/// Successful cloud upload: delete the session marker (saves are safe in the
/// cloud) and record this device as the latest backer for the badge.
pub async fn complete_session_backup(app: &AppHandle, game_name: &str) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    deletefile(&remote.exe, &remote.session_target(game_name)).await;
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    update_device_blob(&remote, &device_id, |b| {
        b.device_name = device_name.clone();
        b.backups.insert(game_name.to_string(), Utc::now().to_rfc3339());
    })
    .await;
}

/// Delete our session marker without recording a backup — used when a game
/// failed to launch so no actual session occurred. Prevents a stale
/// `PendingBackup` marker from permanently blocking other devices.
pub async fn delete_session_marker(app: &AppHandle, game_name: &str) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    deletefile(&remote.exe, &remote.session_target(game_name)).await;
}

/// AppHandle-free marker deletion for `spool --backup` after a successful cloud
/// upload. No-op when cloud isn't configured.
pub async fn complete_session_backup_from_config(cfg: &ConfigData, game_name: &str) {
    let Some(remote) = resolve_remote_from_config(cfg) else {
        return;
    };
    deletefile(&remote.exe, &remote.session_target(game_name)).await;
    let device_id = cfg.device_id.trim();
    if device_id.is_empty() {
        return;
    }
    let device_name = cfg.device_name.trim().to_string();
    update_device_blob(&remote, device_id, |b| {
        b.device_name = device_name.clone();
        b.backups.insert(game_name.to_string(), Utc::now().to_rfc3339());
    })
    .await;
}

// ── Suspend integration (Linux logind) ──────────────────────────────────────

/// Mark our session marker suspended (device sleeping mid-session). A suspended
/// marker never goes stale, so a peer sees "unsynced session" rather than the
/// marker being silently reclaimed. Returns `true` on success.
///
/// Uses SUSPEND_TIMEOUT (< logind's InhibitDelayMaxSec) so the write completes
/// or fails before the inhibitor expires and the kernel freezes the process.
#[cfg(target_os = "linux")]
pub async fn mark_session_suspended(app: &AppHandle, game_name: &str) -> bool {
    let Some(remote) = resolve_remote(app) else {
        return false;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return false;
    }
    let marker = build_marker(game_name, &device_id, &device_name, SessionState::Active, true);
    tokio::time::timeout(SUSPEND_TIMEOUT, write_marker(&remote, &marker))
        .await
        .unwrap_or(false)
}

/// On resume, re-assert our (awake) marker. Returns `Some(device_name)` of the
/// peer that took over while we slept, or `None` if the session is still ours.
#[cfg(target_os = "linux")]
pub async fn resume_session(app: &AppHandle, game_name: &str) -> Option<String> {
    let remote = resolve_remote(app)?;
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return None;
    }
    let existing = read_session_marker(&remote, game_name).await;
    if let Some(m) = &existing {
        if m.device_id != device_id && !m.suspended {
            // Someone took over while we slept.
            return Some(m.device_name.clone());
        }
    }
    let marker = build_marker(game_name, &device_id, &device_name, SessionState::Active, false);
    write_marker(&remote, &marker).await;
    None
}

// ── Per-device blobs: playtime / last-played / backups ──────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceBlob {
    #[serde(default)]
    pub device_name: String,
    /// game -> minutes accrued ON THIS DEVICE only (so the fold can sum).
    #[serde(default)]
    pub playtime: BTreeMap<String, i64>,
    /// game -> rfc3339 of this device's last play.
    #[serde(default)]
    pub last_played: BTreeMap<String, String>,
    /// game -> rfc3339 of this device's last successful cloud upload.
    #[serde(default)]
    pub backups: BTreeMap<String, String>,
    #[serde(default)]
    pub schema: u32,
}

/// Read-modify-write of THIS device's blob. Cats the current file (default if
/// absent), applies `f`, rcats it back. Conflict-free since only this device
/// writes this file.
async fn update_device_blob<F>(remote: &RcloneRemote, device_id: &str, f: F)
where
    F: FnOnce(&mut DeviceBlob),
{
    let target = remote.device_target(device_id);
    let mut blob: DeviceBlob = cat(&remote.exe, &target)
        .await
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    f(&mut blob);
    blob.schema = 1;
    if let Ok(body) = serde_json::to_vec(&blob) {
        rcat(&remote.exe, &target, &body).await;
    }
}

/// Record a finished session AND mark the backup complete in a single device-
/// blob roundtrip (cat + rcat). Deletes the session marker, then writes
/// playtime, last_played, and the backup timestamp together.
///
/// Use this in the normal post-session path (cloud upload succeeded) instead
/// of calling `record_session` + `complete_session_backup` separately, which
/// would do two roundtrips to the same file.
pub async fn record_session_and_complete_backup(
    app: &AppHandle,
    game_name: &str,
    session_minutes: i32,
    session_end: &str,
) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    deletefile(&remote.exe, &remote.session_target(game_name)).await;
    let delta = session_minutes.max(0) as i64;
    update_device_blob(&remote, &device_id, |b| {
        b.device_name = device_name.clone();
        if delta > 0 {
            *b.playtime.entry(game_name.to_string()).or_default() += delta;
        }
        b.last_played.insert(game_name.to_string(), session_end.to_string());
        b.backups.insert(game_name.to_string(), Utc::now().to_rfc3339());
    })
    .await;
}

/// Record a finished session in this device's blob (playtime delta + last
/// played). Best-effort; no-op when cloud isn't configured.
pub async fn record_session(app: &AppHandle, game_name: &str, session_minutes: i32, session_end: &str) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    let delta = session_minutes.max(0) as i64;
    update_device_blob(&remote, &device_id, |b| {
        b.device_name = device_name.clone();
        if delta > 0 {
            *b.playtime.entry(game_name.to_string()).or_default() += delta;
        }
        b.last_played.insert(game_name.to_string(), session_end.to_string());
    })
    .await;
}

/// The badge for a game given the device id of its latest backer.
pub fn compute_badge(our_device_id: &str, latest_backer: Option<&str>) -> &'static str {
    match latest_backer {
        Some(id) if id == our_device_id => "synced",
        Some(_) => "cloud-newer",
        None => "synced", // nobody has backed up ⇒ nothing newer in the cloud
    }
}

/// Folded cross-device totals for one game.
#[derive(Default)]
struct Folded {
    playtime: i64,
    last_played: Option<i64>, // unix ts of the max last-played
    last_played_raw: Option<String>,
    latest_backer: Option<String>,
    latest_backup_ts: Option<i64>, // unix ts backing `latest_backer`
}

/// Fold a set of device blobs into per-game cross-device totals.
fn fold_blobs(blobs: &[(String, DeviceBlob)]) -> BTreeMap<String, Folded> {
    let mut out: BTreeMap<String, Folded> = BTreeMap::new();
    for (device_id, blob) in blobs {
        for (game, mins) in &blob.playtime {
            out.entry(game.clone()).or_default().playtime += *mins;
        }
        for (game, ts) in &blob.last_played {
            let e = out.entry(game.clone()).or_default();
            if let Some(parsed) = parse_ts(ts) {
                if e.last_played.map(|cur| parsed > cur).unwrap_or(true) {
                    e.last_played = Some(parsed);
                    e.last_played_raw = Some(ts.clone());
                }
            }
        }
        for (game, ts) in &blob.backups {
            let e = out.entry(game.clone()).or_default();
            if let Some(parsed) = parse_ts(ts) {
                if e.latest_backup_ts.map(|cur| parsed > cur).unwrap_or(true) {
                    e.latest_backup_ts = Some(parsed);
                    e.latest_backer = Some(device_id.clone());
                }
            }
        }
    }
    out
}

/// One-shot cross-device fold at startup: list device blobs, cat each, fold, and
/// merge into the library (playtime = sum, last-played = max, badge from the
/// latest backer). Saves once and emits `library:changed` if anything changed.
pub fn spawn_startup_fold(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Brief settle so the first reachability poll can resolve.
        tokio::time::sleep(Duration::from_secs(4)).await;
        run_startup_fold(&app).await;
    });
}

async fn run_startup_fold(app: &AppHandle) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    let (our_device_id, _) = device_identity(app);

    // List device files, then cat each.
    let Some(entries) = lsjson(&remote.exe, &remote.devices_dir()).await else {
        return;
    };
    let mut blobs: Vec<(String, DeviceBlob)> = Vec::new();
    for e in entries {
        if e.is_dir || !e.name.ends_with(".json") {
            continue;
        }
        let device_id = e.name.trim_end_matches(".json").to_string();
        let target = remote.device_target(&device_id);
        if let Some(body) = cat(&remote.exe, &target).await {
            if let Ok(blob) = serde_json::from_str::<DeviceBlob>(&body) {
                blobs.push((device_id, blob));
            }
        }
    }
    if blobs.is_empty() {
        return;
    }

    let folded = fold_blobs(&blobs);

    let library = app.state::<SharedLibrary>();
    let mut applied = 0usize;
    if let Ok(mut lib) = library.lock() {
        for entry in lib.entries.iter_mut() {
            let Some(f) = folded.get(&entry.game_name) else {
                continue;
            };
            // playtime = authoritative sum across devices
            let total = f.playtime.min(i32::MAX as i64) as i32;
            if entry.playtime_minutes != total {
                entry.playtime_minutes = total;
                applied += 1;
            }
            // last-played = max(local, folded)
            if let Some(raw) = &f.last_played_raw {
                if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
                    let parsed = parsed.with_timezone(&Utc);
                    if entry.last_played_at.map(|t| parsed > t).unwrap_or(true) {
                        entry.last_played_at = Some(parsed);
                        applied += 1;
                    }
                }
            }
            // badge from the latest backer
            let badge = compute_badge(&our_device_id, f.latest_backer.as_deref());
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
    tracing::info!(applied, devices = blobs.len(), "startup fold: done");
    if applied > 0 {
        let _ = app.emit("library:changed", &());
    }
}

// ── Background reachability poll ─────────────────────────────────────────────

/// Kicks off the reachability poll task. Called once from `lib.rs::run`'s setup.
pub fn spawn_health_poller(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            poll_once(&app).await;
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

async fn poll_once(app: &AppHandle) {
    let prev = app.state::<SyncStatusState>().snapshot();

    let new_status = match resolve_remote(app) {
        None => SyncStatus {
            reachability: SyncReachability::Unconfigured,
            ..Default::default()
        },
        Some(remote) => match lsd(&remote.exe, &remote.remote).await {
            Ok(()) => SyncStatus {
                reachability: SyncReachability::Online,
                last_ok_ago_secs: Some(0),
                ..Default::default()
            },
            Err(e) => SyncStatus {
                reachability: SyncReachability::Offline,
                error: Some(e),
                ..Default::default()
            },
        },
    };

    if new_status.reachability == SyncReachability::Online {
        app.state::<SyncStatusState>().mark_ok();
    }

    let changed = prev.reachability != new_status.reachability || prev.error != new_status.error;
    app.state::<SyncStatusState>().set(new_status);
    if changed {
        let snap = app.state::<SyncStatusState>().snapshot();
        if let Err(e) = app.emit("sync:status-changed", &snap) {
            tracing::warn!(error = %e, "failed to emit sync:status-changed");
        }
    }
}

// ── OAuth remote authentication ─────────────────────────────────────────────

/// Maps a UI provider string to `(rclone_type, remote_name)`.
///
/// `remote_name` must match the bare-string name that `apply_cloud` in
/// `ludusavi_config.rs` writes into ludusavi's `config.yaml` — that is what
/// `rclone lsd <remote>:` (the reachability probe) will look up.
fn oauth_remote(provider: &str) -> Option<(&'static str, &'static str)> {
    match provider {
        "google-drive" => Some(("drive",     "GoogleDrive")),
        "dropbox"      => Some(("dropbox",   "Dropbox")),
        "onedrive"     => Some(("onedrive",  "OneDrive")),
        "box"          => Some(("box",       "Box")),
        _              => None,
    }
}

/// Holds the child process of an in-flight `rclone authorize` so that
/// `cancel_cloud_oauth` can kill it while `connect_cloud_oauth` is waiting.
#[derive(Default)]
pub struct OAuthState {
    child: tokio::sync::Mutex<Option<tokio::process::Child>>,
}

/// Returns `true` when an rclone remote with the name matching `provider` already
/// exists in rclone.conf (i.e. OAuth was previously completed). Fast check via
/// `rclone config show <name>` — non-empty stdout = exists.
#[tauri::command]
pub async fn check_cloud_remote_exists(provider: String) -> bool {
    let Some((_, remote_name)) = oauth_remote(&provider) else {
        return false;
    };
    let Some(exe) = crate::paths::resolve_rclone_path() else {
        return false;
    };
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.args(["config", "show", remote_name]);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    cmd.stdin(std::process::Stdio::null());
    cmd.kill_on_drop(true);
    let Ok(child) = cmd.spawn() else {
        return false;
    };
    match tokio::time::timeout(Duration::from_secs(3), child.wait_with_output()).await {
        Ok(Ok(out)) => out.status.success() && !out.stdout.trim_ascii().is_empty(),
        _ => false,
    }
}

/// Run the rclone OAuth browser flow for `provider` and register the resulting
/// remote in rclone.conf so that ludusavi can use it for cloud sync.
///
/// This command blocks until the user completes the browser auth flow (up to 5
/// minutes). The frontend keeps a spinner visible during this time. Call
/// `cancel_cloud_oauth` to abort.
#[tauri::command]
pub async fn connect_cloud_oauth(
    provider: String,
    app: AppHandle,
    state: State<'_, OAuthState>,
) -> crate::error::AppResult<()> {
    use crate::error::AppError;
    use tokio::io::AsyncBufReadExt;

    let (rclone_type, remote_name) = oauth_remote(&provider)
        .ok_or_else(|| AppError::Other(format!("'{provider}' does not use OAuth (use WebDAV form or configure rclone manually)")))?;

    let exe = crate::paths::resolve_rclone_path()
        .ok_or_else(|| AppError::Other("rclone binary not found".into()))?;

    // Build the authorize command WITHOUT base_command — base_command sets
    // CREATE_NO_WINDOW on Windows which would silently suppress the browser launch.
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("authorize").arg(rclone_type);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    cmd.stdin(std::process::Stdio::null());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn()
        .map_err(|e| AppError::Other(format!("failed to start rclone authorize: {e}")))?;

    let stdout = child.stdout.take()
        .ok_or_else(|| AppError::Other("could not open rclone stdout".into()))?;

    // Park the child so cancel_cloud_oauth can kill it.
    *state.child.lock().await = Some(child);

    // Scan stdout line-by-line for the JSON token between the arrow markers.
    let mut reader = tokio::io::BufReader::new(stdout).lines();
    let mut in_token = false;
    let mut token_json: Option<String> = None;

    // Five-minute wall-clock budget for the user to complete browser auth.
    // EOF or broken pipe (child killed by cancel_cloud_oauth) exits the while let.
    let _ = tokio::time::timeout(Duration::from_secs(300), async {
        while let Ok(Some(line)) = reader.next_line().await {
            if line.contains("--->") {
                in_token = true;
                continue;
            }
            if in_token && line.trim_start().starts_with('{') {
                token_json = Some(line.trim().to_string());
            }
            if line.contains("<---End paste") {
                break;
            }
        }
    })
    .await;

    // Release the child slot regardless of outcome.
    *state.child.lock().await = None;

    let token = token_json
        .ok_or_else(|| AppError::Other("OAuth flow was cancelled or did not complete".into()))?;

    // Register the remote in rclone.conf.
    let token_arg = format!("token={token}");
    let mut create_cmd = base_command(&exe);
    create_cmd.args(["config", "create", remote_name, rclone_type, &token_arg]);
    let create_child = create_cmd.spawn()
        .map_err(|e| AppError::Other(format!("rclone config create spawn failed: {e}")))?;
    let create_out = tokio::time::timeout(
        Duration::from_secs(15),
        create_child.wait_with_output(),
    )
    .await
    .map_err(|_| AppError::Other("rclone config create timed out".into()))?
    .map_err(|e| AppError::Other(format!("rclone config create error: {e}")))?;

    if !create_out.status.success() {
        let stderr = String::from_utf8_lossy(&create_out.stderr).trim().to_string();
        return Err(AppError::Other(format!("rclone config create failed: {stderr}")));
    }

    // Immediately probe the new remote so the frontend badge updates.
    poll_once(&app).await;

    Ok(())
}

/// Abort an in-flight `connect_cloud_oauth` call by killing the `rclone authorize`
/// child process. The blocked `connect_cloud_oauth` will then see EOF on its
/// stdout reader and return an error to the frontend.
#[tauri::command]
pub async fn cancel_cloud_oauth(state: State<'_, OAuthState>) -> crate::error::AppResult<()> {
    let mut slot = state.child.lock().await;
    if let Some(mut child) = slot.take() {
        let _ = child.kill().await;
    }
    Ok(())
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn current_sync_status(state: State<'_, SyncStatusState>) -> SyncStatus {
    state.snapshot()
}

/// Force an immediate reachability probe (Settings refresh button).
#[tauri::command]
pub async fn refresh_sync_status(app: AppHandle) -> SyncStatus {
    poll_once(&app).await;
    app.state::<SyncStatusState>().snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(state: SessionState, updated_at: &str, suspended: bool, device: &str) -> SessionMarker {
        SessionMarker {
            game_name: "Hades".into(),
            device_id: device.into(),
            device_name: format!("{device}-name"),
            started_at: updated_at.into(),
            updated_at: updated_at.into(),
            state,
            suspended,
        }
    }

    #[test]
    fn remote_name_from_yaml_reads_webdav_id() {
        let yaml = r#"
cloud:
  remote:
    WebDav:
      id: ludusavi-1780143898
      url: http://192.168.86.34:47634
  path: Spool/ludusavi-backup
"#;
        let val: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(remote_name_from_yaml(&val), Some("ludusavi-1780143898".into()));
    }

    #[test]
    fn remote_name_from_yaml_reads_preset_string() {
        let yaml = "cloud:\n  remote: Dropbox\n  path: Spool/ludusavi-backup\n";
        let val: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(remote_name_from_yaml(&val), Some("Dropbox".into()));
    }

    #[test]
    fn session_marker_round_trips() {
        let m = marker(SessionState::PendingBackup, "2026-05-31T12:00:00Z", true, "deck");
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("pending-backup"), "kebab-case state: {json}");
        let back: SessionMarker = serde_json::from_str(&json).unwrap();
        assert_eq!(back.state, SessionState::PendingBackup);
        assert!(back.suspended);
    }

    #[test]
    fn session_marker_suspended_defaults_false() {
        let json = r#"{"game_name":"Hades","device_id":"d","device_name":"n",
            "started_at":"2026-05-31T12:00:00Z","updated_at":"2026-05-31T12:00:00Z","state":"active"}"#;
        let m: SessionMarker = serde_json::from_str(json).unwrap();
        assert!(!m.suspended);
    }

    #[test]
    fn session_hash_is_stable() {
        assert_eq!(session_hash("Hades"), session_hash("Hades"));
        assert_ne!(session_hash("Hades"), session_hash("Celeste"));
    }

    #[test]
    fn classify_absent_proceeds() {
        assert_eq!(classify(None, "me", 1000, false), Decision::Proceed);
    }

    #[test]
    fn classify_own_marker_proceeds() {
        let m = marker(SessionState::Active, "1970-01-01T00:00:00Z", false, "me");
        // Even a stale own marker is fine — it's ours to overwrite.
        assert_eq!(classify(Some(&m), "me", 9_999_999_999, false), Decision::Proceed);
    }

    #[test]
    fn classify_steal_always_proceeds() {
        let m = marker(SessionState::Active, "2026-05-31T12:00:00Z", false, "deck");
        let now = parse_ts("2026-05-31T12:00:30Z").unwrap(); // fresh
        assert_eq!(classify(Some(&m), "me", now, true), Decision::Proceed);
    }

    #[test]
    fn classify_fresh_active_peer_blocks_as_active() {
        let m = marker(SessionState::Active, "2026-05-31T12:00:00Z", false, "deck");
        let now = parse_ts("2026-05-31T12:01:00Z").unwrap(); // 60s < 180s
        assert_eq!(classify(Some(&m), "me", now, false), Decision::ActiveElsewhere);
    }

    #[test]
    fn classify_stale_active_peer_is_unsynced() {
        let m = marker(SessionState::Active, "2026-05-31T12:00:00Z", false, "deck");
        let now = parse_ts("2026-05-31T12:10:00Z").unwrap(); // 600s > 180s
        assert_eq!(classify(Some(&m), "me", now, false), Decision::UnsyncedElsewhere);
    }

    #[test]
    fn classify_pending_backup_peer_is_unsynced() {
        let m = marker(SessionState::PendingBackup, "2026-05-31T12:00:00Z", false, "deck");
        let now = parse_ts("2026-05-31T12:00:10Z").unwrap(); // even when fresh
        assert_eq!(classify(Some(&m), "me", now, false), Decision::UnsyncedElsewhere);
    }

    #[test]
    fn classify_suspended_peer_is_unsynced_not_active() {
        // A suspended Deck mid-session: never "active" (so it's overridable).
        let m = marker(SessionState::Active, "2026-05-31T12:00:00Z", true, "deck");
        let now = parse_ts("2026-05-31T12:00:10Z").unwrap();
        assert_eq!(classify(Some(&m), "me", now, false), Decision::UnsyncedElsewhere);
    }

    #[test]
    fn fold_sums_playtime_and_takes_max_last_played() {
        let mut a = DeviceBlob::default();
        a.playtime.insert("Hades".into(), 10);
        a.last_played.insert("Hades".into(), "2026-05-01T00:00:00Z".into());
        a.backups.insert("Hades".into(), "2026-05-01T00:00:00Z".into());
        let mut b = DeviceBlob::default();
        b.playtime.insert("Hades".into(), 5);
        b.last_played.insert("Hades".into(), "2026-05-02T00:00:00Z".into());
        b.backups.insert("Hades".into(), "2026-05-03T00:00:00Z".into());

        let folded = fold_blobs(&[("a".into(), a), ("b".into(), b)]);
        let h = folded.get("Hades").unwrap();
        assert_eq!(h.playtime, 15, "playtime sums across devices");
        assert_eq!(h.last_played_raw.as_deref(), Some("2026-05-02T00:00:00Z"));
        assert_eq!(h.latest_backer.as_deref(), Some("b"), "newest backup wins");
    }

    #[test]
    fn fold_single_device_equals_its_own_counter() {
        // Guards against double-counting: folding one device's blob 3× (as a
        // repeated startup would) never inflates the total.
        let mut a = DeviceBlob::default();
        a.playtime.insert("Hades".into(), 42);
        for _ in 0..3 {
            let folded = fold_blobs(&[("a".into(), a.clone())]);
            assert_eq!(folded.get("Hades").unwrap().playtime, 42);
        }
    }

    #[test]
    fn compute_badge_variants() {
        assert_eq!(compute_badge("me", Some("me")), "synced");
        assert_eq!(compute_badge("me", Some("other")), "cloud-newer");
        assert_eq!(compute_badge("me", None), "synced");
    }
}

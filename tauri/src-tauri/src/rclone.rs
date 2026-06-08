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
//!
//! ## Reachability is observed passively, not polled
//!
//! There's a single `rclone lsd` probe at startup; after that the cloud-icon
//! status is maintained from the success/failure of the control-plane ops the
//! app already runs (claim/heartbeat/backup/fold), reported through the
//! [`init_health_sink`] handle. A leaf op that succeeds — or returns a definite
//! "not found" (a session marker that simply doesn't exist) — proves the remote
//! answered and marks it Online; a connection error or timeout marks it Offline.
//! This avoids a 24/7 polling loop drawing on a (shared, quota-limited) remote
//! while Spool sits idle in the tray. The Settings page can still force an
//! immediate probe via `refresh_sync_status`.

use crate::config::{ConfigData, SharedConfig};
use crate::library::SharedLibrary;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use std::future::Future;
use tauri::{AppHandle, Emitter, Manager, State};

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

    /// Return a typed handle for one blob namespace under `_spool/<subdir>/`.
    /// `schema` is the highest version this build can safely interpret; blobs
    /// with a higher stored schema are rejected by `BlobStore::fetch`.
    fn store<'a>(&'a self, subdir: &'a str, schema: u32) -> BlobStore<'a> {
        BlobStore { remote: self, subdir, schema }
    }
}

// ── Blob-store: path + transport layer for named JSON blob namespaces ─────────

/// Typed handle for one namespace under `_spool/<subdir>/`.  Obtained via
/// `RcloneRemote::store(subdir, schema)`.
///
/// Every blob kind (devices, sessions, history, custom-saves,
/// manifest-overrides) gets its own `BlobStore`; the subdir and current-schema
/// constant are the only things that differ between them. Path helpers
/// (`target`/`dir`) and the three transport primitives (`publish`/`delete`/
/// `fetch`) are therefore shared and live in exactly one place.
struct BlobStore<'a> {
    remote: &'a RcloneRemote,
    subdir: &'a str,
    /// Maximum schema version this build can interpret. `fetch` rejects blobs
    /// whose stored `schema` field exceeds this — forward-compatibility gate.
    schema: u32,
}

impl BlobStore<'_> {
    /// Full remote path for this namespace: `<spool_dir>/<subdir>`.
    fn dir(&self) -> String {
        format!("{}/{}", self.remote.spool_dir(), self.subdir)
    }

    /// Full remote path for one blob: `<spool_dir>/<subdir>/<key>.json`.
    fn target(&self, key: &str) -> String {
        format!("{}/{}.json", self.dir(), key)
    }

    /// Serialize `blob` and upload it as `<key>.json`. Returns `true` on
    /// success, `false` on serialization failure or rclone error.
    async fn publish<T: Serialize>(&self, key: &str, blob: &T) -> bool {
        let Ok(body) = serde_json::to_vec(blob) else {
            return false;
        };
        rcat(&self.remote.exe, &self.target(key), &body).await
    }

    /// Delete `<key>.json`. Best-effort; a missing blob is not an error.
    async fn delete(&self, key: &str) {
        deletefile(&self.remote.exe, &self.target(key)).await;
    }

    /// Fetch, deserialize, and schema-gate one blob. Returns `None` when the
    /// blob is absent, unparseable, or its stored schema exceeds `self.schema`.
    async fn fetch<T: serde::de::DeserializeOwned + HasSchema>(&self, key: &str) -> Option<T> {
        let body = cat(&self.remote.exe, &self.target(key)).await?;
        let blob: T = serde_json::from_str(&body).ok()?;
        (blob.stored_schema() <= self.schema).then_some(blob)
    }

    /// List this namespace and return every parseable `*.json` blob as
    /// `(key, T)` pairs. The key is the filename stem (device-id or
    /// name-hash, depending on the namespace). Blobs whose stored schema
    /// exceeds `self.schema` are silently skipped (same forward-compat gate
    /// as `fetch`). Missing or unreadable entries are also skipped; an
    /// unreachable directory returns an empty vec.
    async fn read_all<T: serde::de::DeserializeOwned + HasSchema>(&self) -> Vec<(String, T)> {
        read_json_blobs::<T>(self.remote, &self.dir())
            .await
            .into_iter()
            .filter(|(_, blob)| blob.stored_schema() <= self.schema)
            .collect()
    }
}

/// Schema-version accessor for `BlobStore::fetch`. Implement on any blob
/// struct whose `schema` field guards forward-compatibility so that a newer
/// blob written by a future Spool version isn't interpreted with stale logic.
trait HasSchema {
    fn stored_schema(&self) -> u32;
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
        // Legacy bare-string form (pre-0.31 schema); kept for configs not yet
        // migrated by `ludusavi_config::migrate_bare_remote`.
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Mapping(m) => {
            for tag in ["Custom", "WebDav", "Box", "Dropbox", "GoogleDrive", "OneDrive", "Ftp", "Smb"] {
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

/// Resolve the remote from a plain [`ConfigData`] — for the headless plugin
/// server, which has no Tauri-managed state.
pub fn resolve_remote_from_config(cfg: &ConfigData) -> Option<RcloneRemote> {
    resolve_remote_inner(base_path(cfg))
}

/// The configured base folder, defaulting to `Spool` if the user cleared it.
/// The remote base folder, normalized: trimmed, no trailing slash, and falling
/// back to `"Spool"` when blank. Both the rclone control plane and the derived
/// ludusavi backup path (`config::update_config`) go through this so an empty
/// `cloud_base_path` can never split saves and `_spool` state across different
/// remote roots.
pub(crate) fn base_path(cfg: &ConfigData) -> String {
    let b = cfg.cloud.base_path.trim().trim_end_matches('/');
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
pub(crate) fn device_identity(app: &AppHandle) -> (String, String) {
    let cfg = app.state::<SharedConfig>();
    let result = match cfg.lock() {
        Ok(g) => (g.data.device_id.clone(), g.data.device_name.clone()),
        Err(_) => (String::new(), String::new()),
    };
    result
}

// ── Passive reachability reporting ──────────────────────────────────────────

/// Process-wide handle for passive reachability reporting. The leaf rclone
/// helpers (`cat`/`rcat`/`deletefile`/`lsjson`) are shared by the GUI and the
/// headless `spool --headless-server` plugin server, which doesn't carry an
/// `AppHandle`; rather than thread `Option<&AppHandle>` through every layer, the
/// GUI registers its handle here once at startup. Headless subprocesses never
/// set it, so reporting is a no-op there — they have no status pill to update.
static HEALTH_SINK: OnceLock<AppHandle> = OnceLock::new();

/// Register the GUI's `AppHandle` so leaf ops can report reachability. Called
/// once from `lib.rs::run`'s setup; later calls are ignored.
pub fn init_health_sink(app: AppHandle) {
    let _ = HEALTH_SINK.set(app);
}

/// Whether a completed control-plane op proves the remote was reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Online,
    Offline,
}

/// Map an rclone exit code to a reachability verdict. Codes where the remote
/// *answered* — 0 ok, 3 dir-not-found, 4 file-not-found, 9 no-files-transferred
/// (a missing session marker is the common, healthy case) — are Online. Every
/// other exit code means we couldn't get a clear answer from the remote
/// (connection refused, DNS failure, auth error, rclone's temporary/fatal
/// codes), so it's Offline. Erring toward Offline is safe for an advisory icon:
/// a false Offline self-corrects on the next successful op, whereas a false
/// Online would tell the user sync works when it doesn't.
fn reach_from_code(code: Option<i32>) -> Reach {
    match code {
        Some(0) | Some(3) | Some(4) | Some(9) => Reach::Online,
        _ => Reach::Offline,
    }
}

/// Last non-empty line of stderr, for the Offline status' diagnostic field.
fn stderr_tail(stderr: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(stderr);
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.lines().last().unwrap_or(trimmed).trim().to_string())
}

/// Fold a leaf op's observed reachability into the shared status. No-op when the
/// health sink isn't registered (headless subprocesses).
fn report_reach(reach: Reach, err: Option<String>) {
    let Some(app) = HEALTH_SINK.get() else {
        return;
    };
    let new_status = match reach {
        Reach::Online => SyncStatus {
            reachability: SyncReachability::Online,
            ..Default::default()
        },
        Reach::Offline => SyncStatus {
            reachability: SyncReachability::Offline,
            error: err,
            ..Default::default()
        },
    };
    apply_status(app, new_status);
}

/// Store a freshly-observed status: mark last-ok on success, persist it, and emit
/// `sync:status-changed` only when the reachability or error actually changed.
/// Shared by the startup probe (`poll_once`) and passive reporting.
fn apply_status(app: &AppHandle, new_status: SyncStatus) {
    let state = app.state::<SyncStatusState>();
    let prev = state.snapshot();
    if new_status.reachability == SyncReachability::Online {
        state.mark_ok();
    }
    let changed = prev.reachability != new_status.reachability || prev.error != new_status.error;
    state.set(new_status);
    if changed {
        let snap = app.state::<SyncStatusState>().snapshot();
        if let Err(e) = app.emit("sync:status-changed", &snap) {
            tracing::warn!(error = %e, "failed to emit sync:status-changed");
        }
    }
}

// ── Low-level rclone helpers ────────────────────────────────────────────────

fn base_command(exe: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(exe);
    crate::capture_stdio!(cmd);
    cmd.kill_on_drop(true);
    cmd.args(FAST_FLAGS);
    cmd
}

/// Run a prepared control-plane rclone `cmd` to completion under `OP_TIMEOUT`,
/// optionally writing `stdin_body` first, and fold the observed reachability
/// into the shared status. `label` names the op for logs/diagnostics.
///
/// Returns the process output when rclone ran to completion (any exit code), or
/// `None` on spawn failure, stdin-write failure, or our timeout — all reported
/// as Offline, since a leaf op that can't get an answer from the remote can't
/// confirm it's reachable. The one case left untouched is a process-management
/// error (couldn't reap the child): not a remote signal, so reachability is
/// unchanged. Shared by every leaf helper so the timeout + exit-code
/// classification lives in exactly one place.
async fn run_op(
    mut cmd: tokio::process::Command,
    label: &str,
    stdin_body: Option<&[u8]>,
) -> Option<std::process::Output> {
    use tokio::io::AsyncWriteExt;
    if stdin_body.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, op = label, "rclone spawn failed");
            report_reach(Reach::Offline, Some(format!("rclone {label}: spawn failed")));
            return None;
        }
    };
    if let Some(body) = stdin_body {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(body).await {
                tracing::warn!(error = %e, op = label, "rclone stdin write failed");
                report_reach(Reach::Offline, Some(format!("rclone {label}: stdin write failed")));
                return None;
            }
            drop(stdin); // close so rclone sees EOF
        }
    }
    match tokio::time::timeout(OP_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(out)) => {
            report_reach(reach_from_code(out.status.code()), stderr_tail(&out.stderr));
            Some(out)
        }
        Ok(Err(e)) => {
            tracing::warn!(error = %e, op = label, "rclone run error");
            None
        }
        Err(_) => {
            tracing::warn!(op = label, "rclone op timed out");
            report_reach(Reach::Offline, Some(format!("rclone {label} timed out")));
            None
        }
    }
}

/// Outcome of a `cat`, distinguishing a genuinely missing object from a remote
/// we couldn't reach. Lets callers tell "no cloud backup for this game yet" (a
/// clean first upload) apart from "transient/unreachable" (don't guess).
pub enum CatOutcome {
    /// Object read successfully — its contents.
    Found(String),
    /// Remote reachable, object/dir not there (rclone exit 3 = dir, 4 = file).
    NotFound,
    /// Couldn't determine — connection error, retries exhausted, timeout, spawn.
    Unreachable,
}

/// `rclone cat <target>`, classifying the result (see [`CatOutcome`]).
pub async fn cat_outcome(exe: &Path, target: &str) -> CatOutcome {
    let mut cmd = base_command(exe);
    cmd.arg("cat").arg(target);
    match run_op(cmd, "cat", None).await {
        Some(out) if out.status.success() => {
            CatOutcome::Found(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        // rclone exit codes: 3 = directory not found, 4 = file not found — a
        // definite "it isn't there". Anything else (5 temporary, 7 fatal, or a
        // timeout/spawn failure → None) means we can't be sure.
        Some(out) if matches!(out.status.code(), Some(3) | Some(4)) => CatOutcome::NotFound,
        _ => CatOutcome::Unreachable,
    }
}

/// `rclone cat <target>` → stdout as a String. `None` on any failure (missing
/// file, network error, timeout). Thin wrapper over [`cat_outcome`] for callers
/// that don't need to tell missing from unreachable apart.
pub async fn cat(exe: &Path, target: &str) -> Option<String> {
    match cat_outcome(exe, target).await {
        CatOutcome::Found(s) => Some(s),
        _ => None,
    }
}

/// `rclone rcat <target>` reading `body` from stdin → object. rclone creates
/// intermediate dirs. Returns `true` on success.
async fn rcat(exe: &Path, target: &str, body: &[u8]) -> bool {
    let mut cmd = base_command(exe);
    cmd.arg("rcat").arg(target);
    let Some(out) = run_op(cmd, "rcat", Some(body)).await else {
        return false;
    };
    if !out.status.success() {
        tracing::warn!(
            target,
            stderr = %String::from_utf8_lossy(&out.stderr),
            "rclone rcat non-zero exit"
        );
    }
    out.status.success()
}

/// `rclone deletefile <target>`. Best-effort; a missing file is fine.
async fn deletefile(exe: &Path, target: &str) -> bool {
    let mut cmd = base_command(exe);
    cmd.arg("deletefile").arg(target);
    run_op(cmd, "deletefile", None)
        .await
        .is_some_and(|out| out.status.success())
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
    let out = run_op(cmd, "lsjson", None).await?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    /// A game is being played right now on the owning device.
    Active,
    /// The session ended but its saves haven't been uploaded to the cloud yet.
    /// Fail-safe default: a marker missing its `state` is treated as having
    /// unsynced saves (peer warns) rather than as a live active session.
    #[default]
    PendingBackup,
}

/// Container-level `#[serde(default)]` (plus the `Default` impls above and on
/// `SessionState`) means an older marker missing a future field still loads —
/// the same JSON-shape-compatibility rule the persisted config/library follow,
/// important here because markers are written and read across devices that may
/// be on different Spool versions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
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
    let target = remote.store("sessions", 0).target(&session_hash(game_name));
    let body = cat(&remote.exe, &target).await?;
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
    remote
        .store("sessions", 0)
        .publish(&session_hash(&marker.game_name), marker)
        .await
}

/// Deletes the session marker for `game_name`, but only when it belongs to
/// `device_id` (or no longer exists). Once another device steals/takes over a
/// session it owns the marker; deleting it by name regardless — e.g. on this
/// device's own normal session finish — would erase the peer's live record and
/// silence the cross-device "unsynced session" warning. Best-effort.
async fn delete_marker_if_ours(remote: &RcloneRemote, game_name: &str, device_id: &str) {
    if let Some(m) = read_session_marker(remote, game_name).await {
        if m.device_id != device_id {
            tracing::info!(
                game_name,
                owner = %m.device_id,
                "skipping session-marker delete — owned by another device"
            );
            return;
        }
    }
    remote.store("sessions", 0).delete(&session_hash(game_name)).await;
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
            // If a peer has taken over the session (the user hit "Play here
            // instead" on another device), stop re-asserting our marker —
            // otherwise the two devices' heartbeats fight and the steal never
            // sticks. The local game is still running, but the cross-device
            // record now belongs to the peer.
            if let Some(m) = read_session_marker(&remote, &game_name).await {
                if m.device_id != device_id {
                    tracing::info!(
                        game_name,
                        owner = %m.device_id,
                        "heartbeat: session taken over by another device — stopping"
                    );
                    return;
                }
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
    // If a peer took over during our session, the marker is now theirs — don't
    // clobber their live record with our PendingBackup. Our own post-session
    // backup still runs; we just don't reclaim the cross-device marker.
    if let Some(m) = read_session_marker(&remote, game_name).await {
        if m.device_id != device_id {
            tracing::info!(
                game_name,
                owner = %m.device_id,
                "skipping pending-backup marker write — owned by another device"
            );
            return;
        }
    }
    let marker = build_marker(game_name, &device_id, &device_name, SessionState::PendingBackup, false);
    write_marker(&remote, &marker).await;
}

/// AppHandle-free `PendingBackup` flip for the plugin server's game-stop path
/// (the Decky forced-close fallback). No-op when cloud isn't configured.
#[cfg_attr(windows, allow(dead_code))] // only called from the unix-gated plugin server
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
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    // Marker delete and the device-blob stamp hit independent remote files, and
    // only the blob update takes the control-plane lock — run them concurrently
    // so their round-trips don't stack on a high-latency remote.
    tokio::join!(
        delete_marker_if_ours(&remote, game_name, &device_id),
        update_device_blob(&remote, &device_id, |b| {
            b.device_name = device_name.clone();
            b.backups.insert(game_name.to_string(), Utc::now().to_rfc3339());
        }),
    );
}

/// Delete our session marker without recording a backup — used when a game
/// failed to launch so no actual session occurred. Prevents a stale
/// `PendingBackup` marker from permanently blocking other devices.
pub async fn delete_session_marker(app: &AppHandle, game_name: &str) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    let (device_id, _) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    delete_marker_if_ours(&remote, game_name, &device_id).await;
}

/// AppHandle-free marker deletion for the plugin server's game-stop backup after
/// a successful cloud upload. No-op when cloud isn't configured.
pub async fn complete_session_backup_from_config(cfg: &ConfigData, game_name: &str) {
    let Some(remote) = resolve_remote_from_config(cfg) else {
        return;
    };
    let device_id = cfg.device_id.trim();
    if device_id.is_empty() {
        return;
    }
    let device_name = cfg.device_name.trim().to_string();
    // Independent remote files — run concurrently (mirrors complete_session_backup).
    tokio::join!(
        delete_marker_if_ours(&remote, game_name, device_id),
        update_device_blob(&remote, device_id, |b| {
            b.device_name = device_name.clone();
            b.backups.insert(game_name.to_string(), Utc::now().to_rfc3339());
        }),
    );
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
        if m.device_id != device_id {
            // Someone took over while we slept — whether their marker is
            // active or itself suspended. Don't overwrite it (that would erase
            // their unsynced-session record); just report the takeover.
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

impl HasSchema for DeviceBlob {
    fn stored_schema(&self) -> u32 { self.schema }
}

/// Read-modify-write of THIS device's blob. Cats the current file (default if
/// absent), applies `f`, rcats it back.
///
/// Each device writes only its own file, but `device_id` is per *installation*,
/// not per *process* — the tray GUI and the Decky `spool --headless-server`
/// all share it and so target the same file. `playtime` is a
/// `+=` accumulator, so two interleaved cat->rcat cycles would lose an update and
/// permanently undercount cross-device playtime (the `last_played`/`backups` maps
/// are last-writer-wins, so a lost update there is benign — and unlike the
/// history blob, which self-heals by re-projecting the local table, this
/// accumulator has no recovery path). A dedicated short-lived control-plane lock
/// serialises the brief write across processes; it is *not* the backup lock,
/// which is held across whole backups and which the soft-deferred-backup path
/// can't acquire. On the rare lock timeout we proceed best-effort rather than
/// drop the record entirely. See issue #282.
async fn update_device_blob<F>(remote: &RcloneRemote, device_id: &str, f: F)
where
    F: FnOnce(&mut DeviceBlob),
{
    let _lock = crate::proc_lock::acquire_control_plane(Duration::from_secs(15))
        .await
        .ok();
    let store = remote.store("devices", 1);
    let mut blob: DeviceBlob = match cat(&remote.exe, &store.target(device_id)).await {
        Some(s) => match serde_json::from_str::<DeviceBlob>(&s) {
            Ok(b) if b.schema <= store.schema => b,
            Ok(b) => {
                tracing::warn!(
                    device_id,
                    schema = b.schema,
                    "device blob was written by a newer Spool version — skipping update to avoid downgrade"
                );
                return;
            }
            Err(_) => DeviceBlob::default(),
        },
        None => DeviceBlob::default(),
    };
    f(&mut blob);
    blob.schema = store.schema;
    store.publish(device_id, &blob).await;
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

/// AppHandle-free [`record_session`] for the Game-Mode forced-close backup (the
/// plugin server's game-stop path): add playtime (when > 0) + last_played to this
/// device's blob from config. No-op when cloud isn't configured. The backup
/// timestamp + marker delete are a separate concern handled by
/// [`complete_session_backup_from_config`] once the upload lands.
#[cfg_attr(windows, allow(dead_code))] // only reached via the unix-gated plugin server
pub async fn record_session_from_config(
    cfg: &ConfigData,
    game_name: &str,
    session_minutes: i32,
    session_end: &str,
) {
    let Some(remote) = resolve_remote_from_config(cfg) else {
        return;
    };
    let device_id = cfg.device_id.trim();
    if device_id.is_empty() {
        return;
    }
    let device_name = cfg.device_name.trim().to_string();
    let delta = session_minutes.max(0) as i64;
    update_device_blob(&remote, device_id, |b| {
        b.device_name = device_name.clone();
        if delta > 0 {
            *b.playtime.entry(game_name.to_string()).or_default() += delta;
        }
        b.last_played.insert(game_name.to_string(), session_end.to_string());
    })
    .await;
}

// ── Per-device play-session history (the cross-device timeline) ─────────────

/// One device's full play-session history, stored at
/// `_spool/history/<device_id>.json`. Like [`DeviceBlob`], each device writes
/// only its *own* file, so the store is conflict-free: the cross-device view is
/// a union of every device's sessions keyed by `session_id`, never a merge.
///
/// The blob is a projection of the local `play_sessions` table (the source of
/// truth) rather than an independently-appended log: [`sync_play_history`]
/// rewrites it from all of this device's local rows. So a lost remote write
/// self-heals on the next session instead of permanently dropping a record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryBlob {
    pub device_name: String,
    pub sessions: Vec<crate::library::PlaySession>,
    pub schema: u32,
}

impl HasSchema for HistoryBlob {
    fn stored_schema(&self) -> u32 { self.schema }
}

/// Push this device's play-session history to the remote: read every local
/// session for our device id and overwrite our history blob with the full set.
/// Best-effort and idempotent; no-op when cloud isn't configured. Call after
/// recording a session locally.
pub async fn sync_play_history(app: &AppHandle) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    let library = app.state::<SharedLibrary>().inner().clone();
    let sessions = match library.sessions_for_device(&device_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "sync_play_history: failed to read local sessions");
            return;
        }
    };
    let blob = HistoryBlob { device_name, sessions, schema: 1 };
    remote.store("history", 1).publish(&device_id, &blob).await;
}

/// AppHandle-free [`sync_play_history`] for the Game-Mode forced-close fallback:
/// push this device's local play-session rows to its history blob from config.
/// Best-effort and idempotent; no-op when cloud isn't configured.
#[cfg_attr(windows, allow(dead_code))] // only reached via the unix-gated plugin server
pub async fn sync_play_history_from_config(cfg: &ConfigData, library: &crate::library::Library) {
    let Some(remote) = resolve_remote_from_config(cfg) else {
        return;
    };
    let device_id = cfg.device_id.trim();
    if device_id.is_empty() {
        return;
    }
    let device_name = cfg.device_name.trim().to_string();
    let sessions = match library.sessions_for_device(device_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "sync_play_history_from_config: failed to read local sessions");
            return;
        }
    };
    let blob = HistoryBlob { device_name, sessions, schema: 1 };
    remote.store("history", 1).publish(device_id, &blob).await;
}

/// Fold every peer's history blob into the local `play_sessions` table. Lists
/// `_spool/history`, cats each `<device_id>.json`, and bulk `INSERT OR IGNORE`s
/// its sessions (idempotent by `session_id`). Returns the number of new rows
/// added. Our own blob is included — harmless (already present locally), and it
/// restores our history if the local DB was wiped/reinstalled.
async fn fold_history(remote: &RcloneRemote, library: &crate::library::Library) -> usize {
    let blobs: Vec<(String, HistoryBlob)> = remote.store("history", 1).read_all().await;
    let mut all: Vec<crate::library::PlaySession> = Vec::new();
    for (_, blob) in blobs {
        all.extend(blob.sessions);
    }
    library.upsert_sessions(&all).await.unwrap_or(0)
}

// ── Generic fold: adopt name-keyed definitions from the control plane ─────────

/// Fetch all blobs in a namespace, map each to an optional `(game_name, def)`
/// pair via `to_def` (which handles schema/validity checks), then apply each
/// definition to every matching library entry that doesn't already have one.
/// Returns the number of entries updated. Best-effort; 0 when the namespace is
/// empty or unreachable.
///
/// `entry_has` returns `true` when the entry already carries this definition
/// (skip it). `set_if_absent` performs the conditional DB write — it must be
/// atomic so it can't clobber a definition set between the library list and
/// this write (the underlying `json_set … WHERE … IS NULL` ensures that).
async fn fold_name_keyed<B, D, F, Fut>(
    app: &AppHandle,
    store: &BlobStore<'_>,
    to_def: impl Fn(B) -> Option<(String, D)>,
    entry_has: impl Fn(&crate::library::GameEntry) -> bool,
    set_if_absent: F,
) -> usize
where
    B: serde::de::DeserializeOwned + HasSchema,
    D: Clone,
    F: Fn(SharedLibrary, String, D) -> Fut,
    Fut: Future<Output = bool>,
{
    let blobs: Vec<(String, B)> = store.read_all().await;
    let mut defs = std::collections::HashMap::<String, D>::new();
    for (_, blob) in blobs {
        if let Some((name, def)) = to_def(blob) {
            defs.insert(name, def);
        }
    }
    if defs.is_empty() {
        return 0;
    }
    let library = app.state::<SharedLibrary>().inner().clone();
    let entries = library.list().await.unwrap_or_default();
    let mut applied = 0usize;
    for entry in &entries {
        if entry_has(entry) {
            continue;
        }
        if let Some(def) = defs.get(&entry.game_name).cloned() {
            if set_if_absent(library.clone(), entry.id.clone(), def).await {
                applied += 1;
            }
        }
    }
    applied
}

// ── Custom-save definitions (cross-device "specify once") ────────────────────

/// Current custom-save blob schema. A blob written by a newer Spool (higher
/// `schema`) is ignored on read rather than adopted with templates this version
/// might not understand.
const CUSTOM_SAVE_SCHEMA: u32 = 1;

/// A replicated custom-save definition: `_spool/custom-saves/<blake3(name)>.json`.
/// Name-keyed (like session markers) so a game added on any device finds the
/// definition published from another. The `files`/`registry` are *portable*
/// ludusavi templates (placeholder tokens), identical on every device — see
/// [`crate::save_template`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct CustomSaveBlob {
    name: String,
    files: Vec<String>,
    registry: Vec<String>,
    updated_at: String,
    schema: u32,
}

impl HasSchema for CustomSaveBlob {
    fn stored_schema(&self) -> u32 { self.schema }
}

/// Publish (or update) this game's custom-save definition to the control plane
/// so other devices adopt it. Best-effort; no-op when cloud isn't configured.
pub async fn publish_custom_save(
    app: &AppHandle,
    game_name: &str,
    files: &[String],
    registry: &[String],
) {
    let Some(remote) = resolve_remote(app) else { return; };
    let blob = CustomSaveBlob {
        name: game_name.to_string(),
        files: files.to_vec(),
        registry: registry.to_vec(),
        updated_at: Utc::now().to_rfc3339(),
        schema: CUSTOM_SAVE_SCHEMA,
    };
    remote.store("custom-saves", CUSTOM_SAVE_SCHEMA).publish(&session_hash(game_name), &blob).await;
}

/// Remove a game's published custom-save definition (when the user clears it).
/// Best-effort; a missing blob is fine.
pub async fn delete_custom_save(app: &AppHandle, game_name: &str) {
    let Some(remote) = resolve_remote(app) else { return; };
    remote.store("custom-saves", CUSTOM_SAVE_SCHEMA).delete(&session_hash(game_name)).await;
}

/// Fetch a single published custom-save definition by game name (one `cat`).
/// Used to adopt a definition the moment a matching game is added on a new
/// device. `None` when cloud isn't configured or no definition exists.
pub async fn fetch_custom_save(app: &AppHandle, game_name: &str) -> Option<crate::library::CustomSave> {
    let remote = resolve_remote(app)?;
    let blob: CustomSaveBlob = remote
        .store("custom-saves", CUSTOM_SAVE_SCHEMA)
        .fetch(&session_hash(game_name))
        .await?;
    if blob.files.is_empty() && blob.registry.is_empty() {
        return None;
    }
    Some(crate::library::CustomSave { files: blob.files, registry: blob.registry })
}

/// List a control-plane subdir and read+parse every `*.json` blob in it as
/// `(file-stem, T)` pairs — the stem is the device id under `_spool/devices/`,
/// the name hash under `_spool/custom-saves/`. Empty on a missing/unreadable
/// dir. One place for the list→cat→parse loop the device fold and custom-save
/// fold both need (previously copied in each).
async fn read_json_blobs<T: serde::de::DeserializeOwned>(
    remote: &RcloneRemote,
    dir: &str,
) -> Vec<(String, T)> {
    let Some(entries) = lsjson(&remote.exe, dir).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries {
        if e.is_dir || !e.name.ends_with(".json") {
            continue;
        }
        let stem = e.name.trim_end_matches(".json").to_string();
        let target = format!("{dir}/{}", e.name);
        if let Some(body) = cat(&remote.exe, &target).await {
            if let Ok(blob) = serde_json::from_str::<T>(&body) {
                out.push((stem, blob));
            }
        }
    }
    out
}

/// Adopt published custom-save definitions into local library entries that don't
/// have one yet (matched by game name). Only fills `None` — a device that has
/// already set its own custom save keeps it. Returns how many entries were
/// updated; the caller re-syncs ludusavi's `customGames` block and emits
/// `library:changed`. Best-effort; 0 when cloud isn't configured.
pub async fn fold_custom_saves(app: &AppHandle) -> usize {
    let Some(remote) = resolve_remote(app) else { return 0; };
    fold_name_keyed(
        app,
        &remote.store("custom-saves", CUSTOM_SAVE_SCHEMA),
        |blob: CustomSaveBlob| {
            let has_paths = !blob.files.is_empty() || !blob.registry.is_empty();
            if blob.schema > CUSTOM_SAVE_SCHEMA || blob.name.is_empty() || !has_paths {
                return None;
            }
            Some((blob.name, crate::library::CustomSave { files: blob.files, registry: blob.registry }))
        },
        |entry| entry.custom_save.is_some(),
        |lib, id, def| async move { lib.set_custom_save_if_absent(&id, &def).await.unwrap_or(false) },
    )
    .await
}

// ── Manifest overrides (control-plane replication) ──────────────────────────

const MANIFEST_OVERRIDE_SCHEMA: u32 = 1;

/// A replicated manifest override: `_spool/manifest-overrides/<blake3(name)>.json`.
/// Name-keyed like custom saves. Stores the user's *exclusion intent* (tags +
/// literal templates), never resolved paths — each device re-derives its own
/// ludusavi override from its manifest minus these. Tags carry across OSes;
/// templates only match on a device whose manifest has them.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ManifestOverrideBlob {
    name: String,
    excluded_tags: Vec<String>,
    excluded_paths: Vec<String>,
    updated_at: String,
    schema: u32,
}

impl HasSchema for ManifestOverrideBlob {
    fn stored_schema(&self) -> u32 { self.schema }
}

/// Publish (or update) this game's manifest override so other devices adopt the
/// same exclusions. Best-effort; no-op when cloud isn't configured.
pub async fn publish_manifest_override(
    app: &AppHandle,
    game_name: &str,
    ov: &crate::library::ManifestOverride,
) {
    let Some(remote) = resolve_remote(app) else { return; };
    let blob = ManifestOverrideBlob {
        name: game_name.to_string(),
        excluded_tags: ov.excluded_tags.clone(),
        excluded_paths: ov.excluded_paths.clone(),
        updated_at: Utc::now().to_rfc3339(),
        schema: MANIFEST_OVERRIDE_SCHEMA,
    };
    remote
        .store("manifest-overrides", MANIFEST_OVERRIDE_SCHEMA)
        .publish(&session_hash(game_name), &blob)
        .await;
}

/// Remove a game's published manifest override (when the user clears it).
/// Best-effort; a missing blob is fine.
pub async fn delete_manifest_override(app: &AppHandle, game_name: &str) {
    let Some(remote) = resolve_remote(app) else { return; };
    remote
        .store("manifest-overrides", MANIFEST_OVERRIDE_SCHEMA)
        .delete(&session_hash(game_name))
        .await;
}

/// Fetch a single published manifest override by game name (one `cat`). `None`
/// when cloud isn't configured, no override exists, or it excludes nothing.
pub async fn fetch_manifest_override(
    app: &AppHandle,
    game_name: &str,
) -> Option<crate::library::ManifestOverride> {
    let remote = resolve_remote(app)?;
    let blob: ManifestOverrideBlob = remote
        .store("manifest-overrides", MANIFEST_OVERRIDE_SCHEMA)
        .fetch(&session_hash(game_name))
        .await?;
    let ov = crate::library::ManifestOverride {
        excluded_tags: blob.excluded_tags,
        excluded_paths: blob.excluded_paths,
    };
    ov.is_active().then_some(ov)
}

/// Adopt published manifest overrides into local entries that don't have one yet
/// (matched by game name). Only fills `None`. Returns how many entries changed;
/// the caller re-syncs the `customGames` block and emits `library:changed`.
/// Best-effort; 0 when cloud isn't configured.
pub async fn fold_manifest_overrides(app: &AppHandle) -> usize {
    let Some(remote) = resolve_remote(app) else { return 0; };
    fold_name_keyed(
        app,
        &remote.store("manifest-overrides", MANIFEST_OVERRIDE_SCHEMA),
        |blob: ManifestOverrideBlob| {
            if blob.schema > MANIFEST_OVERRIDE_SCHEMA || blob.name.is_empty() {
                return None;
            }
            let ov = crate::library::ManifestOverride {
                excluded_tags: blob.excluded_tags,
                excluded_paths: blob.excluded_paths,
            };
            ov.is_active().then_some((blob.name, ov))
        },
        |entry| entry.manifest_override.is_some(),
        |lib, id, def| async move {
            lib.set_manifest_override_if_absent(&id, &def).await.unwrap_or(false)
        },
    )
    .await
}

/// The badge for a game given the device id of its latest backer.
pub fn compute_badge(our_device_id: &str, latest_backer: Option<&str>) -> &'static str {
    match latest_backer {
        Some(id) if id == our_device_id => "synced",
        Some(_) => "cloud-newer",
        None => "synced", // nobody has backed up ⇒ nothing newer in the cloud
    }
}

/// The backer device name + cloud revision time to record for a given badge.
/// Only `cloud-newer` carries them (so the UI can show "Desktop-PC · 2h ago");
/// every other state clears the pair.
fn backer_for_badge(badge: &str, f: &Folded) -> (Option<String>, Option<DateTime<Utc>>) {
    if badge != "cloud-newer" {
        return (None, None);
    }
    let rev = f
        .latest_backup_raw
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc));
    (f.latest_backer_name.clone(), rev)
}

/// Folded cross-device backup state for one game (the sync badge). Playtime and
/// last-played are no longer folded here — they're derived from the local
/// `play_sessions` timeline once peer history has been folded in.
#[derive(Default)]
struct Folded {
    latest_backer: Option<String>,
    latest_backup_ts: Option<i64>, // unix ts backing `latest_backer`
    latest_backer_name: Option<String>, // display name of `latest_backer`
    latest_backup_raw: Option<String>, // rfc3339 of `latest_backup_ts`
}

/// Fold a set of device blobs into each game's cross-device backup state (the
/// latest backer, for the sync badge).
fn fold_device_totals(blobs: &[(String, DeviceBlob)]) -> BTreeMap<String, Folded> {
    let mut out: BTreeMap<String, Folded> = BTreeMap::new();
    for (device_id, blob) in blobs {
        for (game, ts) in &blob.backups {
            let e = out.entry(game.clone()).or_default();
            if let Some(parsed) = parse_ts(ts) {
                if e.latest_backup_ts.map(|cur| parsed > cur).unwrap_or(true) {
                    e.latest_backup_ts = Some(parsed);
                    e.latest_backer = Some(device_id.clone());
                    // device_name may be blank on legacy blobs — fall back to id.
                    e.latest_backer_name = Some(if blob.device_name.is_empty() {
                        device_id.clone()
                    } else {
                        blob.device_name.clone()
                    });
                    e.latest_backup_raw = Some(ts.clone());
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

/// Headless variant of the startup fold — no `AppHandle` required. Loads
/// config + library from disk, runs the cross-device fold, saves if anything
/// changed, and returns whether the library was modified. Used by the plugin
/// server's `POST /fold` so the Decky UI can refresh data on page navigation.
#[cfg(unix)]
pub async fn fold_devices_from_config() -> bool {
    let config = match crate::config::Config::load() {
        Ok(c) => c.data,
        Err(_) => return false,
    };
    let Some(remote) = resolve_remote_from_config(&config) else {
        return false;
    };
    let our_device_id = config.device_id.clone();

    let library = match crate::library::Library::open().await {
        Ok(l) => l,
        Err(_) => return false,
    };

    // Fold peer play-session history first, independent of the device-blob fold.
    let new_sessions = fold_history(&remote, &library).await;
    if new_sessions > 0 {
        tracing::info!(new_sessions, "headless fold: imported peer play sessions");
    }
    // Re-derive playtime/last-played from the timeline (replaces the device-blob
    // playtime fold).
    let recomputed = library.recompute_all_playtime().await.unwrap_or(0);

    let blobs: Vec<(String, DeviceBlob)> = remote.store("devices", 1).read_all().await;
    if blobs.is_empty() {
        return new_sessions > 0 || recomputed > 0;
    }

    let folded = fold_device_totals(&blobs);
    let entries = library.list().await.unwrap_or_default();
    let mut applied = 0usize;
    for entry in &entries {
        let Some(f) = folded.get(&entry.game_name) else {
            continue;
        };
        let fields = fold_fields_for(entry, f, &our_device_id);
        if !fields.is_empty() && library.update_fields(&entry.id, &fields).await.unwrap_or(false) {
            applied += 1;
        }
    }
    tracing::info!(applied, devices = blobs.len(), "headless fold: done");
    applied > 0 || new_sessions > 0 || recomputed > 0
}

/// Computes the JSON field updates the cross-device fold should apply to one
/// library entry — only the fields whose folded value differs from the current
/// one. Shared by the headless and GUI folds.
fn fold_fields_for(
    entry: &crate::library::GameEntry,
    f: &Folded,
    our_device_id: &str,
) -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::{json, to_value, Value};
    let mut fields: Vec<(&'static str, Value)> = Vec::new();
    // playtime + last-played are no longer folded from device blobs — they're
    // derived from the play_sessions timeline (see `Library::recompute_playtime`),
    // which the caller refreshes after folding peer history.
    // badge from the latest backer
    let badge = compute_badge(our_device_id, f.latest_backer.as_deref());
    if entry.sync_badge.as_deref() != Some(badge) {
        fields.push(("sync_badge", json!(badge)));
    }
    // backer device + revision time behind a `cloud-newer` badge
    let (backer, rev) = backer_for_badge(badge, f);
    if entry.save_last_backer_device != backer {
        fields.push((
            "save_last_backer_device",
            to_value(&backer).unwrap_or(Value::Null),
        ));
    }
    if entry.save_cloud_revision_at != rev {
        fields.push(("save_cloud_revision_at", to_value(rev).unwrap_or(Value::Null)));
    }
    fields
}

async fn run_startup_fold(app: &AppHandle) {
    let Some(remote) = resolve_remote(app) else {
        return;
    };
    let (our_device_id, _) = device_identity(app);

    // Fold peer play-session history into the local table first — independent of
    // the device-blob fold below, so a peer with sessions but no device blob
    // (or vice versa) is still picked up.
    let library = app.state::<SharedLibrary>().inner().clone();
    let new_sessions = fold_history(&remote, &library).await;
    if new_sessions > 0 {
        tracing::info!(new_sessions, "startup fold: imported peer play sessions");
    }
    // Re-derive playtime/last-played from the timeline (now including any peer
    // sessions just folded in) — this is what replaces the device-blob playtime
    // fold below.
    let recomputed = library.recompute_all_playtime().await.unwrap_or(0);

    // List device files, then cat each — only the backup-badge fields remain.
    let blobs: Vec<(String, DeviceBlob)> = remote.store("devices", 1).read_all().await;
    if blobs.is_empty() {
        if recomputed > 0 || new_sessions > 0 {
            let _ = app.emit("library:changed", &());
        }
        return;
    }

    let folded = fold_device_totals(&blobs);

    let entries = library.list().await.unwrap_or_default();
    let mut applied = 0usize;
    for entry in &entries {
        let Some(f) = folded.get(&entry.game_name) else {
            continue;
        };
        let fields = fold_fields_for(entry, f, &our_device_id);
        if !fields.is_empty() && library.update_fields(&entry.id, &fields).await.unwrap_or(false) {
            applied += 1;
        }
    }
    tracing::info!(applied, devices = blobs.len(), "startup fold: done");
    if applied > 0 || recomputed > 0 || new_sessions > 0 {
        let _ = app.emit("library:changed", &());
    }
}

// ── Background reachability poll ─────────────────────────────────────────────

/// Run the single startup reachability probe. Called once from `lib.rs::run`'s
/// setup, after [`init_health_sink`]. From then on the status is maintained
/// passively from real ops (see the module header) plus `refresh_sync_status`.
pub fn spawn_initial_sync_probe(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        poll_once(&app).await;
    });
}

/// Explicit `rclone lsd` probe — the startup check and the Settings refresh
/// button. The passive path (`report_reach`) keeps the status current between
/// these without spending a request per minute on an idle remote.
async fn poll_once(app: &AppHandle) {
    let new_status = match resolve_remote(app) {
        None => {
            tracing::info!("sync probe: no remote configured (resolve_remote returned None)");
            SyncStatus {
                reachability: SyncReachability::Unconfigured,
                ..Default::default()
            }
        }
        Some(remote) => {
            tracing::info!(remote = %remote.remote, exe = ?remote.exe, "sync probe: running lsd");
            match lsd(&remote.exe, &remote.remote).await {
                Ok(()) => {
                    tracing::info!(remote = %remote.remote, "sync probe: online");
                    SyncStatus {
                        reachability: SyncReachability::Online,
                        ..Default::default()
                    }
                }
                Err(e) => {
                    tracing::warn!(remote = %remote.remote, error = %e, "sync probe: offline");
                    SyncStatus {
                        reachability: SyncReachability::Offline,
                        error: Some(e),
                        ..Default::default()
                    }
                }
            }
        }
    };

    apply_status(app, new_status);
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

/// OAuth scope requested for Google Drive. `drive.file` limits access to
/// files this OAuth client created (i.e. Spool's own backup tree) rather
/// than the user's entire Drive. It's a "sensitive" — not "restricted" —
/// scope, so publishing the consent screen doesn't trigger Google's paid
/// annual restricted-scope security assessment that the full `drive` scope
/// would. It's persisted into the remote (so ludusavi's rclone calls inherit
/// it) and requested during the authorize flow.
const GDRIVE_SCOPE: &str = "drive.file";

/// Spool's own Google Drive OAuth client, baked in at build time from the
/// `SPOOL_GDRIVE_CLIENT_ID` / `SPOOL_GDRIVE_CLIENT_SECRET` env vars (set by
/// CI from repo secrets — see `.github/workflows/release.yml`).
///
/// Google enforces Drive API quota per OAuth client, and rclone's built-in
/// default client is shared by every rclone user worldwide, so it's
/// permanently rate-limited. Shipping our own client gives Spool users a
/// dedicated quota we can raise from the Google Cloud console.
///
/// Returns `None` when the env vars weren't present at compile time (local /
/// dev builds), in which case the OAuth flow falls back to rclone's shared
/// default client — same behaviour as before this was added.
fn gdrive_oauth_client() -> Option<(&'static str, &'static str)> {
    match (
        option_env!("SPOOL_GDRIVE_CLIENT_ID"),
        option_env!("SPOOL_GDRIVE_CLIENT_SECRET"),
    ) {
        (Some(id), Some(secret)) if !id.is_empty() && !secret.is_empty() => Some((id, secret)),
        _ => None,
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

    // Kill any authorize child left over from a previous attempt so the port
    // rclone binds (127.0.0.1:53682) isn't already in use.
    {
        let mut slot = state.child.lock().await;
        if let Some(mut child) = slot.take() {
            let _ = child.kill().await;
        }
    }

    // Spool's baked-in Google Drive client, if this build has one. Applied
    // only to the drive backend — other providers keep rclone's defaults.
    let gdrive = (rclone_type == "drive").then(gdrive_oauth_client).flatten();

    // Build the authorize command WITHOUT base_command — base_command sets
    // CREATE_NO_WINDOW on Windows which would silently suppress the browser launch.
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("authorize").arg(rclone_type);
    if let Some((id, secret)) = gdrive {
        // Documented custom-client form: `rclone authorize drive <id> <secret>`.
        cmd.arg(id).arg(secret);
        // Request the narrower scope during authorization so the granted token
        // is itself limited to Spool's files. rclone reads backend options from
        // RCLONE_<BACKEND>_<OPTION> env vars on every command, authorize included.
        cmd.env("RCLONE_DRIVE_SCOPE", GDRIVE_SCOPE);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdin(std::process::Stdio::null());
    cmd.kill_on_drop(true);
    // rclone authorize forks xdg-open to launch the browser. Under the AppImage,
    // a bare spawn leaks $APPDIR's PATH/LD_LIBRARY_PATH/GTK_* into that child, so
    // xdg-open (and the browser) fail and no auth window ever appears. Strip the
    // AppImage env so the opener sees the host runtime — same reason system_open
    // and the Proton runner do this.
    #[cfg(target_os = "linux")]
    crate::process::strip_appimage_env(&mut cmd);

    let mut child = cmd.spawn()
        .map_err(|e| AppError::Other(format!("failed to start rclone authorize: {e}")))?;

    let stdout = child.stdout.take()
        .ok_or_else(|| AppError::Other("could not open rclone stdout".into()))?;
    let stderr = child.stderr.take()
        .ok_or_else(|| AppError::Other("could not open rclone stderr".into()))?;

    // Park the child so cancel_cloud_oauth can kill it.
    *state.child.lock().await = Some(child);

    // Scan both stdout and stderr for the token — different rclone builds (and
    // versions) write the "Paste the following --->" block to different streams.
    // Non-token stderr lines are collected so we can surface them in the error
    // if the flow never produces a token.
    let mut stdout_reader = tokio::io::BufReader::new(stdout).lines();
    let mut stderr_reader = tokio::io::BufReader::new(stderr).lines();
    let mut in_token = false;
    let mut token_json: Option<String> = None;
    let mut stderr_diag: Vec<String> = Vec::new();
    let mut stdout_done = false;
    let mut stderr_done = false;

    // Five-minute wall-clock budget for the user to complete browser auth.
    let timed_out = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            if token_json.is_some() || (stdout_done && stderr_done) {
                break;
            }
            tokio::select! {
                line = stdout_reader.next_line(), if !stdout_done => {
                    match line {
                        Ok(Some(line)) => {
                            if line.contains("--->") { in_token = true; }
                            else if in_token && line.trim_start().starts_with('{') {
                                token_json = Some(line.trim().to_string());
                            } else if line.contains("<---End paste") { break; }
                        }
                        _ => { stdout_done = true; }
                    }
                }
                line = stderr_reader.next_line(), if !stderr_done => {
                    match line {
                        Ok(Some(line)) => {
                            if line.contains("--->") { in_token = true; }
                            else if in_token && line.trim_start().starts_with('{') {
                                token_json = Some(line.trim().to_string());
                            } else if line.contains("<---End paste") { break; }
                            else { stderr_diag.push(line); }
                        }
                        _ => { stderr_done = true; }
                    }
                }
            }
        }
    })
    .await;

    // Release the child slot regardless of outcome.
    *state.child.lock().await = None;

    if timed_out.is_err() {
        return Err(AppError::Other(
            "OAuth flow timed out after 300s — the browser window was not completed in time".to_string(),
        ));
    }

    let token = token_json.ok_or_else(|| {
        let detail = if stderr_diag.is_empty() {
            "no output from rclone — check that the bundled rclone binary is executable".to_string()
        } else {
            stderr_diag.join(" | ")
        };
        AppError::Other(format!("OAuth flow did not complete: {detail}"))
    })?;

    // Register the remote in rclone.conf. Persist the custom client + scope
    // alongside the token so ludusavi's own rclone invocations (backup /
    // restore / the reachability probe) use the same client and quota.
    let token_arg = format!("token={token}");
    let mut create_args: Vec<String> = vec![
        "config".into(),
        "create".into(),
        remote_name.into(),
        rclone_type.into(),
        token_arg,
        // We already obtained a token via `rclone authorize` above. Without
        // --non-interactive, `config create` runs the backend's OAuth config
        // process and *takes the default* for each question — and the oauth step
        // ("Already have a token - refresh?") defaults to yes, so it opens the
        // browser a SECOND time to re-authorise. --non-interactive makes it
        // store the params we pass (token/client/scope) and return the pending
        // question as JSON instead of acting on it, so no browser is launched.
        "--non-interactive".into(),
    ];
    if let Some((id, secret)) = gdrive {
        create_args.push(format!("client_id={id}"));
        create_args.push(format!("client_secret={secret}"));
        create_args.push(format!("scope={GDRIVE_SCOPE}"));
    }
    let mut create_cmd = base_command(&exe);
    create_cmd.args(&create_args);
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

    // Write the provider into ludusavi's config.yaml so the reachability probe
    // can resolve the remote. Without this the probe returns Unconfigured and
    // the pill stays "Offline" until the user separately triggers a settings
    // save. This mirrors what update_config does when the user changes the
    // provider dropdown, making the OAuth button idempotent on that path.
    // Use the provider/remote we just authorised — the authoritative values for
    // this flow — rather than config's cloud_provider/cloud_remote, which may be
    // stale or blank if the user hasn't committed the settings change yet.
    let cloud_snapshot = {
        let cfg = app.state::<crate::config::SharedConfig>();
        cfg.lock().ok().map(|g| (
            g.data.cloud.rclone_args.clone(),
            base_path(&g.data),
        ))
    };
    if let Some((rclone_args, base)) = cloud_snapshot {
        let ludusavi_remote_path = format!("{}/ludusavi-backup", base);
        crate::ludusavi_config::set_cloud(
            Some(&provider),
            Some(remote_name),
            Some(&ludusavi_remote_path),
            None, // rclone path stays as "rclone"; resolved via PATH at run_api spawn time
            Some(&rclone_args),
        )
        .map_err(|e| AppError::Other(format!("failed to write cloud config: {e}")))?;
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

    fn make_remote() -> RcloneRemote {
        RcloneRemote {
            exe: std::path::PathBuf::from("rclone"),
            remote: "Dropbox".into(),
            base: "Spool".into(),
        }
    }

    #[test]
    fn has_schema_rejects_newer_blobs() {
        // DeviceBlob and HistoryBlob now implement HasSchema; schema > store cap is rejected.
        let new_dev = DeviceBlob { schema: 2, ..Default::default() };
        let old_dev = DeviceBlob { schema: 1, ..Default::default() };
        assert!(new_dev.stored_schema() > 1, "schema 2 must be filtered by a schema-1 store");
        assert!(old_dev.stored_schema() <= 1, "schema 1 must pass a schema-1 store");

        let new_hist = HistoryBlob { schema: 3, ..Default::default() };
        let old_hist = HistoryBlob { schema: 1, ..Default::default() };
        assert!(new_hist.stored_schema() > 1);
        assert!(old_hist.stored_schema() <= 1);
    }

    #[test]
    fn blob_store_paths_are_correct() {
        let remote = make_remote();
        let store = remote.store("custom-saves", 1);
        assert_eq!(store.dir(), "Dropbox:Spool/_spool/custom-saves");
        assert_eq!(store.target("abc123"), "Dropbox:Spool/_spool/custom-saves/abc123.json");
    }

    #[test]
    fn blob_store_base_trailing_slash_stripped() {
        let remote = RcloneRemote {
            exe: std::path::PathBuf::from("rclone"),
            remote: "GDrive".into(),
            base: "My/Saves/".into(),
        };
        let store = remote.store("sessions", 0);
        assert!(store.dir().starts_with("GDrive:My/Saves/_spool/sessions"));
        assert!(!store.dir().contains("//"));
    }

    #[test]
    fn blob_store_different_subdirs_are_independent() {
        let remote = make_remote();
        let dev = remote.store("devices", 1);
        let hist = remote.store("history", 1);
        assert_ne!(dev.dir(), hist.dir());
        assert_ne!(dev.target("id1"), hist.target("id1"));
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
    fn fold_takes_newest_backup_as_latest_backer() {
        // The device-blob fold now carries only backup state (the sync badge):
        // the newest backup across devices names the latest backer.
        let mut a = DeviceBlob::default();
        a.backups.insert("Hades".into(), "2026-05-01T00:00:00Z".into());
        let mut b = DeviceBlob { device_name: "Deck".into(), ..Default::default() };
        b.backups.insert("Hades".into(), "2026-05-03T00:00:00Z".into());

        let folded = fold_device_totals(&[("a".into(), a), ("b".into(), b)]);
        let h = folded.get("Hades").unwrap();
        assert_eq!(h.latest_backer.as_deref(), Some("b"), "newest backup wins");
        assert_eq!(h.latest_backer_name.as_deref(), Some("Deck"));
        assert_eq!(h.latest_backup_raw.as_deref(), Some("2026-05-03T00:00:00Z"));
    }

    #[test]
    fn reach_from_code_classifies() {
        // The remote answered: success, or a definite "not found".
        assert_eq!(reach_from_code(Some(0)), Reach::Online);
        assert_eq!(reach_from_code(Some(3)), Reach::Online); // dir not found
        assert_eq!(reach_from_code(Some(4)), Reach::Online); // file not found
        assert_eq!(reach_from_code(Some(9)), Reach::Online); // no files transferred
        // Every other code — temporary/fatal/usage errors, connection failures,
        // signal kills (None) — means we couldn't confirm the remote: Offline.
        assert_eq!(reach_from_code(Some(5)), Reach::Offline);
        assert_eq!(reach_from_code(Some(7)), Reach::Offline);
        assert_eq!(reach_from_code(Some(1)), Reach::Offline);
        assert_eq!(reach_from_code(Some(2)), Reach::Offline);
        assert_eq!(reach_from_code(None), Reach::Offline);
    }

    #[test]
    fn stderr_tail_takes_last_nonempty_line() {
        assert_eq!(stderr_tail(b""), None);
        assert_eq!(stderr_tail(b"   \n  \n"), None);
        assert_eq!(
            stderr_tail(b"warming up\nFailed to cat: directory not found\n").as_deref(),
            Some("Failed to cat: directory not found")
        );
    }

    #[test]
    fn compute_badge_variants() {
        assert_eq!(compute_badge("me", Some("me")), "synced");
        assert_eq!(compute_badge("me", Some("other")), "cloud-newer");
        assert_eq!(compute_badge("me", None), "synced");
    }
}

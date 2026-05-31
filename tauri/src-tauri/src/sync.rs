//! Sync server HTTP client.
//!
//! Talks to the Bun/Hono server at `server/` — register accounts,
//! acquire / release / heartbeat per-game locks, record save backup
//! and restore events, sync last-played + playtime across devices.
//!
//! All endpoints other than `/health` and `/auth/register` use
//! `Authorization: Bearer <api_key>`. Endpoint URLs are composed by
//! joining the user's configured `sync_server_url` with the path.
//!
//! Background: every 30 s, a polling task pings `/health` and emits
//! a `sync:status-changed` event so the chrome cloud icon can tint
//! itself green / red. The polled status is also cached in
//! `SyncStatusState` so a fresh UI mount has something to read
//! immediately without waiting for the next poll.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(30);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const ENDPOINT_TIMEOUT: Duration = Duration::from_secs(8);

/// Reachability state. The frontend renders the chrome cloud icon
/// based on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncReachability {
    /// User hasn't set a URL / API key — nothing to check.
    Unconfigured,
    /// `/health` returned 200 within the timeout window.
    Online,
    /// Network error, DNS failure, non-200, or timeout.
    Offline,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub reachability: SyncReachability,
    /// Server's reported version on the last successful poll, or
    /// `None` if we've never had a green ping.
    pub server_version: Option<String>,
    /// Diagnostic on the last failure (network error, HTTP status,
    /// timeout) so the UI can show "Couldn't reach <host>: …".
    pub error: Option<String>,
    /// Seconds since the last successful poll.
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

/// Tauri-managed wrapper around the current sync status. Updated by
/// the polling task; read by the `current_sync_status` command + the
/// `sync:status-changed` event listener on the frontend.
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

/// Server JSON: `GET /health` response.
#[derive(Debug, Deserialize)]
struct HealthResponse {
    #[allow(dead_code)]
    ok: bool,
    version: Option<String>,
}

/// Composes a request URL from the configured base URL + the given
/// path. Trims trailing slashes from the base so users can paste with
/// or without a slash and either works.
fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{base}/{path}")
}

/// Reads the current sync-server URL + API key from config, returning
/// `Err(Unconfigured)` if either is missing or the toggle is off.
fn config_snapshot(app: &AppHandle) -> Result<(String, String), SyncReachability> {
    let cfg = app.state::<SharedConfig>();
    let g = cfg.lock().map_err(|_| SyncReachability::Offline)?;
    if !g.data.sync_server_enabled {
        return Err(SyncReachability::Unconfigured);
    }
    let url = g.data.sync_server_url.trim().to_string();
    let key = g.data.sync_server_api_key.trim().to_string();
    if url.is_empty() || key.is_empty() {
        return Err(SyncReachability::Unconfigured);
    }
    Ok((url, key))
}

// ── Background polling ──────────────────────────────────────────────────────

/// Kicks off the health-poll task. Called once from `lib.rs::run`'s
/// setup hook. The task runs forever, polling every 30 s.
pub fn spawn_health_poller(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Brief delay so the rest of startup settles first.
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            poll_once(&app).await;
            tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
        }
    });
}

async fn poll_once(app: &AppHandle) {
    let prev = app.state::<SyncStatusState>().snapshot();

    let new_status = match config_snapshot(app) {
        Err(reachability) => SyncStatus {
            reachability,
            ..Default::default()
        },
        Ok((url, _key)) => probe_health(app, &url).await,
    };

    // Mark last-ok timestamp on successful probes so the UI can age
    // out the "online" state when polling fails later.
    if new_status.reachability == SyncReachability::Online {
        app.state::<SyncStatusState>().mark_ok();
    }

    let changed = prev.reachability != new_status.reachability
        || prev.error != new_status.error
        || prev.server_version != new_status.server_version;
    app.state::<SyncStatusState>().set(new_status);
    if changed {
        let snap = app.state::<SyncStatusState>().snapshot();
        if let Err(e) = app.emit("sync:status-changed", &snap) {
            tracing::warn!(error = %e, "failed to emit sync:status-changed");
        }
    }
}

async fn probe_health(app: &AppHandle, base_url: &str) -> SyncStatus {
    let client = (*app.state::<reqwest::Client>()).clone();
    let url = join_url(base_url, "health");
    let resp = match client.get(&url).timeout(HEALTH_TIMEOUT).send().await {
        Ok(r) => r,
        Err(e) => {
            return SyncStatus {
                reachability: SyncReachability::Offline,
                error: Some(format!("{e}")),
                ..Default::default()
            };
        }
    };
    let status = resp.status();
    if !status.is_success() {
        return SyncStatus {
            reachability: SyncReachability::Offline,
            error: Some(format!("server returned {status}")),
            ..Default::default()
        };
    }
    let body: HealthResponse = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            return SyncStatus {
                reachability: SyncReachability::Offline,
                error: Some(format!("parse /health: {e}")),
                ..Default::default()
            };
        }
    };
    SyncStatus {
        reachability: SyncReachability::Online,
        server_version: body.version,
        error: None,
        last_ok_ago_secs: Some(0),
    }
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn current_sync_status(state: State<'_, SyncStatusState>) -> SyncStatus {
    state.snapshot()
}

/// Force an immediate /health poll instead of waiting for the next
/// 30 s tick. Used by the Settings page after the user edits the
/// URL / API key — gives them instant feedback rather than a stale
/// "Offline" badge.
#[tauri::command]
pub async fn refresh_sync_status(app: AppHandle) -> SyncStatus {
    poll_once(&app).await;
    app.state::<SyncStatusState>().snapshot()
}

// ── Endpoint payloads + helpers (used by the run workflow + register flow) ──

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RegisterResponse {
    pub api_key: String,
}

/// Server JSON: `GET /storage` response — connection details for the
/// self-hosted WebDAV save store.
#[derive(Debug, Deserialize)]
struct StorageResponse {
    webdav_url: String,
    username: String,
    password: String,
    #[serde(default)]
    base_path: String,
    #[serde(default)]
    provider: String,
}

/// Turnkey self-hosted save storage: fetch WebDAV credentials from the
/// configured sync server's `/storage` endpoint, then configure ludusavi to use
/// that remote (via `ludusavi cloud set webdav`). One click replaces the manual
/// per-device rclone/SFTP setup.
#[tauri::command]
pub async fn use_server_save_storage(app: AppHandle) -> AppResult<()> {
    // Read sync-server URL + API key from config.
    let (server_url, api_key) = {
        let cfg = app.state::<SharedConfig>();
        let g = cfg.lock().map_err(|_| AppError::LockPoisoned)?;
        if !g.data.sync_server_enabled {
            return Err(AppError::Other("Sync server is not enabled in Settings.".to_string()));
        }
        let url = g.data.sync_server_url.trim().to_string();
        let key = g.data.sync_server_api_key.trim().to_string();
        if url.is_empty() || key.is_empty() {
            return Err(AppError::Other(
                "Set your sync server URL and API key first.".to_string(),
            ));
        }
        (url, key)
    };

    let endpoint = join_url(&server_url, "storage");
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = client
        .get(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(&api_key)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET {endpoint}: {e}")))?;

    let status = resp.status();
    if status.as_u16() == 404 {
        return Err(AppError::Other(
            "This server doesn't have save storage enabled (set WEBDAV_PUBLIC_URL on the server)."
                .to_string(),
        ));
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("storage request failed: {status} {body}")));
    }
    let info: StorageResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("parse storage response: {e}")))?;

    // Configure ludusavi's owned config to use the WebDAV remote.
    let cfg_state = app.state::<SharedConfig>();
    // The server's auth-proxy (`/internal/webdav-auth`) validates the incoming
    // basic-auth password *verbatim* against the account's API key — it does
    // NOT deobscure it. rclone already obscures the password at rest in
    // rclone.conf and reveals it back to plaintext on the wire, so we must hand
    // ludusavi the plaintext key (obscure_password = false). Pre-obscuring here
    // would put a double-wrapped value on the wire and every sync would 401.
    crate::ludusavi::apply_webdav_remote(
        cfg_state.inner(),
        &info.webdav_url,
        &info.username,
        &info.password,
        &info.provider,
        false,
    )
    .await?;

    // Point ludusavi's cloud path at the server-provided base path.
    if !info.base_path.is_empty() {
        crate::ludusavi_config::set_cloud(None, None, Some(&info.base_path), None, None)?;
    }

    // Persist for the settings UI (password is never stored — ludusavi obscures
    // it into rclone.conf). The dedicated `spool-server` provider keeps this
    // distinct from a manually-configured WebDAV remote so the UI shows a clean
    // connected state instead of the editable url/user/pass fields.
    {
        let mut g = cfg_state.lock().map_err(|_| AppError::LockPoisoned)?;
        g.data.cloud_provider = "spool-server".to_string();
        g.data.cloud_webdav_url = info.webdav_url;
        g.data.cloud_webdav_username = info.username;
        if !info.base_path.is_empty() {
            g.data.cloud_path = info.base_path;
        }
        g.save()?;
    }
    Ok(())
}

/// POST /auth/register with the admin secret. Returns the new API
/// key on success.
#[tauri::command]
pub async fn sync_register_account(
    app: AppHandle,
    server_url: String,
    admin_secret: String,
    username: String,
) -> AppResult<String> {
    let client = (*app.state::<reqwest::Client>()).clone();
    let url = join_url(&server_url, "auth/register");
    let resp = client
        .post(&url)
        .timeout(ENDPOINT_TIMEOUT)
        .header("X-Admin-Secret", admin_secret)
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await
        .map_err(|e| AppError::Other(format!("POST {url}: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!(
            "register failed: {status} {body}"
        )));
    }
    let body: RegisterResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("parse register response: {e}")))?;
    Ok(body.api_key)
}

// ── Lock + event API used by the run workflow ──────────────────────────────

/// Outcome of an `acquire_lock` attempt.
///
/// We return `Acquired` even on server errors so a flaky network /
/// offline NAS doesn't prevent the user from playing — matching the
/// C# Spool "Sync server unavailable — launching anyway…" path. The
/// only path that returns `Conflict` is an explicit HTTP 409 with the
/// other device's name in the body.
#[derive(Debug, Clone)]
pub enum AcquireOutcome {
    /// Either we got the lock, sync is disabled, or the server is
    /// unreachable — the workflow should proceed.
    Acquired,
    /// Another device currently holds the lock. The body carries
    /// their device name so we can show a useful error toast.
    Conflict { device_name: String },
}

/// Body shape POSTed to /locks/:game/acquire.
#[derive(Debug, Serialize)]
struct AcquireBody<'a> {
    device_id: &'a str,
    device_name: &'a str,
}

/// 409 response body for an acquire conflict.
#[derive(Debug, Deserialize, Default)]
struct AcquireConflictBody {
    #[serde(default)]
    device_name: Option<String>,
}

/// Asks the sync server to acquire a per-game play lock. Returns
/// `Acquired` on success / disabled / server error; `Conflict` only
/// when another device holds an unexpired lock. Best-effort:
/// network/timeout errors log a warning and resolve as `Acquired`.
pub async fn acquire_lock(app: &AppHandle, game_name: &str) -> AcquireOutcome {
    let Ok((url, key)) = config_snapshot(app) else {
        return AcquireOutcome::Acquired;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return AcquireOutcome::Acquired;
    }
    let endpoint = join_url(&url, &format!("locks/{}/acquire", urlencode(game_name)));
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = match client
        .post(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(&key)
        .json(&AcquireBody {
            device_id: &device_id,
            device_name: &device_name,
        })
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "sync: acquire_lock network error — proceeding without lock");
            return AcquireOutcome::Acquired;
        }
    };
    let status = resp.status();
    if status.is_success() {
        return AcquireOutcome::Acquired;
    }
    if status.as_u16() == 409 {
        let body: AcquireConflictBody = resp.json().await.unwrap_or_default();
        return AcquireOutcome::Conflict {
            device_name: body
                .device_name
                .unwrap_or_else(|| "another device".to_string()),
        };
    }
    tracing::warn!(status = %status, "sync: acquire_lock unexpected status — proceeding without lock");
    AcquireOutcome::Acquired
}

/// Fire-and-forget POST /locks/:game/release. Failures are logged
/// and ignored — the server's stale-lock detection will eventually
/// reclaim if we never released.
///
/// The server identifies which lock to delete by the `X-Device-Id`
/// header (it scopes the delete to `user_id + game_name + device_id`),
/// so this header is REQUIRED — without it the endpoint 400s and the
/// lock is never released, leaving the game "playing" until the stale
/// window elapses.
pub async fn release_lock(app: &AppHandle, game_name: &str) {
    let Ok((url, key)) = config_snapshot(app) else {
        return;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    let client = (*app.state::<reqwest::Client>()).clone();
    release_lock_request(&client, &url, &key, &device_id, &device_name, game_name).await;
}

/// AppHandle-free lock release for the headless `spool --backup` path (the
/// Decky Loader forced-close fallback). In SteamOS Game Mode, Steam SIGKILLs
/// the primary Spool before [`crate::runner`]'s workflow reaches its own
/// `release_lock`, so the play-state lock would otherwise dangle until the
/// server's stale window elapses. This separate `--backup` process re-reads the
/// on-disk config and releases the lock directly as part of the same safety net
/// that re-runs the backup.
///
/// Best-effort and offline-safe: no-op when sync is disabled / unconfigured or
/// the device id is missing. Builds a one-shot reqwest client since this process
/// has no Tauri-managed one.
pub async fn release_lock_headless(cfg: &crate::config::ConfigData, game_name: &str) {
    if !cfg.sync_server_enabled {
        return;
    }
    let url = cfg.sync_server_url.trim();
    let key = cfg.sync_server_api_key.trim();
    let device_id = cfg.device_id.trim();
    if url.is_empty() || key.is_empty() || device_id.is_empty() {
        return;
    }
    let client = reqwest::Client::new();
    release_lock_request(&client, url, key, device_id, cfg.device_name.trim(), game_name).await;
}

/// Shared POST to /locks/:game/release used by both the in-app and headless
/// release paths. The `X-Device-Id` header is REQUIRED — the server scopes the
/// delete to `user_id + game_name + device_id`, so without it the endpoint 400s
/// and the lock is never released. Fire-and-forget: failures are logged.
async fn release_lock_request(
    client: &reqwest::Client,
    url: &str,
    key: &str,
    device_id: &str,
    device_name: &str,
    game_name: &str,
) {
    let endpoint = join_url(url, &format!("locks/{}/release", urlencode(game_name)));
    match client
        .post(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(key)
        .header("X-Device-Id", device_id)
        .header("X-Device-Name", device_name)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => tracing::warn!(status = %resp.status(), "sync: release_lock non-200"),
        Err(e) => tracing::warn!(error = %e, "sync: release_lock failed"),
    }
}

/// Starts a tokio task that pings /locks/:game/heartbeat every 30s
/// so the server knows we're still playing. Returns the JoinHandle —
/// caller `.abort()`s it when the session ends.
pub fn start_heartbeat(app: AppHandle, game_name: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let Ok((url, key)) = config_snapshot(&app) else {
                return; // sync got disabled mid-session — stop pinging
            };
            // The server's heartbeat endpoint scopes the update by
            // `X-Device-Id` (user_id + game_name + device_id) and 400s
            // without it — so the header is REQUIRED to keep the lock
            // fresh. Missing it means `last_heartbeat` never advances and
            // the lock goes stale mid-session.
            let (device_id, device_name) = device_identity(&app);
            if device_id.is_empty() {
                continue;
            }
            let endpoint =
                join_url(&url, &format!("locks/{}/heartbeat", urlencode(&game_name)));
            let client = (*app.state::<reqwest::Client>()).clone();
            if let Err(e) = client
                .post(&endpoint)
                .timeout(ENDPOINT_TIMEOUT)
                .bearer_auth(&key)
                .header("X-Device-Id", &device_id)
                .header("X-Device-Name", &device_name)
                .send()
                .await
            {
                tracing::warn!(error = %e, "sync: heartbeat failed");
            }
        }
    })
}

/// POST /events/:game/backup — records the device that just backed
/// up. Best-effort. Headers include the device identity so the server
/// can attribute the event.
pub async fn record_backup_event(app: &AppHandle, game_name: &str) {
    record_event(app, game_name, "backup").await;
}

/// POST /events/:game/restore.
pub async fn record_restore_event(app: &AppHandle, game_name: &str) {
    record_event(app, game_name, "restore").await;
}

async fn record_event(app: &AppHandle, game_name: &str, kind: &str) {
    let Ok((url, key)) = config_snapshot(app) else {
        return;
    };
    let (device_id, device_name) = device_identity(app);
    if device_id.is_empty() {
        return;
    }
    let endpoint = join_url(&url, &format!("events/{}/{}", urlencode(game_name), kind));
    let client = (*app.state::<reqwest::Client>()).clone();
    if let Err(e) = client
        .post(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(&key)
        .header("X-Device-Id", &device_id)
        .header("X-Device-Name", &device_name)
        .send()
        .await
    {
        tracing::warn!(error = %e, kind, "sync: record_event failed");
    }
}

/// Reads (device_id, device_name) from config. Empty strings when
/// the config lock is poisoned — callers treat that as "skip the
/// sync interaction".
fn device_identity(app: &AppHandle) -> (String, String) {
    // Bind State to a local first so the MutexGuard's borrow has a
    // stable anchor — the borrow checker chokes on the chained
    // `app.state::<T>().lock()` form here.
    let cfg = app.state::<SharedConfig>();
    let guard = match cfg.lock() {
        Ok(g) => g,
        Err(_) => return (String::new(), String::new()),
    };
    (guard.data.device_id.clone(), guard.data.device_name.clone())
}

/// Percent-encodes a path segment so game names with spaces /
/// punctuation survive in the URL. We re-export the `urlencoding`
/// crate's helper rather than depending on `url::form_urlencoded`
/// (already pulled in elsewhere for the LAN client).
fn urlencode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

// ── Cross-device sync queries (latest-backup, last-played, playtime) ───────

#[derive(Debug, Deserialize, Default)]
pub struct LatestBackupResponse {
    #[serde(default)]
    pub found: bool,
    #[serde(default)]
    pub device_id: Option<String>,
    /// Other device's display name. Currently only consumed by future
    /// "Last backup from X on date" tooltip work — kept here so the
    /// JSON deserialization captures it for that follow-up.
    #[serde(default)]
    #[allow(dead_code)]
    pub device_name: Option<String>,
    /// ISO 8601 timestamp. Same future-use story as `device_name`.
    #[serde(default)]
    #[allow(dead_code)]
    pub occurred_at: Option<String>,
}

/// `GET /events/:game/latest-backup`. Returns `None` when sync is
/// disabled, the server is unreachable, or the response can't be
/// parsed — callers treat that as "no badge change".
pub async fn fetch_latest_backup(
    app: &AppHandle,
    game_name: &str,
) -> Option<LatestBackupResponse> {
    let (url, key) = config_snapshot(app).ok()?;
    let endpoint = join_url(&url, &format!("events/{}/latest-backup", urlencode(game_name)));
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = client
        .get(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(&key)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<LatestBackupResponse>().await.ok()
}

#[derive(Debug, Deserialize, Clone)]
pub struct LastPlayedRecord {
    pub game_name: String,
    pub last_played_at: String,
}

/// `GET /last-played` — every game this user has played on any
/// device. Used at startup to backfill our local `last_played_at`
/// for games that other devices have played more recently.
pub async fn fetch_all_last_played(app: &AppHandle) -> Vec<LastPlayedRecord> {
    let Ok((url, key)) = config_snapshot(app) else {
        return Vec::new();
    };
    let endpoint = join_url(&url, "last-played");
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = match client
        .get(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(&key)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return Vec::new(),
    };
    resp.json::<Vec<LastPlayedRecord>>().await.unwrap_or_default()
}

/// `POST /last-played` — push a single record. Best-effort.
pub async fn push_last_played(app: &AppHandle, game_name: &str, last_played_at: &str) {
    let Ok((url, key)) = config_snapshot(app) else {
        return;
    };
    let endpoint = join_url(&url, "last-played");
    let client = (*app.state::<reqwest::Client>()).clone();
    if let Err(e) = client
        .post(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(&key)
        .json(&serde_json::json!({
            "game_name": game_name,
            "last_played_at": last_played_at,
        }))
        .send()
        .await
    {
        tracing::warn!(error = %e, "sync: push_last_played failed");
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PlaytimeRecord {
    pub game_name: String,
    pub total_minutes: i64,
}

/// `GET /playtime` — server's per-game cumulative total across all
/// devices. We take `max(local, server)` so if the server has more
/// (because another device played offline and synced), we adopt it.
pub async fn fetch_all_playtime(app: &AppHandle) -> Vec<PlaytimeRecord> {
    let Ok((url, key)) = config_snapshot(app) else {
        return Vec::new();
    };
    let endpoint = join_url(&url, "playtime");
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = match client
        .get(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(&key)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return Vec::new(),
    };
    resp.json::<Vec<PlaytimeRecord>>().await.unwrap_or_default()
}

/// `POST /playtime/:game` with `{ delta_minutes }`. Server requires
/// a positive integer; non-positive values short-circuit (no point
/// telling the server "the session was 0 minutes").
pub async fn push_playtime_delta(app: &AppHandle, game_name: &str, delta_minutes: i32) {
    if delta_minutes <= 0 {
        return;
    }
    let Ok((url, key)) = config_snapshot(app) else {
        return;
    };
    let endpoint = join_url(&url, &format!("playtime/{}", urlencode(game_name)));
    let client = (*app.state::<reqwest::Client>()).clone();
    if let Err(e) = client
        .post(&endpoint)
        .timeout(ENDPOINT_TIMEOUT)
        .bearer_auth(&key)
        .json(&serde_json::json!({ "delta_minutes": delta_minutes }))
        .send()
        .await
    {
        tracing::warn!(error = %e, "sync: push_playtime_delta failed");
    }
}

// ── Startup sync: merge cross-device state into the local library ──────────

/// One-shot pull of every cross-device data point: last-played
/// timestamps, playtime totals, and per-game latest-backup events.
/// Spawned at startup; runs once, then exits. Updates the library in
/// a single atomic save and emits `library:changed`.
///
/// No-op when sync is disabled / unreachable. The first poll after
/// startup may briefly see "Probing" before this fires — by design,
/// we don't want a slow server to delay the library showing.
pub fn spawn_startup_sync(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Wait a few seconds so the first health poll can resolve —
        // no point firing this against an unreachable server.
        tokio::time::sleep(Duration::from_secs(4)).await;
        run_startup_sync(&app).await;
    });
}

async fn run_startup_sync(app: &AppHandle) {
    if config_snapshot(app).is_err() {
        return;
    }
    let status = app.state::<SyncStatusState>().snapshot();
    if status.reachability != SyncReachability::Online {
        return;
    }

    tracing::info!("sync: pulling cross-device state");

    // Pull everything in parallel.
    let (last_played, playtime) = tokio::join!(
        fetch_all_last_played(app),
        fetch_all_playtime(app),
    );

    // Snapshot the local game names so we can fetch /latest-backup
    // for each shared entry (drop the lock before the I/O).
    let local_names: Vec<(String, String)> = {
        let library = app.state::<crate::library::SharedLibrary>();
        let lib = match library.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        lib.entries
            .iter()
            .map(|e| (e.id.clone(), e.game_name.clone()))
            .collect()
    };

    // Fetch latest-backup per game. Sequential rather than parallel —
    // most users have <100 games and per-request overhead dominates,
    // so a single in-flight at a time keeps the server polite.
    let mut latest_backups: Vec<(String, LatestBackupResponse)> = Vec::new();
    for (_id, name) in &local_names {
        if let Some(info) = fetch_latest_backup(app, name).await {
            if info.found {
                latest_backups.push((name.clone(), info));
            }
        }
    }

    // Now apply everything in a single library save.
    let (device_id, _device_name) = device_identity(app);
    let library = app.state::<crate::library::SharedLibrary>();
    let mut applied = 0usize;
    if let Ok(mut lib) = library.lock() {
        // last-played: take max(local, server)
        for record in &last_played {
            if let Some(entry) = lib
                .entries
                .iter_mut()
                .find(|e| e.game_name == record.game_name)
            {
                let server_time = chrono::DateTime::parse_from_rfc3339(&record.last_played_at)
                    .ok()
                    .map(|d| d.with_timezone(&chrono::Utc));
                if let Some(server_time) = server_time {
                    if entry.last_played_at.map(|t| server_time > t).unwrap_or(true) {
                        entry.last_played_at = Some(server_time);
                        applied += 1;
                    }
                }
            }
        }
        // playtime: take max(local, server)
        for record in &playtime {
            if let Some(entry) = lib
                .entries
                .iter_mut()
                .find(|e| e.game_name == record.game_name)
            {
                let server_mins = record.total_minutes as i32;
                if server_mins > entry.playtime_minutes {
                    entry.playtime_minutes = server_mins;
                    applied += 1;
                }
            }
        }
        // sync badge: derive from latest backup vs our local mtime
        for (name, info) in &latest_backups {
            let badge = compute_badge(&device_id, info);
            if let Some(entry) = lib.entries.iter_mut().find(|e| e.game_name == *name) {
                if entry.sync_badge.as_deref() != Some(badge) {
                    entry.sync_badge = Some(badge.to_string());
                    applied += 1;
                }
            }
        }

        if applied > 0 {
            if let Err(e) = lib.save() {
                tracing::warn!(error = %e, "startup sync: library save failed");
            }
        }
    }

    tracing::info!(applied, "startup sync: done");
    if applied > 0 {
        let _ = app.emit("library:changed", &());
    }
}

/// Maps a latest-backup response into one of "synced" / "cloud-newer"
/// based on whether the most-recent backup came from us. We don't
/// have enough info here to detect `local-newer` (would need to
/// compare against our local `save_last_backed_up_at`); the runner
/// sets that explicitly after a local backup that fails to record
/// against the server.
fn compute_badge(our_device_id: &str, info: &LatestBackupResponse) -> &'static str {
    match info.device_id.as_deref() {
        Some(id) if id == our_device_id => "synced",
        Some(_) => "cloud-newer",
        None => "cloud-newer", // server has info but no device id — treat conservatively
    }
}

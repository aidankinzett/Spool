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

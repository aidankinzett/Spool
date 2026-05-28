//! TorBox debrid client.
//!
//! TorBox is a cloud-side torrent service: you POST a magnet, their
//! servers download it, and you stream the result over HTTPS at line
//! rate. Spool uses it as a download backend when the Browse Games
//! window picks a game from a Hydra feed — POST the magnet, poll
//! until cached, request a per-file download link, stream the bytes
//! to the user's download folder.
//!
//! This module is the HTTP client. It does NOT do the polling loop
//! or the download itself — those live in the download orchestration
//! layer (Phase 4 of the TorBox/Browse work). The functions below
//! are pure request wrappers, exposed as Tauri commands so the
//! frontend can drive each step or test the connection from
//! Settings.
//!
//! API base: `https://api.torbox.app/v1/api`. Auth: `Authorization:
//! Bearer <api_key>` for all endpoints; `request_download_link` ALSO
//! requires the API key in a query param because TorBox returns a
//! signed URL the caller's browser hits directly (legacy pattern).

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const BASE: &str = "https://api.torbox.app/v1/api";
const TORBOX_TIMEOUT: Duration = Duration::from_secs(30);

/// One file inside a TorBox torrent. The `id` is what we hand to
/// `request_download_link`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub id: i32,
    pub name: String,
    pub size: i64,
    #[serde(default)]
    pub short_name: String,
}

/// A torrent in the user's TorBox queue. `download_state` is a free-
/// form string from the API ("downloading", "metaDL", "completed",
/// "stalled", "cached", …). `cached: true` is the cleanest signal
/// that the torrent is ready to stream; `download_present: true`
/// (when present) is the new-API equivalent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Torrent {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub download_state: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub cached: bool,
    #[serde(default)]
    pub download_present: Option<bool>,
    #[serde(default)]
    pub files: Option<Vec<TorrentFile>>,
}

/// Standard TorBox response envelope. `data` is the typed payload on
/// success; `error` / `detail` carry a human-readable message on
/// failure.
#[derive(Debug, Deserialize)]
struct TorBoxResponse<T> {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    error: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct AddTorrentResponse {
    #[allow(dead_code)]
    #[serde(default)]
    hash: Option<String>,
    torrent_id: i32,
}

/// Reads the TorBox API key out of config. Returns `Err` when sync
/// is disabled or the key is empty.
fn api_key(app: &AppHandle) -> AppResult<String> {
    let cfg = app.state::<SharedConfig>();
    let g = cfg.lock().map_err(|_| AppError::LockPoisoned)?;
    if !g.data.torbox_enabled {
        return Err(AppError::Other("TorBox is disabled in Settings".into()));
    }
    let key = g.data.torbox_api_key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::Other("TorBox API key is not set".into()));
    }
    Ok(key)
}

/// Pulls one of the envelope's error strings; falls back to the
/// HTTP status text when the body parses but carries no message.
fn body_error<T>(envelope: TorBoxResponse<T>) -> AppError {
    let msg = envelope
        .error
        .or(envelope.detail)
        .unwrap_or_else(|| "TorBox returned an unknown error".to_string());
    AppError::Other(msg)
}

// ── HTTP wrappers ───────────────────────────────────────────────────────────

/// POST `/torrents/createtorrent` with form data `magnet=<uri>`.
/// Returns the new torrent's id. The server may queue or hit cache
/// instantly; either way the id is what we'll poll on.
pub async fn add_magnet(app: &AppHandle, magnet_uri: &str) -> AppResult<i32> {
    let key = api_key(app)?;
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = client
        .post(format!("{BASE}/torrents/createtorrent"))
        .timeout(TORBOX_TIMEOUT)
        .bearer_auth(&key)
        .form(&[("magnet", magnet_uri)])
        .send()
        .await
        .map_err(|e| AppError::Other(format!("TorBox add magnet: {e}")))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!(
            "TorBox add magnet failed: {text}"
        )));
    }
    let envelope: TorBoxResponse<AddTorrentResponse> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("TorBox add magnet: parse response: {e}")))?;
    if !envelope.success {
        return Err(body_error(envelope));
    }
    envelope
        .data
        .map(|d| d.torrent_id)
        .ok_or_else(|| AppError::Other("TorBox add magnet: empty data".into()))
}

/// GET `/torrents/mylist?id=…&bypass_cache=true`. Always bypasses
/// any TorBox-side cache so we see fresh state during a poll loop.
pub async fn torrent_info(app: &AppHandle, torrent_id: i32) -> AppResult<Torrent> {
    let key = api_key(app)?;
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = client
        .get(format!(
            "{BASE}/torrents/mylist?id={torrent_id}&bypass_cache=true"
        ))
        .timeout(TORBOX_TIMEOUT)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("TorBox torrent_info: {e}")))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!(
            "TorBox torrent_info failed: {text}"
        )));
    }
    let envelope: TorBoxResponse<Torrent> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("TorBox torrent_info parse: {e}")))?;
    if !envelope.success {
        return Err(body_error(envelope));
    }
    envelope
        .data
        .ok_or_else(|| AppError::Other("TorBox torrent_info: empty data".into()))
}

/// GET `/torrents/requestdl?token=…&torrent_id=…&file_id=…`. The
/// returned URL is a signed CDN link the caller streams directly.
/// We pass the API key in the query string as well as the bearer
/// header — that's a TorBox quirk (the signed URL is meant to be
/// embeddable in a browser, where headers aren't available).
pub async fn request_download_link(
    app: &AppHandle,
    torrent_id: i32,
    file_id: i32,
) -> AppResult<String> {
    let key = api_key(app)?;
    let client = (*app.state::<reqwest::Client>()).clone();
    let url = format!(
        "{BASE}/torrents/requestdl?token={token}&torrent_id={torrent_id}&file_id={file_id}",
        token = urlencoding::encode(&key)
    );
    let resp = client
        .get(&url)
        .timeout(TORBOX_TIMEOUT)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("TorBox requestdl: {e}")))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!(
            "TorBox requestdl failed: {text}"
        )));
    }
    let envelope: TorBoxResponse<String> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("TorBox requestdl parse: {e}")))?;
    if !envelope.success {
        return Err(body_error(envelope));
    }
    envelope
        .data
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Other("TorBox requestdl: empty data".into()))
}

/// Quick "is the API key valid?" probe. Hits `/torrents/mylist`
/// without an id (returns the full list, may be empty) and checks
/// for 200. Used by the Settings "Test connection" button.
pub async fn ping(app: &AppHandle) -> AppResult<()> {
    let key = api_key(app)?;
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = client
        .get(format!("{BASE}/torrents/mylist"))
        .timeout(TORBOX_TIMEOUT)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("TorBox ping: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!(
            "TorBox ping failed: {status} {text}"
        )));
    }
    Ok(())
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn torbox_add_magnet(app: AppHandle, magnet_uri: String) -> AppResult<i32> {
    add_magnet(&app, &magnet_uri).await
}

#[tauri::command]
pub async fn torbox_torrent_info(app: AppHandle, torrent_id: i32) -> AppResult<Torrent> {
    torrent_info(&app, torrent_id).await
}

#[tauri::command]
pub async fn torbox_request_download_link(
    app: AppHandle,
    torrent_id: i32,
    file_id: i32,
) -> AppResult<String> {
    request_download_link(&app, torrent_id, file_id).await
}

#[tauri::command]
pub async fn torbox_ping(app: AppHandle) -> AppResult<()> {
    ping(&app).await
}

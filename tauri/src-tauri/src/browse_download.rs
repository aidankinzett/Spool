//! Browse Games download orchestrator.
//!
//! Drives the actual file transfer when the user clicks Download in
//! the Browse Games window. Two backends:
//!
//!   - **TorBox** (`magnet:` URIs): POST the magnet, poll
//!     `torrent_info` until cached, request a signed URL per file,
//!     stream the bytes to `download_dir`.
//!   - **Direct HTTP** (`http://` / `https://` URIs): straight
//!     reqwest stream into `download_dir`.
//!
//! Single in-flight slot — same UX model as `LanDownloadState`. The
//! `BrowseDownloadState` carries the current progress + cancel flag
//! and is exposed to the frontend via `current_browse_download` and
//! `cancel_browse_download` commands.
//!
//! Emits `browse:download` events on every progress tick (throttled
//! to ~5 Hz) and on each terminal status change. The Browse window
//! listens for these to render in-window progress; future polish can
//! also surface them in the central Transfers panel.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::hydra::HydraEntry;
use crate::library::make_safe_filename;
use crate::torbox;
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;

/// Emit cap — same 200 ms cadence as the LAN install so the UI
/// frequency is consistent across surfaces.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
/// Cap on how long we'll wait between polls of /torrents/mylist
/// before giving up. TorBox typically caches within seconds for
/// known torrents; minutes for fresh ones.
const TORBOX_POLL_INTERVAL: Duration = Duration::from_secs(5);
const TORBOX_POLL_TIMEOUT: Duration = Duration::from_secs(60 * 30); // 30 min

#[derive(Debug, Clone, Serialize)]
pub struct BrowseDownloadProgress {
    pub install_token: String,
    /// "torbox" | "direct"
    pub source_kind: String,
    /// Display label — "TorBox · debrid" | host of the URL.
    pub source_name: String,
    pub game_name: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: String,
    /// "starting" | "queuing" | "downloading" | "done" | "error" | "canceled"
    pub status: String,
    pub message: Option<String>,
    /// Set on `done` — the user-facing path of the saved file (or
    /// containing folder for multi-file torrents).
    pub dest_path: Option<String>,
}

#[derive(Default)]
pub struct BrowseDownloadState {
    current: Mutex<Option<BrowseDownloadProgress>>,
    cancel_flag: AtomicBool,
    start_instant: Mutex<Option<Instant>>,
}

const CANCELED_MSG: &str = "canceled by user";

impl BrowseDownloadState {
    fn try_start(&self, p: BrowseDownloadProgress) -> AppResult<()> {
        let mut guard = self.current.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(existing) = guard.as_ref() {
            if matches!(existing.status.as_str(), "starting" | "queuing" | "downloading") {
                return Err(AppError::Other(
                    "Another browse download is already in progress".into(),
                ));
            }
        }
        self.cancel_flag.store(false, Ordering::Relaxed);
        if let Ok(mut g) = self.start_instant.lock() {
            *g = Some(Instant::now());
        }
        *guard = Some(p);
        Ok(())
    }

    fn snapshot(&self) -> Option<BrowseDownloadProgress> {
        self.current.lock().ok().and_then(|g| g.clone())
    }

    fn update<F: FnOnce(&mut BrowseDownloadProgress)>(
        &self,
        f: F,
    ) -> Option<BrowseDownloadProgress> {
        let mut guard = self.current.lock().ok()?;
        let p = guard.as_mut()?;
        f(p);
        Some(p.clone())
    }

    fn set(&self, value: Option<BrowseDownloadProgress>) {
        if let Ok(mut g) = self.current.lock() {
            *g = value;
        }
    }

    fn is_canceled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }

    fn request_cancel(&self, token: &str) -> bool {
        let g = match self.current.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match g.as_ref() {
            Some(p) if p.install_token == token => {
                self.cancel_flag.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }
}

fn emit(app: &AppHandle, progress: &BrowseDownloadProgress) {
    if let Err(e) = app.emit("browse:download", progress) {
        tracing::warn!(error = %e, "failed to emit browse:download");
    }
}

/// Resolves the destination dir. Honours `config.download_dir`;
/// falls back to the OS Downloads folder, or the current dir as a
/// last resort.
fn resolve_dest_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let cfg = app.state::<SharedConfig>();
    let configured = {
        let g = cfg.lock().map_err(|_| AppError::LockPoisoned)?;
        g.data.download_dir.trim().to_string()
    };
    if !configured.is_empty() {
        return Ok(PathBuf::from(configured));
    }
    Ok(dirs::download_dir()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from(".")))
}

fn uri_kind(uri: &str) -> &'static str {
    if uri.starts_with("magnet:") {
        "magnet"
    } else if uri.starts_with("http://") || uri.starts_with("https://") {
        "http"
    } else {
        "unknown"
    }
}

/// Picks the best URI from a Hydra entry. Magnet beats HTTP when
/// TorBox is configured; otherwise HTTP wins (we can't do anything
/// with a magnet without a debrid backend).
fn pick_uri(app: &AppHandle, entry: &HydraEntry) -> Option<(String, &'static str)> {
    let cfg = app.state::<SharedConfig>();
    let torbox_on = cfg
        .lock()
        .map(|g| g.data.torbox_enabled && !g.data.torbox_api_key.trim().is_empty())
        .unwrap_or(false);

    if torbox_on {
        if let Some(m) = entry.uris.iter().find(|u| uri_kind(u) == "magnet") {
            return Some((m.clone(), "magnet"));
        }
    }
    if let Some(h) = entry.uris.iter().find(|u| uri_kind(u) == "http") {
        return Some((h.clone(), "http"));
    }
    if !torbox_on {
        if let Some(m) = entry.uris.iter().find(|u| uri_kind(u) == "magnet") {
            // We CAN add a magnet without TorBox via a real torrent
            // client, but Spool isn't one. Surface as unsupported.
            return Some((m.clone(), "magnet-no-backend"));
        }
    }
    None
}

// ── Top-level orchestrator ─────────────────────────────────────────────────

#[tauri::command]
pub async fn start_browse_download(
    app: AppHandle,
    state: State<'_, BrowseDownloadState>,
    entry: HydraEntry,
) -> AppResult<String> {
    let (uri, kind) = pick_uri(&app, &entry).ok_or_else(|| {
        AppError::Other(format!(
            "{} has no supported URI in this feed",
            entry.title
        ))
    })?;
    if kind == "magnet-no-backend" {
        return Err(AppError::Other(
            "This release is a magnet link — enable TorBox in Settings → Sources & Downloads to use it".into(),
        ));
    }

    let install_token = uuid::Uuid::new_v4().to_string();
    let return_token = install_token.clone();

    let (source_kind_label, source_display) = match kind {
        "magnet" => ("torbox", "TorBox · debrid".to_string()),
        "http" => {
            let host = url_host(&uri).unwrap_or_else(|| "direct".to_string());
            ("direct", format!("Direct · {host}"))
        }
        _ => unreachable!(),
    };

    let progress = BrowseDownloadProgress {
        install_token: install_token.clone(),
        source_kind: source_kind_label.to_string(),
        source_name: source_display.clone(),
        game_name: entry.title.clone(),
        bytes_done: 0,
        bytes_total: 0,
        current_file: String::new(),
        status: "starting".into(),
        message: None,
        dest_path: None,
    };
    state.try_start(progress.clone())?;
    emit(&app, &progress);

    let dest_root = resolve_dest_dir(&app)?;
    let safe = make_safe_filename(&entry.title);
    let dest_dir = if kind == "magnet" {
        dest_root.join(&safe) // multi-file: own folder
    } else {
        dest_root.clone()
    };
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| AppError::Other(format!("create dest dir: {e}")))?;

    let app_clone = app.clone();
    let token_for_task = install_token.clone();
    let display_for_task = source_display.clone();
    let kind_label_for_task = source_kind_label.to_string();
    tauri::async_runtime::spawn(async move {
        let result = match kind {
            "magnet" => run_torbox(&app_clone, &token_for_task, &uri, &entry, &dest_dir).await,
            "http" => run_direct(&app_clone, &token_for_task, &uri, &entry, &dest_dir).await,
            _ => Err(AppError::Other("unsupported URI kind".into())),
        };

        let final_progress = match result {
            Ok(final_path) => BrowseDownloadProgress {
                install_token: token_for_task.clone(),
                source_kind: kind_label_for_task,
                source_name: display_for_task,
                game_name: entry.title.clone(),
                bytes_done: 0,
                bytes_total: 0,
                current_file: String::new(),
                status: "done".into(),
                message: None,
                dest_path: Some(final_path.to_string_lossy().to_string()),
            },
            Err(e) => {
                let msg = e.to_string();
                let status = if msg == CANCELED_MSG { "canceled" } else { "error" };
                BrowseDownloadProgress {
                    install_token: token_for_task.clone(),
                    source_kind: kind_label_for_task,
                    source_name: display_for_task,
                    game_name: entry.title.clone(),
                    bytes_done: 0,
                    bytes_total: 0,
                    current_file: String::new(),
                    status: status.to_string(),
                    message: Some(msg),
                    dest_path: None,
                }
            }
        };
        app_clone
            .state::<BrowseDownloadState>()
            .set(Some(final_progress.clone()));
        emit(&app_clone, &final_progress);

        // 2 s grace then clear the slot.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let state = app_clone.state::<BrowseDownloadState>();
        if let Some(p) = state.snapshot() {
            if p.install_token == token_for_task {
                state.set(None);
            }
        }
    });

    Ok(return_token)
}

#[tauri::command]
pub fn cancel_browse_download(
    state: State<'_, BrowseDownloadState>,
    install_token: String,
) -> bool {
    state.request_cancel(&install_token)
}

#[tauri::command]
pub fn current_browse_download(
    state: State<'_, BrowseDownloadState>,
) -> Option<BrowseDownloadProgress> {
    state.snapshot()
}

// ── Direct HTTP backend ─────────────────────────────────────────────────────

async fn run_direct(
    app: &AppHandle,
    _token: &str,
    url: &str,
    entry: &HydraEntry,
    dest_dir: &std::path::Path,
) -> AppResult<PathBuf> {
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "{url} returned {}",
            resp.status()
        )));
    }

    let filename = guess_filename(&resp, url, &entry.title);
    let dest = dest_dir.join(&filename);
    let total = resp.content_length().unwrap_or(0);

    app.state::<BrowseDownloadState>().update(|p| {
        p.status = "downloading".into();
        p.current_file = filename.clone();
        p.bytes_total = total;
    });
    emit(app, &app.state::<BrowseDownloadState>().snapshot().unwrap());

    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| AppError::Other(format!("create {dest:?}: {e}")))?;
    let mut bytes_done: u64 = 0;
    let mut last_emit = Instant::now() - PROGRESS_EMIT_INTERVAL * 2;

    while let Some(chunk) = stream.next().await {
        if app.state::<BrowseDownloadState>().is_canceled() {
            drop(file);
            let _ = tokio::fs::remove_file(&dest).await;
            return Err(AppError::Other(CANCELED_MSG.into()));
        }
        let chunk = chunk.map_err(|e| AppError::Other(format!("recv chunk: {e}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| AppError::Other(format!("write {dest:?}: {e}")))?;
        bytes_done += chunk.len() as u64;
        if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
            let _ = app.state::<BrowseDownloadState>().update(|p| {
                p.bytes_done = bytes_done;
            });
            if let Some(s) = app.state::<BrowseDownloadState>().snapshot() {
                emit(app, &s);
            }
            last_emit = Instant::now();
        }
    }
    file.flush()
        .await
        .map_err(|e| AppError::Other(format!("flush {dest:?}: {e}")))?;
    drop(file);
    Ok(dest)
}

// ── TorBox backend ─────────────────────────────────────────────────────────

async fn run_torbox(
    app: &AppHandle,
    _token: &str,
    magnet_uri: &str,
    _entry: &HydraEntry,
    dest_dir: &std::path::Path,
) -> AppResult<PathBuf> {
    // Submit magnet — TorBox returns the torrent id immediately
    // whether it's already cached or just queued for download.
    app.state::<BrowseDownloadState>().update(|p| {
        p.status = "queuing".into();
        p.current_file = "Adding to TorBox…".into();
    });
    if let Some(s) = app.state::<BrowseDownloadState>().snapshot() {
        emit(app, &s);
    }
    let torrent_id = torbox::add_magnet(app, magnet_uri).await?;

    // Poll until cached / download_present. Bail after 30 min total.
    let poll_start = Instant::now();
    let info = loop {
        if app.state::<BrowseDownloadState>().is_canceled() {
            return Err(AppError::Other(CANCELED_MSG.into()));
        }
        if poll_start.elapsed() > TORBOX_POLL_TIMEOUT {
            return Err(AppError::Other(
                "TorBox didn't cache the torrent within 30 min".into(),
            ));
        }
        let info = torbox::torrent_info(app, torrent_id).await?;
        let ready = info.cached || info.download_present.unwrap_or(false);
        // Update the queuing progress with TorBox's own % so the user
        // can see it's not stuck.
        let pct = (info.progress.clamp(0.0, 1.0) * 100.0).round() as u64;
        app.state::<BrowseDownloadState>().update(|p| {
            p.current_file = format!("TorBox preparing · {pct}%");
        });
        if let Some(s) = app.state::<BrowseDownloadState>().snapshot() {
            emit(app, &s);
        }
        if ready {
            break info;
        }
        tokio::time::sleep(TORBOX_POLL_INTERVAL).await;
    };

    let files = info
        .files
        .clone()
        .ok_or_else(|| AppError::Other("TorBox torrent has no files".into()))?;
    if files.is_empty() {
        return Err(AppError::Other("TorBox torrent has no files".into()));
    }

    let total_bytes: u64 = files.iter().map(|f| f.size.max(0) as u64).sum();
    app.state::<BrowseDownloadState>().update(|p| {
        p.status = "downloading".into();
        p.bytes_total = total_bytes;
        p.bytes_done = 0;
    });

    let client = (*app.state::<reqwest::Client>()).clone();
    let mut total_done: u64 = 0;
    let mut last_emit = Instant::now() - PROGRESS_EMIT_INTERVAL * 2;

    for file in files {
        if app.state::<BrowseDownloadState>().is_canceled() {
            return Err(AppError::Other(CANCELED_MSG.into()));
        }
        let url = torbox::request_download_link(app, torrent_id, file.id).await?;
        // The TorBox file.name is the full path inside the torrent —
        // sanitise to avoid path traversal even though the API is
        // trusted.
        let safe_rel = file
            .name
            .replace('\\', "/")
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".." && *s != ".")
            .collect::<Vec<_>>()
            .join("/");
        let target = dest_dir.join(safe_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Other(format!("mkdir {parent:?}: {e}")))?;
        }

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Other(format!("GET {url}: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Other(format!(
                "TorBox CDN returned {} for {}",
                resp.status(),
                file.name
            )));
        }

        let mut out = tokio::fs::File::create(&target)
            .await
            .map_err(|e| AppError::Other(format!("create {target:?}: {e}")))?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if app.state::<BrowseDownloadState>().is_canceled() {
                drop(out);
                return Err(AppError::Other(CANCELED_MSG.into()));
            }
            let chunk = chunk.map_err(|e| AppError::Other(format!("recv chunk: {e}")))?;
            out.write_all(&chunk)
                .await
                .map_err(|e| AppError::Other(format!("write {target:?}: {e}")))?;
            total_done += chunk.len() as u64;
            if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                let _ = app.state::<BrowseDownloadState>().update(|p| {
                    p.bytes_done = total_done;
                    p.current_file = file.name.clone();
                });
                if let Some(s) = app.state::<BrowseDownloadState>().snapshot() {
                    emit(app, &s);
                }
                last_emit = Instant::now();
            }
        }
        out.flush()
            .await
            .map_err(|e| AppError::Other(format!("flush {target:?}: {e}")))?;
    }

    // For multi-file torrents the "dest" we want to surface is the
    // containing dir we created up front.
    Ok(dest_dir.to_path_buf())
}

fn guess_filename(
    resp: &reqwest::Response,
    url: &str,
    fallback_title: &str,
) -> String {
    if let Some(disp) = resp.headers().get(reqwest::header::CONTENT_DISPOSITION) {
        if let Ok(s) = disp.to_str() {
            if let Some(filename) = parse_content_disposition_filename(s) {
                return filename;
            }
        }
    }
    let path = url.split('?').next().unwrap_or(url);
    if let Some(last) = path.rsplit('/').next() {
        if !last.is_empty() {
            return urlencoding::decode(last)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| last.to_string());
        }
    }
    format!("{}.bin", make_safe_filename(fallback_title))
}

/// Crude `filename="…"` extraction. Doesn't handle RFC 5987 extended
/// encoding; sufficient for the common case where Hydra-listed direct
/// links use plain ASCII filenames.
fn parse_content_disposition_filename(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let idx = lower.find("filename=")?;
    let after = &s[idx + "filename=".len()..];
    let trimmed = after.trim_start();
    if let Some(stripped) = trimmed.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(stripped[..end].to_string())
    } else {
        // Unquoted — stop at `;` or end.
        let end = trimmed.find(';').unwrap_or(trimmed.len());
        Some(trimmed[..end].trim().to_string())
    }
}

fn url_host(url: &str) -> Option<String> {
    let without_scheme = url.split("://").nth(1)?;
    let host_with_rest = without_scheme.split('/').next()?;
    Some(host_with_rest.split(':').next()?.to_string())
}

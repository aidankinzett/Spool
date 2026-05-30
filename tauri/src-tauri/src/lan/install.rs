//! The receiving half of LAN transfer: browse a peer's catalogue, fetch
//! a manifest, stream every file to a `.partial` staging dir with blake3
//! verification + HTTP-range resume, then rename into place and register
//! a new library entry. Single in-flight install slot with a cooperative
//! cancel flag.

use super::{PeerFile, PeerGame, PeerGameManifest};
use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::{make_safe_filename, GameEntry, SharedLibrary};
use crate::paths;
use chrono::Utc;
use futures_util::StreamExt;
use reqwest::header;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Cap on how long an outgoing peer-games fetch is allowed to take. Peers
/// on the same LAN should respond in milliseconds; anything past this is
/// almost certainly a dropped peer or a firewall hole.
const PEER_FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// Cap on the manifest fetch. The host's `/manifest` handler walks the
/// game folder and blake3-hashes every byte on the first request
/// (~1 s/GB on modern hardware), so the snappy `PEER_FETCH_TIMEOUT` is
/// far too short — a multi-GB game routinely takes longer than 5 s to
/// hash. Five minutes covers ~300 GB at 1 GB/s while still bounding a
/// wedged peer.
const MANIFEST_FETCH_TIMEOUT: Duration = Duration::from_secs(300);
/// How many files to stream from a peer at once. 4 is a sweet spot:
/// enough to keep gigabit pipes full when games are full of tiny files,
/// few enough that a peer's HTTP server (or a residential router) isn't
/// drowning in concurrent sockets.
const LAN_PARALLEL_FILES: usize = 4;
/// Minimum gap between `lan:download` event emissions. The download
/// loop fires every chunk; without throttling that's hundreds of
/// events per second on a fast transfer.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
/// Maximum number of times a single file download is retried on
/// transient network errors (connection drop, timeout, body read
/// failure). Each retry uses the existing partial file as a resume
/// point and waits 2^(attempt-1) seconds before re-connecting.
const MAX_DOWNLOAD_RETRIES: u32 = 5;
/// Base delay for retry backoff: attempt 1 → 2 s, 2 → 4 s, 3 → 8 s …
const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

/// Snapshot of an in-flight (or just-finished) peer install. Emitted as
/// `lan:download` events and also held in `LanDownloadState` so the UI
/// can pick up mid-transfer on a late mount.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub install_token: String,
    pub source_device_id: String,
    pub source_device_name: String,
    pub source_game_id: String,
    pub game_name: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: String,
    pub status: String, // "starting" | "transferring" | "done" | "error" | "canceled"
    pub message: Option<String>,
    /// Set when status == "done": the id of the freshly-created
    /// library entry so the UI can jump straight to it.
    pub new_game_id: Option<String>,
    /// Average download throughput in bytes per second since the
    /// install started. Set by `LanDownloadState::update` after the
    /// caller's mutation runs. 0 during the first half-second so
    /// the UI doesn't flash a silly "9999 GB/s" off the first chunk.
    pub bytes_per_second: f64,
    /// Local path to the peer-supplied cover image, prefetched in the
    /// background once the manifest lands so the transfer-panel row
    /// has a thumbnail to render before the install completes and a
    /// library entry exists. `None` until the prefetch lands; stays
    /// `None` if the peer 404s its `/cover` endpoint.
    pub cover_image_path: Option<String>,
}

/// Single-slot in-flight install tracker. Same model as `RunState` —
/// one transfer at a time keeps the UX (and bandwidth) predictable,
/// and the next phase can lift this to a HashMap if multi-download
/// becomes a real ask.
///
/// The `cancel_flag` lets the user abort an in-flight install. The
/// download loop polls it between chunks and between files, so cancel
/// is cooperative — the partial dir gets cleaned up on the way out
/// rather than left as orphan junk.
#[derive(Default)]
pub struct LanDownloadState {
    current: Mutex<Option<DownloadProgress>>,
    cancel_flag: AtomicBool,
    /// Set by the heartbeat task when the source returned 410 Gone
    /// from `/cancel-check`. Tells the chunk loop to surface
    /// `AppError::HostCanceled` rather than the generic `Canceled`,
    /// so we can log + display the right reason. Always implies
    /// `cancel_flag` is also set.
    host_cancel_flag: AtomicBool,
    /// Wall-clock anchor for computing `bytes_per_second`. Set in
    /// `try_start`, cleared (implicitly) when a new install replaces
    /// it. Stored separately from `current` because `Instant` isn't
    /// serializable and doesn't belong in the wire-format DTO.
    start_instant: Mutex<Option<Instant>>,
}

impl LanDownloadState {
    fn try_start(&self, p: DownloadProgress) -> AppResult<DownloadGuard<'_>> {
        let mut guard = self.current.lock().map_err(|_| AppError::LockPoisoned)?;
        if guard.is_some() {
            return Err(AppError::Other(
                "Another LAN install is already in progress".into(),
            ));
        }
        // Reset the cancel flags for the fresh install — any lingering
        // `true` from a previous cancelled run would otherwise abort us
        // immediately.
        self.cancel_flag.store(false, Ordering::Relaxed);
        self.host_cancel_flag.store(false, Ordering::Relaxed);
        // Anchor the throughput clock here, not at command-receive
        // time, so the first few hundred ms of manifest-fetch don't
        // skew the average down.
        if let Ok(mut g) = self.start_instant.lock() {
            *g = Some(Instant::now());
        }
        *guard = Some(p);
        Ok(DownloadGuard { state: self })
    }

    /// Marks the current install as cancelled iff `token` matches. The
    /// download loop will notice on its next poll and abort cleanly.
    /// Returns true if a cancel was actually requested (token matched
    /// an in-flight install).
    fn request_cancel(&self, token: &str) -> bool {
        let guard = match self.current.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match guard.as_ref() {
            Some(p) if p.install_token == token => {
                self.cancel_flag.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    fn is_canceled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }

    /// Signals "cancelled by host" — sets both flags so the chunk
    /// loop bails on its next poll and the eventual error variant
    /// reflects who initiated the cancel.
    fn request_host_cancel(&self) {
        self.host_cancel_flag.store(true, Ordering::Relaxed);
        self.cancel_flag.store(true, Ordering::Relaxed);
    }

    /// Returns the right `AppError` variant for the current cancel
    /// state — `HostCanceled` if the heartbeat detected a remote
    /// kick, otherwise `Canceled` for user-initiated.
    fn cancel_error(&self) -> AppError {
        if self.host_cancel_flag.load(Ordering::Relaxed) {
            AppError::HostCanceled
        } else {
            AppError::Canceled
        }
    }

    /// Computes how long the caller should sleep to drag the
    /// aggregate transfer rate back under `max_bps`. Each parallel
    /// file task shares the same `bytes_done` atomic and the same
    /// `start_instant` anchor, so they collectively converge on the
    /// cap.
    ///
    /// Returns `None` when no throttling is needed (rate is under
    /// the cap, no cap configured, or first 100 ms of the install
    /// where the average is noisy). Sleep is capped at 500 ms so
    /// cancellation stays responsive.
    pub fn throttle_required(&self, bytes_done: u64, max_bps: f64) -> Option<Duration> {
        if max_bps <= 0.0 {
            return None;
        }
        let start = self.start_instant.lock().ok()?.as_ref().copied()?;
        let actual_secs = start.elapsed().as_secs_f64();
        if actual_secs < 0.1 {
            return None;
        }
        let bd = bytes_done as f64;
        if bd / actual_secs <= max_bps {
            return None;
        }
        let target_secs = bd / max_bps;
        let sleep_secs = (target_secs - actual_secs).min(0.5);
        if sleep_secs <= 0.0 {
            return None;
        }
        Some(Duration::from_millis((sleep_secs * 1000.0) as u64))
    }

    fn snapshot(&self) -> Option<DownloadProgress> {
        self.current.lock().ok().and_then(|g| g.clone())
    }

    fn update<F: FnOnce(&mut DownloadProgress)>(&self, f: F) -> Option<DownloadProgress> {
        let mut guard = self.current.lock().ok()?;
        if let Some(p) = guard.as_mut() {
            f(p);
            // Refresh derived throughput after the caller's mutation
            // so callers don't have to remember to set it. Suppress
            // the value for the first half-second — a single 64 KB
            // chunk in 5 ms otherwise reads as "13 MB/s" before the
            // average smooths out.
            if let Ok(start_g) = self.start_instant.lock() {
                if let Some(start) = *start_g {
                    let elapsed = start.elapsed().as_secs_f64();
                    if elapsed > 0.5 {
                        p.bytes_per_second = (p.bytes_done as f64) / elapsed;
                    }
                }
            }
            return Some(p.clone());
        }
        None
    }

    /// Overwrite the slot wholesale. Used by the install task to publish
    /// the final "done" / "error" state. Wrapped as a method so callers
    /// don't have to touch the private `current` field across a State
    /// deref (which the borrow checker objects to when the State is a
    /// temporary).
    fn set(&self, value: Option<DownloadProgress>) {
        if let Ok(mut g) = self.current.lock() {
            *g = value;
        }
    }

    /// Clear the slot iff the in-flight install matches `token`. The
    /// guard against clearing the wrong install protects the case where
    /// the user kicked off a second install during the 2 s grace period
    /// after the first one finished.
    fn clear_if_token(&self, token: &str) {
        if let Ok(mut g) = self.current.lock() {
            if let Some(p) = g.as_ref() {
                if p.install_token == token {
                    *g = None;
                }
            }
        }
    }
}

/// RAII guard — clears the slot when the install task ends, even if it
/// panics. Mirrors `runner::RunGuard`. Without this a crashed transfer
/// would jam the slot until restart.
struct DownloadGuard<'a> {
    state: &'a LanDownloadState,
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut g) = self.state.current.lock() {
            *g = None;
        }
    }
}

/// Resolves where new LAN installs land. Defaults to
/// `<app_data>/lan-games` when the user hasn't set `lan_install_dir`
/// in config — matches the convention of every other Spool path.
fn install_root_from(app: &AppHandle) -> AppResult<PathBuf> {
    let config = app.state::<SharedConfig>();
    let configured = {
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        cfg.data.lan_install_dir.clone()
    };
    if configured.is_empty() {
        Ok(paths::app_data_dir().join("lan-games"))
    } else {
        Ok(PathBuf::from(configured))
    }
}

/// Picks an install directory inside the LAN root that doesn't collide
/// with an existing install. Adds `" (2)"`, `" (3)"` etc. as needed.
fn allocate_install_dir(root: &Path, safe_name: &str) -> PathBuf {
    let base = if safe_name.is_empty() {
        "Game".to_string()
    } else {
        make_safe_filename(safe_name)
    };
    let first = root.join(&base);
    if !first.exists() {
        return first;
    }
    for n in 2u32..=999 {
        let candidate = root.join(format!("{base} ({n})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // Pathological collision — append timestamp.
    root.join(format!("{base}-{}", Utc::now().timestamp()))
}

/// Resolves the `(final_dir, partial_dir, resuming)` triple for a new
/// install. If a `<base>.partial` directory already exists from a
/// previous interrupted attempt — and the would-be final dir is still
/// free — we resume into it. Otherwise allocate a fresh non-colliding
/// pair.
///
/// We deliberately only check the *preferred* base name (no scanning
/// for `Name (2).partial`, `Name (3).partial`, …): keeping the rule
/// simple means a user who genuinely wants a fresh install can get
/// one by deleting the leftover `.partial` folder.
fn resolve_install_dirs(root: &Path, safe_name: &str) -> (PathBuf, PathBuf, bool) {
    let base = if safe_name.is_empty() {
        "Game".to_string()
    } else {
        make_safe_filename(safe_name)
    };
    let preferred_final = root.join(&base);
    let preferred_partial = root.join(format!("{base}.partial"));
    if preferred_partial.is_dir() && !preferred_final.exists() {
        return (preferred_final, preferred_partial, true);
    }
    let final_dir = allocate_install_dir(root, safe_name);
    let partial = final_dir.with_extension("partial");
    (final_dir, partial, false)
}

/// Formats a reqwest error including its full cause chain. reqwest's own
/// Display only shows the top-level wrapper ("error sending request for
/// url (...)"), which hides whether the underlying problem was a timeout,
/// connection refusal, or TCP reset — exactly the detail you need to
/// debug a LAN download failure.
fn format_reqwest_error(e: &reqwest::Error) -> String {
    let mut out = e.to_string();
    let mut src: Option<&dyn std::error::Error> = std::error::Error::source(e);
    while let Some(s) = src {
        out.push_str(": ");
        out.push_str(&s.to_string());
        src = s.source();
    }
    out
}

fn emit_progress(app: &AppHandle, progress: &DownloadProgress) {
    if let Err(e) = app.emit("lan:download", progress) {
        tracing::warn!(error = %e, "failed to emit lan:download");
    }
}

/// Fetches the game catalogue from a peer's HTTP server. Frontend calls
/// this when the user opens a peer's row in the LAN popover. Times out
/// quickly so a stale peer in the registry can't hang the UI.
#[tauri::command]
pub async fn fetch_peer_games(app: AppHandle, addr: String, port: u16) -> AppResult<Vec<PeerGame>> {
    if port == 0 {
        return Err(AppError::Other(
            "peer is discovery-only (no file server)".into(),
        ));
    }
    let url = format!("http://{addr}:{port}/games");
    let resp = app
        .state::<reqwest::Client>()
        .get(&url)
        .timeout(PEER_FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "peer responded {} to /games",
            resp.status()
        )));
    }
    resp.json::<Vec<PeerGame>>()
        .await
        .map_err(|e| AppError::Other(format!("parse peer /games: {e}")))
}

/// Snapshot of the active LAN install (if any). The frontend uses this
/// on mount to catch up after a navigation that lost in-memory state —
/// otherwise it tracks live via the `lan:download` event stream.
#[tauri::command]
pub fn current_peer_download(state: State<'_, LanDownloadState>) -> Option<DownloadProgress> {
    state.snapshot()
}

/// Requests cancellation of an in-flight install. The download task
/// polls the cancel flag between chunks, cleans up its `.partial` dir,
/// then emits a final `lan:download` with `status: "canceled"`. Returns
/// `true` if the token matched an active install, `false` if there was
/// nothing to cancel (no in-flight transfer, or different token).
#[tauri::command]
pub fn cancel_peer_install(state: State<'_, LanDownloadState>, install_token: String) -> bool {
    state.request_cancel(&install_token)
}

/// Kicks off a peer install. Acquires the single-slot guard, fetches
/// the manifest, streams every file to a `.partial` staging dir, then
/// renames into place and registers a new library entry. Progress is
/// emitted continuously as `lan:download` events.
///
/// Returns the install_token (uuid) once the transfer has been queued —
/// the heavy work runs in a spawned task so the command returns
/// immediately and the UI can render an in-flight row right away.
#[tauri::command]
pub async fn start_peer_install(
    app: AppHandle,
    state: State<'_, LanDownloadState>,
    peer_addr: String,
    peer_port: u16,
    game_id: String,
) -> AppResult<String> {
    if peer_port == 0 {
        return Err(AppError::Other(
            "peer is discovery-only (no file server)".into(),
        ));
    }

    // Reserve the transfer-panel slot up front with a placeholder
    // showing "Fetching manifest…" so the UI has a row to render
    // immediately. The host blake3-hashes the whole game folder on
    // first manifest request (~1 s/GB) — without this, the user sees
    // nothing in the transfer panel until the response lands, which
    // for a multi-GB game means tens of seconds of dead air after the
    // Install button is clicked.
    let install_token = uuid::Uuid::new_v4().to_string();
    let return_token = install_token.clone();
    let placeholder = DownloadProgress {
        install_token: install_token.clone(),
        source_device_id: String::new(),
        source_device_name: peer_addr.clone(),
        source_game_id: game_id.clone(),
        game_name: String::new(),
        bytes_done: 0,
        bytes_total: 0,
        current_file: "Fetching manifest…".into(),
        status: "starting".into(),
        message: None,
        new_game_id: None,
        bytes_per_second: 0.0,
        cover_image_path: None,
    };
    let _check = state.try_start(placeholder.clone())?;
    drop(_check);
    emit_progress(&app, &placeholder);

    // Fetch the manifest. Uses MANIFEST_FETCH_TIMEOUT (not the snappy
    // PEER_FETCH_TIMEOUT) because the host blake3-hashes the folder
    // on first request. On any failure here we clear the slot so the
    // placeholder row goes away before the error toast fires.
    //
    // Pass our session token so the sender can register the upload
    // session immediately — before any file fetches arrive — and show
    // the game name + a progress bar in their Transfers panel while
    // we're still in the "Fetching manifest…" phase.
    let manifest_url = format!(
        "http://{peer_addr}:{peer_port}/games/{game_id}/manifest?session={}",
        urlencoding::encode(&install_token),
    );
    let manifest_result: AppResult<PeerGameManifest> = async {
        let resp = app
            .state::<reqwest::Client>()
            .get(&manifest_url)
            .timeout(MANIFEST_FETCH_TIMEOUT)
            .send()
            .await
            .map_err(|e| AppError::Other(format!("GET manifest: {}", format_reqwest_error(&e))))?;
        if !resp.status().is_success() {
            return Err(AppError::Other(format!(
                "peer responded {} to /manifest",
                resp.status()
            )));
        }
        resp.json::<PeerGameManifest>()
            .await
            .map_err(|e| AppError::Other(format!("parse manifest: {e}")))
    }
    .await;
    let manifest = match manifest_result {
        Ok(m) => m,
        Err(e) => {
            app.state::<LanDownloadState>()
                .clear_if_token(&install_token);
            return Err(e);
        }
    };

    // Manifest in hand — update the panel row with the real game info
    // and total size. Status stays "starting" until run_install flips
    // it to "transferring".
    let progress = DownloadProgress {
        install_token: install_token.clone(),
        source_device_id: manifest.source_device_id.clone(),
        source_device_name: manifest.source_device_name.clone(),
        source_game_id: manifest.game_id.clone(),
        game_name: manifest.game_name.clone(),
        bytes_done: 0,
        bytes_total: manifest.total_bytes,
        current_file: String::new(),
        status: "starting".into(),
        message: None,
        new_game_id: None,
        bytes_per_second: 0.0,
        cover_image_path: None,
    };
    app.state::<LanDownloadState>().set(Some(progress.clone()));
    emit_progress(&app, &progress);

    // Prefetch the cover image from the peer in the background so the
    // transfer-panel row has a thumbnail to render while files stream.
    // The post-install fetch_peer_artwork pass overwrites this with the
    // same filename, so there's no dual-storage concern. Best-effort:
    // a 404 (older peer, no local cover) just leaves cover_image_path
    // None and the panel falls back to the sleeve gradient.
    let cover_app = app.clone();
    let cover_token = install_token.clone();
    let cover_safe_name = manifest.safe_name.clone();
    let cover_peer_addr = peer_addr.clone();
    let cover_source_id = manifest.game_id.clone();
    tauri::async_runtime::spawn(async move {
        let client: reqwest::Client = (*cover_app.state::<reqwest::Client>()).clone();
        let covers_dir = paths::covers_dir();
        if tokio::fs::create_dir_all(&covers_dir).await.is_err() {
            return;
        }
        let url = format!("http://{cover_peer_addr}:{peer_port}/games/{cover_source_id}/cover");
        let Some(path) =
            fetch_and_save_peer_image(&client, &url, &covers_dir, &cover_safe_name, "").await
        else {
            return;
        };
        let path_str = path.to_string_lossy().to_string();
        // Only update if the in-flight install still matches our
        // token — otherwise a slow prefetch could leak into a
        // freshly-started next install.
        let state = cover_app.state::<LanDownloadState>();
        let matched = state
            .snapshot()
            .map(|p| p.install_token == cover_token)
            .unwrap_or(false);
        if !matched {
            return;
        }
        if let Some(snap) = state.update(|p| {
            p.cover_image_path = Some(path_str.clone());
        }) {
            emit_progress(&cover_app, &snap);
        }
    });

    let app_clone = app.clone();
    let state_handle: tauri::State<'_, LanDownloadState> = app.state::<LanDownloadState>();
    // We can't move a `State` across an `await`, but `LanDownloadState`
    // lives on the AppHandle's managed map for the whole process — so
    // re-fetching inside the task is the idiomatic move.
    let _ = state_handle;

    tauri::async_runtime::spawn(async move {
        let result = run_install(
            app_clone.clone(),
            peer_addr.clone(),
            peer_port,
            manifest.clone(),
        )
        .await;
        // Preserve the prefetched cover across the terminal event so
        // the panel row keeps its thumbnail during the 2 s grace
        // window before the slot clears.
        let cover_carry = app_clone
            .state::<LanDownloadState>()
            .snapshot()
            .and_then(|p| p.cover_image_path);
        // Final event. On error, surface the message; on success, point
        // at the freshly-created library entry.
        let final_progress = match result {
            Ok(new_id) => DownloadProgress {
                install_token: install_token.clone(),
                source_device_id: manifest.source_device_id.clone(),
                source_device_name: manifest.source_device_name.clone(),
                source_game_id: manifest.game_id.clone(),
                game_name: manifest.game_name.clone(),
                bytes_done: manifest.total_bytes,
                bytes_total: manifest.total_bytes,
                current_file: String::new(),
                status: "done".into(),
                message: None,
                new_game_id: Some(new_id),
                bytes_per_second: 0.0,
                cover_image_path: cover_carry.clone(),
            },
            Err(e) => {
                // Cancellation is a typed variant on `AppError` so this
                // branch is exact rather than string-matched.
                if e.is_canceled() {
                    tracing::info!(
                        game = %manifest.game_name,
                        by_host = matches!(e, AppError::HostCanceled),
                        "LAN install cancelled",
                    );
                    DownloadProgress {
                        install_token: install_token.clone(),
                        source_device_id: manifest.source_device_id.clone(),
                        source_device_name: manifest.source_device_name.clone(),
                        source_game_id: manifest.game_id.clone(),
                        game_name: manifest.game_name.clone(),
                        bytes_done: 0,
                        bytes_total: manifest.total_bytes,
                        current_file: String::new(),
                        status: "canceled".into(),
                        message: None,
                        new_game_id: None,
                        bytes_per_second: 0.0,
                        cover_image_path: cover_carry.clone(),
                    }
                } else {
                    tracing::warn!(game = %manifest.game_name, error = %e, "LAN install failed");
                    DownloadProgress {
                        install_token: install_token.clone(),
                        source_device_id: manifest.source_device_id.clone(),
                        source_device_name: manifest.source_device_name.clone(),
                        source_game_id: manifest.game_id.clone(),
                        game_name: manifest.game_name.clone(),
                        bytes_done: 0,
                        bytes_total: manifest.total_bytes,
                        current_file: String::new(),
                        status: "error".into(),
                        message: Some(e.to_string()),
                        new_game_id: None,
                        bytes_per_second: 0.0,
                        cover_image_path: cover_carry.clone(),
                    }
                }
            }
        };
        // Publish the final state. `State<'_, T>` is borrowed from
        // `app_clone` and the lock guard's lifetime ties back to it —
        // so we delegate to a method on the state that takes ownership
        // of the lock internally and avoids holding the borrow.
        app_clone
            .state::<LanDownloadState>()
            .set(Some(final_progress.clone()));
        emit_progress(&app_clone, &final_progress);
        // Brief grace period so the UI can pick up the terminal state
        // via snapshot before we clear it. 2 s feels right — long enough
        // for the toast to settle, short enough that a fresh popover
        // open doesn't see stale data.
        tokio::time::sleep(Duration::from_secs(2)).await;
        app_clone
            .state::<LanDownloadState>()
            .clear_if_token(&install_token);
    });

    Ok(return_token)
}

/// Returns true when a reqwest error is transient enough to retry —
/// i.e., it's a network-level failure rather than a protocol error.
/// Connection drops, timeouts, and body read failures on a network
/// switch all qualify; 4xx/5xx responses and decode errors do not.
fn is_retryable_network_error(e: &reqwest::Error) -> bool {
    e.is_connect() || e.is_timeout() || e.is_body()
}

/// Streams one file from the peer. Honours resume (probes the on-disk
/// remnant and sends a Range header if needed), polls the cancel flag
/// between chunks, and bumps the shared `bytes_done` counter as bytes
/// land. Progress event emission is throttled by `last_emit` so
/// thousands of tiny chunks don't drown the IPC channel.
///
/// Retries up to `MAX_DOWNLOAD_RETRIES` times on transient network errors
/// (connection drop, timeout, mid-stream body failure) with exponential
/// backoff. On each retry the partial file is used as a resume point and
/// any bytes we already credited to `bytes_done` are rolled back before
/// re-crediting from the new partial size, keeping the progress counter
/// accurate.
///
/// `max_bps` is the configured bandwidth cap in bytes/s (0 = unlimited).
#[allow(clippy::too_many_arguments)]
async fn download_one_file(
    file: PeerFile,
    partial_dir: PathBuf,
    url: String,
    client: reqwest::Client,
    app: AppHandle,
    bytes_done: Arc<AtomicU64>,
    last_emit: Arc<Mutex<Instant>>,
    max_bps: f64,
) -> AppResult<()> {
    let target = partial_dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Other(format!("mkdir {parent:?}: {e}")))?;
    }

    // Tracks how many bytes this invocation has added to the shared
    // `bytes_done` counter. On retry we roll back this amount and
    // re-credit from the new partial file size, keeping the aggregate
    // progress counter accurate across retries.
    let mut bytes_added: u64 = 0;

    for attempt in 0..=MAX_DOWNLOAD_RETRIES {
        // Cancel check at the top of every attempt.
        {
            let state = app.state::<LanDownloadState>();
            if state.is_canceled() {
                return Err(state.cancel_error());
            }
        }

        if attempt > 0 {
            // Roll back the bytes we credited in the previous failed
            // attempt so the progress bar doesn't overcount. The retry
            // will re-credit from the updated partial file size.
            bytes_done.fetch_sub(bytes_added, Ordering::Relaxed);
            bytes_added = 0;

            let delay = RETRY_BASE_DELAY.saturating_mul(2u32.pow(attempt - 1));
            tracing::info!(
                path = %file.path,
                attempt,
                max = MAX_DOWNLOAD_RETRIES,
                delay_ms = delay.as_millis(),
                "LAN install: retrying after network error"
            );
            let msg = format!("Reconnecting… (attempt {attempt}/{MAX_DOWNLOAD_RETRIES})");
            if let Some(snap) = app.state::<LanDownloadState>().update(|p| {
                p.current_file = msg.clone();
            }) {
                emit_progress(&app, &snap);
            }

            // Interruptible sleep — cancel still works during backoff.
            tokio::time::sleep(delay).await;
            {
                let state = app.state::<LanDownloadState>();
                if state.is_canceled() {
                    return Err(state.cancel_error());
                }
            }
        }

        // Re-read the partial file size on every attempt — it may have
        // grown from bytes written during the previous (failed) attempt.
        // Three branches:
        //   - already complete (size == expected): skip the GET entirely
        //   - partial (0 < existing < expected): Range request, append
        //   - oversized: corrupt remnant, truncate and re-fetch
        let existing_size = match tokio::fs::metadata(&target).await {
            Ok(m) if m.is_file() => m.len(),
            _ => 0,
        };
        if existing_size == file.size {
            bytes_done.fetch_add(file.size, Ordering::Relaxed);
            bytes_added += file.size;
            maybe_emit_progress(&app, &bytes_done, &last_emit, &file.path);
            return Ok(());
        }
        let resume_from = if existing_size < file.size {
            existing_size
        } else {
            0
        };

        let mut request = client.get(&url);
        if resume_from > 0 {
            request = request.header(header::RANGE, format!("bytes={resume_from}-"));
        }
        let resp = match request.send().await {
            Ok(r) => r,
            Err(e) if is_retryable_network_error(&e) && attempt < MAX_DOWNLOAD_RETRIES => {
                tracing::warn!(path = %file.path, attempt, error = %e, "LAN download: network error on send");
                continue;
            }
            Err(e) => return Err(AppError::Other(format!("GET {url}: {e}"))),
        };
        let status = resp.status();
        if !status.is_success() {
            return Err(AppError::Other(format!(
                "peer returned {} for {}",
                status, file.path
            )));
        }
        let server_served_range = status == reqwest::StatusCode::PARTIAL_CONTENT;
        let appending = resume_from > 0 && server_served_range;

        let mut out = if appending {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&target)
                .await
                .map_err(|e| AppError::Other(format!("open append {target:?}: {e}")))?
        } else {
            tokio::fs::File::create(&target)
                .await
                .map_err(|e| AppError::Other(format!("create {target:?}: {e}")))?
        };
        if appending {
            bytes_done.fetch_add(resume_from, Ordering::Relaxed);
            bytes_added += resume_from;
        }

        // Hasher running in parallel with disk writes. When the source
        // didn't include a hash (older peer, empty file), `expected`
        // stays empty and we skip verification on the way out.
        let expected = file.hash.clone();
        let verify = !expected.is_empty();
        let mut hasher = blake3::Hasher::new();
        if verify && appending && resume_from > 0 {
            // Pre-seed the hasher with the already-on-disk prefix so the
            // final digest covers the whole file (not just the tail we
            // just downloaded). On retry, `resume_from` reflects all
            // bytes written so far (including prior failed attempts),
            // so this correctly seeds from the whole partial.
            let mut existing = tokio::fs::File::open(&target)
                .await
                .map_err(|e| AppError::Other(format!("open existing {target:?}: {e}")))?;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = existing
                    .read(&mut buf)
                    .await
                    .map_err(|e| AppError::Other(format!("read existing {target:?}: {e}")))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
        }

        // Surface "now starting this file" — racy with sibling tasks;
        // fine, the UI just shows one representative file name.
        if let Some(snap) = app.state::<LanDownloadState>().update(|p| {
            p.status = "transferring".into();
            p.current_file = file.path.clone();
            p.bytes_done = bytes_done.load(Ordering::Relaxed);
        }) {
            emit_progress(&app, &snap);
        }

        let mut stream = resp.bytes_stream();
        let mut stream_error: Option<reqwest::Error> = None;
        'chunks: while let Some(chunk_result) = stream.next().await {
            {
                let state = app.state::<LanDownloadState>();
                if state.is_canceled() {
                    // Drop the file before any directory cleanup —
                    // Windows refuses to remove a dir with open handles.
                    drop(out);
                    return Err(state.cancel_error());
                }
            }
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) if is_retryable_network_error(&e) && attempt < MAX_DOWNLOAD_RETRIES => {
                    tracing::warn!(path = %file.path, attempt, error = %e, "LAN download: stream error");
                    stream_error = Some(e);
                    break 'chunks;
                }
                Err(e) => {
                    drop(out);
                    return Err(AppError::Other(format!("recv chunk: {e}")));
                }
            };
            if verify {
                hasher.update(&chunk);
            }
            out.write_all(&chunk)
                .await
                .map_err(|e| AppError::Other(format!("write {target:?}: {e}")))?;
            let chunk_len = chunk.len() as u64;
            let bd_after = bytes_done.fetch_add(chunk_len, Ordering::Relaxed) + chunk_len;
            bytes_added += chunk_len;
            maybe_emit_progress(&app, &bytes_done, &last_emit, &file.path);

            // Bandwidth throttle — shared counter keeps all parallel
            // tasks collectively under the cap.
            if let Some(sleep) = app
                .state::<LanDownloadState>()
                .throttle_required(bd_after, max_bps)
            {
                tokio::time::sleep(sleep).await;
            }
        }
        drop(stream);

        if let Some(_e) = stream_error {
            // Flush what we have, close the handle, then loop for retry.
            let _ = out.flush().await;
            drop(out);
            continue;
        }

        out.flush()
            .await
            .map_err(|e| AppError::Other(format!("flush {target:?}: {e}")))?;
        drop(out);

        // Verify the digest. On mismatch we move the corrupt file aside
        // as `<name>.bad` so it can be inspected without losing the
        // evidence. The next attempt re-fetches from scratch because the
        // renamed file no longer occupies the target path.
        if verify {
            let actual = hasher.finalize().to_hex().to_string();
            if actual != expected {
                let on_disk_size = tokio::fs::metadata(&target)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                tracing::warn!(
                    path = %file.path,
                    expected_size = file.size,
                    on_disk_size,
                    expected_hash = %expected,
                    actual_hash = %actual,
                    "LAN install: checksum mismatch (preserving as .bad)"
                );
                let bad_path = target.with_extension(
                    target
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| format!("{e}.bad"))
                        .unwrap_or_else(|| "bad".to_string()),
                );
                let _ = tokio::fs::remove_file(&bad_path).await;
                let _ = tokio::fs::rename(&target, &bad_path).await;
                return Err(AppError::ChecksumMismatch {
                    path: file.path.clone(),
                    expected,
                    actual,
                });
            }
        }

        // Restamp mtime so the destination matches the source.
        if file.mtime_unix_ms > 0 {
            let mtime = filetime::FileTime::from_unix_time(
                (file.mtime_unix_ms / 1000) as i64,
                ((file.mtime_unix_ms % 1000) * 1_000_000) as u32,
            );
            let target_for_blocking = target.clone();
            let _ = tokio::task::spawn_blocking(move || {
                filetime::set_file_mtime(&target_for_blocking, mtime)
            })
            .await;
        }

        return Ok(());
    }

    Err(AppError::Other(format!(
        "download of '{}' failed after {} retries",
        file.path, MAX_DOWNLOAD_RETRIES
    )))
}

/// Throttled progress emit. Multiple parallel tasks race for the lock;
/// whichever task wins the "last_emit too old?" check fires the event,
/// the rest silently skip. The brief `std::sync::Mutex<Instant>` lock
/// is dropped before any work — we never hold a sync mutex across an
/// await.
fn maybe_emit_progress(
    app: &AppHandle,
    bytes_done: &AtomicU64,
    last_emit: &Mutex<Instant>,
    current_file: &str,
) {
    let should_emit = {
        match last_emit.lock() {
            Ok(mut le) if le.elapsed() >= PROGRESS_EMIT_INTERVAL => {
                *le = Instant::now();
                true
            }
            _ => false,
        }
    };
    if !should_emit {
        return;
    }
    let bd = bytes_done.load(Ordering::Relaxed);
    if let Some(snap) = app.state::<LanDownloadState>().update(|p| {
        p.bytes_done = bd;
        p.current_file = current_file.to_string();
    }) {
        emit_progress(app, &snap);
    }
}

/// Heavy lifting for `start_peer_install` — runs in the spawned task.
/// Returns the new library entry's id on success.
async fn run_install(
    app: AppHandle,
    peer_addr: String,
    peer_port: u16,
    manifest: PeerGameManifest,
) -> AppResult<String> {
    let root = install_root_from(&app)?;
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|e| AppError::Other(format!("create install root: {e}")))?;

    // Resume detection: if a `.partial` exists at the preferred name
    // we pick up where we left off rather than allocating a fresh
    // `<name> (2)` install.
    let (final_dir, partial_dir, resuming) = resolve_install_dirs(&root, &manifest.safe_name);
    if resuming {
        tracing::info!(
            partial = %partial_dir.display(),
            "resuming previous LAN install"
        );
    } else {
        tokio::fs::create_dir_all(&partial_dir)
            .await
            .map_err(|e| AppError::Other(format!("create partial dir: {e}")))?;
    }

    // Flip the published status from "starting" to "transferring" as
    // soon as the worker is actually running. Without this, an install
    // where every file short-circuits (resume case where the partial
    // dir already holds everything at the correct size) would stay at
    // "Preparing transfer…" in the UI until the final "done" event
    // arrived — no progress bar movement, no feedback at all. The
    // per-file workers used to publish this transition, but they skip
    // the update on the size-matches-skip-the-fetch path, which is
    // the only path that runs for a fully-resumed install.
    if let Some(snap) = app.state::<LanDownloadState>().update(|p| {
        p.status = "transferring".into();
    }) {
        emit_progress(&app, &snap);
    }

    // Reuse the process-wide shared client. The shared client has no
    // top-level timeout so multi-GB transfers can run as long as they
    // need; the heartbeat uses RequestBuilder::timeout for the short
    // poll. (Per `m07` + `domain-web`: one client per process, share
    // its connection pool + DNS cache.)
    let client: reqwest::Client = (*app.state::<reqwest::Client>()).clone();

    // Shared counters for the parallel file downloads. `bytes_done`
    // accumulates across all tasks; `last_emit` throttles the progress
    // event firehose to ~5 Hz instead of the per-chunk rate (which on
    // a gigabit transfer is thousands per second).
    let bytes_done = Arc::new(AtomicU64::new(0));
    let last_emit = Arc::new(Mutex::new(Instant::now() - PROGRESS_EMIT_INTERVAL * 2));

    // Build the per-file futures. We stream them through
    // `buffer_unordered(LAN_PARALLEL_FILES)` so the slot keeps full
    // even when individual files vary wildly in size. A first-error
    // short-circuit drops the rest cooperatively.
    let manifest_game_id = manifest.game_id.clone();
    // The install_token doubles as the upload session id seen by the
    // source — its host UI groups all 4 of our parallel file fetches
    // into a single row, and host-side cancel keys off it. Reach for
    // it via the public `snapshot()` so we don't touch the private
    // `current` field through a temporary `State<'_, _>`.
    let session_id_for_url = app
        .state::<LanDownloadState>()
        .snapshot()
        .map(|p| p.install_token)
        .unwrap_or_default();
    let game_name_for_url = manifest.game_name.clone();

    // Snapshot the bandwidth cap once at install start. Mid-install
    // setting changes won't take effect until the next install —
    // simpler than threading config through every chunk loop. Convert
    // Mbps (megabits/s, decimal — matching the speed shown in the
    // transfers UI) → bytes/s here so the chunk loop doesn't repeat the
    // math: 1 Mbit = 1_000_000 bits = 125_000 bytes.
    let max_bps = {
        let cfg = app.state::<SharedConfig>();
        let mbps = cfg
            .lock()
            .map(|c| c.data.lan_download_max_mbps)
            .unwrap_or(0.0);
        mbps * 1_000_000.0 / 8.0
    };

    let file_futures = manifest.files.clone().into_iter().map(|file| {
        let partial_dir = partial_dir.clone();
        let client = client.clone();
        let app = app.clone();
        let bytes_done = bytes_done.clone();
        let last_emit = last_emit.clone();
        let peer_addr = peer_addr.clone();
        let game_id = manifest_game_id.clone();
        let session = session_id_for_url.clone();
        let game_name = game_name_for_url.clone();
        async move {
            // URL-encode each segment so spaces / special chars survive.
            let encoded = file
                .path
                .split('/')
                .map(|seg| urlencoding::encode(seg).into_owned())
                .collect::<Vec<_>>()
                .join("/");
            // Session + game_name query params let the source group us
            // into a single "uploads" row and show a friendly title.
            let url = format!(
                "http://{peer_addr}:{peer_port}/games/{game_id}/files/{encoded}?session={}&game_name={}",
                urlencoding::encode(&session),
                urlencoding::encode(&game_name),
            );
            download_one_file(
                file,
                partial_dir,
                url,
                client,
                app,
                bytes_done,
                last_emit,
                max_bps,
            )
            .await
        }
    });

    // Heartbeat: poll the source's /cancel-check every ~3 s so a
    // host-initiated cancel takes effect promptly even between file
    // fetches. On 410 we set the same cancel_flag the user-initiated
    // path uses, so the rest of the code converges to a clean abort.
    let heartbeat = {
        let app_for_hb = app.clone();
        let session = session_id_for_url.clone();
        let game_id = manifest_game_id.clone();
        let peer_addr = peer_addr.clone();
        // Reuse the shared client; per-request timeout via RequestBuilder.
        let hb_client: reqwest::Client = (*app.state::<reqwest::Client>()).clone();
        tokio::spawn(async move {
            let url = format!(
                "http://{peer_addr}:{peer_port}/games/{game_id}/cancel-check?session={}",
                urlencoding::encode(&session)
            );
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let state = app_for_hb.state::<LanDownloadState>();
                if state.is_canceled() {
                    return;
                }
                // GONE (410) is the "host cancelled" signal.
                if let Ok(resp) = hb_client
                    .get(&url)
                    .timeout(Duration::from_secs(3))
                    .send()
                    .await
                {
                    if resp.status() == reqwest::StatusCode::GONE {
                        tracing::info!("LAN install: host cancelled the upload");
                        state.request_host_cancel();
                        return;
                    }
                }
            }
        })
    };

    let mut stream = futures_util::stream::iter(file_futures).buffer_unordered(LAN_PARALLEL_FILES);
    // Drain until cancel or first error. We capture the terminal state
    // into `maybe_err` so we can finish cleanup (drop stream → cancel
    // in-flight tasks; abort heartbeat) before propagating up.
    let mut maybe_err: Option<AppError> = None;
    while let Some(result) = stream.next().await {
        {
            let state = app.state::<LanDownloadState>();
            if state.is_canceled() {
                maybe_err = Some(state.cancel_error());
                break;
            }
        }
        if let Err(e) = result {
            maybe_err = Some(e);
            break;
        }
    }
    drop(stream);
    heartbeat.abort();
    if let Some(e) = maybe_err {
        // Any flavour of cancel wipes the partial dir so a fresh
        // attempt doesn't pick up half-written state. Other errors
        // keep the partial dir so the user can retry with resume.
        if e.is_canceled() {
            let _ = tokio::fs::remove_dir_all(&partial_dir).await;
        }
        return Err(e);
    }
    // Final progress flush — make sure the UI shows 100% before we
    // emit the terminal "done" event.
    let final_bd = bytes_done.load(Ordering::Relaxed);
    if let Some(snap) = app
        .state::<LanDownloadState>()
        .update(|p| p.bytes_done = final_bd)
    {
        emit_progress(&app, &snap);
    }

    // All files landed — flip the staging dir into its real location.
    tokio::fs::rename(&partial_dir, &final_dir)
        .await
        .map_err(|e| AppError::Other(format!("finalise install dir: {e}")))?;

    // Build the library entry. exe_path is the manifest-supplied
    // relative path joined to our final install dir; if the source
    // didn't have one we leave it empty and the user wires it up.
    let exe_path = manifest
        .exe_relative_path
        .as_ref()
        .map(|rel| {
            final_dir
                .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default();

    let new_id = uuid::Uuid::new_v4().to_string();
    let library = app.state::<SharedLibrary>();
    let entry = {
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = GameEntry {
            id: new_id.clone(),
            catalog_number: lib.next_catalog_number(),
            game_name: manifest.game_name.clone(),
            exe_path,
            safe_name: manifest.safe_name.clone(),
            added_at: Some(Utc::now()),
            game_folder_path: Some(final_dir.to_string_lossy().to_string()),
            steam_id: manifest.steam_id,
            gog_id: manifest.gog_id,
            lutris_slug: manifest.lutris_slug.clone(),
            has_cloud_save: manifest.has_cloud_save,
            manifest_install_dir: manifest.manifest_install_dir.clone(),
            save_paths: manifest.save_paths.clone(),
            developer: manifest.developer.clone(),
            publisher: manifest.publisher.clone(),
            genres: manifest.genres.clone(),
            release_date: manifest.release_date,
            install_size_mb: (manifest.total_bytes as f64) / (1024.0 * 1024.0),
            install_source: "lan".to_string(),
            lan_install_source_device_id: Some(manifest.source_device_id.clone()),
            lan_install_source_device_name: Some(manifest.source_device_name.clone()),
            ..GameEntry::default()
        };
        lib.entries.push(entry.clone());
        lib.save()?;
        entry
    };

    if let Err(e) = app.emit("library:changed", &entry.id) {
        tracing::warn!(error = %e, "failed to emit library:changed after LAN install");
    }

    // Background artwork fetch. Try the peer's `/cover` and `/hero`
    // first — that gives us pixel-identical art with no SteamGridDB
    // API key requirement and works for games SGDB doesn't index.
    // If the peer 404s the cover (older Spool, no local cover), fall
    // back to the regular SteamGridDB fetch.
    let app_for_art = app.clone();
    let id_for_art = entry.id.clone();
    let safe_name_for_art = entry.safe_name.clone();
    let peer_addr_for_art = peer_addr.clone();
    let source_id_for_art = manifest.game_id.clone();
    tauri::async_runtime::spawn(async move {
        let got_cover = fetch_peer_artwork(
            &app_for_art,
            &id_for_art,
            &safe_name_for_art,
            &peer_addr_for_art,
            peer_port,
            &source_id_for_art,
        )
        .await;
        if !got_cover {
            if let Err(e) =
                crate::steamgriddb::fetch_and_save_cover(&app_for_art, &id_for_art).await
            {
                tracing::warn!(
                    game_id = %id_for_art,
                    error = %e,
                    "cover fetch failed (peer 404 + SteamGridDB fallback)"
                );
            }
        }
    });

    Ok(new_id)
}

/// Fetches cover + hero artwork from a peer and writes them into the
/// covers/ dir, then updates the library entry's image paths +
/// accent_color. Best-effort: each fetch (cover and hero) is
/// independent — if hero 404s we still keep the cover, and vice
/// versa. Returns `true` if a cover landed, which is what the caller
/// uses to decide whether to fall back to SteamGridDB.
async fn fetch_peer_artwork(
    app: &AppHandle,
    new_game_id: &str,
    safe_name: &str,
    peer_addr: &str,
    peer_port: u16,
    source_game_id: &str,
) -> bool {
    // Shared client; the 30s budget is applied per request below.
    let client: reqwest::Client = (*app.state::<reqwest::Client>()).clone();

    let covers_dir = paths::covers_dir();
    if tokio::fs::create_dir_all(&covers_dir).await.is_err() {
        return false;
    }

    let cover_url = format!("http://{peer_addr}:{peer_port}/games/{source_game_id}/cover");
    let hero_url = format!("http://{peer_addr}:{peer_port}/games/{source_game_id}/hero");

    // Fetch both in parallel — they're tiny relative to the game
    // bytes and there's no point serialising them.
    let (cover_path, hero_path) = tokio::join!(
        fetch_and_save_peer_image(&client, &cover_url, &covers_dir, safe_name, ""),
        fetch_and_save_peer_image(&client, &hero_url, &covers_dir, safe_name, "-hero"),
    );

    if cover_path.is_none() && hero_path.is_none() {
        return false;
    }

    // Accent extraction is best-effort and only meaningful from the
    // portrait cover. Heroes are wide and would skew the colour.
    // Image decode + histogram is sync CPU/disk work (~10ms for a
    // typical cover), so per `m07-concurrency` it lives on
    // `spawn_blocking` rather than blocking the async runtime.
    let accent = if let Some(p) = cover_path.as_ref() {
        let p = p.clone();
        tokio::task::spawn_blocking(move || crate::steamgriddb::extract_vibrant_color(&p))
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    // Update the library entry. Same shape as the pattern in
    // `run_install` above: bind State to a local first so the
    // MutexGuard's borrow has a stable anchor — Tauri's
    // `State<'_, T>` lifetime + a chained `.lock()` confuses the
    // borrow checker otherwise.
    let library = app.state::<SharedLibrary>();
    if let Ok(mut lib) = library.lock() {
        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == new_game_id) {
            if let Some(p) = &cover_path {
                entry.cover_image_path = Some(p.to_string_lossy().to_string());
            }
            if let Some(p) = &hero_path {
                entry.hero_image_path = Some(p.to_string_lossy().to_string());
            }
            if let Some(a) = accent {
                entry.accent_color = Some(a);
            }
        }
        let _ = lib.save();
    }
    let _ = app.emit("library:changed", &new_game_id.to_string());
    cover_path.is_some()
}

/// Downloads one image from `url` and saves it as
/// `<dir>/<safe_name><suffix>.<ext>` where the extension is sniffed
/// from the response's Content-Type. Returns the path on success,
/// `None` on any failure (404, network, write error).
async fn fetch_and_save_peer_image(
    client: &reqwest::Client,
    url: &str,
    dir: &Path,
    safe_name: &str,
    suffix: &str,
) -> Option<PathBuf> {
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_default();
    let ext = crate::steamgriddb::mime_to_ext(&mime).unwrap_or("jpg");
    let bytes = resp.bytes().await.ok()?;
    let path = dir.join(format!("{safe_name}{suffix}.{ext}"));
    tokio::fs::write(&path, &bytes).await.ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn allocate_uses_preferred_name_when_free() {
        let root = tempdir().unwrap();
        assert_eq!(
            allocate_install_dir(root.path(), "Hades"),
            root.path().join("Hades")
        );
    }

    #[test]
    fn allocate_suffixes_on_collision() {
        let root = tempdir().unwrap();
        std::fs::create_dir(root.path().join("Hades")).unwrap();
        assert_eq!(
            allocate_install_dir(root.path(), "Hades"),
            root.path().join("Hades (2)")
        );
        std::fs::create_dir(root.path().join("Hades (2)")).unwrap();
        assert_eq!(
            allocate_install_dir(root.path(), "Hades"),
            root.path().join("Hades (3)")
        );
    }

    #[test]
    fn allocate_falls_back_to_game_for_empty_name() {
        let root = tempdir().unwrap();
        assert_eq!(
            allocate_install_dir(root.path(), ""),
            root.path().join("Game")
        );
    }

    #[test]
    fn resolve_fresh_install_picks_base_and_partial() {
        let root = tempdir().unwrap();
        let (final_dir, partial, resuming) = resolve_install_dirs(root.path(), "Hades");
        assert_eq!(final_dir, root.path().join("Hades"));
        assert_eq!(partial, root.path().join("Hades.partial"));
        assert!(!resuming);
    }

    #[test]
    fn resolve_resumes_into_existing_partial() {
        let root = tempdir().unwrap();
        std::fs::create_dir(root.path().join("Hades.partial")).unwrap();
        let (final_dir, partial, resuming) = resolve_install_dirs(root.path(), "Hades");
        assert_eq!(final_dir, root.path().join("Hades"));
        assert_eq!(partial, root.path().join("Hades.partial"));
        assert!(
            resuming,
            "a leftover .partial with a free final dir resumes"
        );
    }

    #[test]
    fn resolve_allocates_fresh_when_final_already_installed() {
        let root = tempdir().unwrap();
        // Both a finished install and a stale partial exist — we must not
        // resume into the partial because the final name is taken, so a
        // fresh non-colliding pair is allocated instead.
        std::fs::create_dir(root.path().join("Hades")).unwrap();
        std::fs::create_dir(root.path().join("Hades.partial")).unwrap();
        let (final_dir, partial, resuming) = resolve_install_dirs(root.path(), "Hades");
        assert_eq!(final_dir, root.path().join("Hades (2)"));
        assert_eq!(partial, root.path().join("Hades (2).partial"));
        assert!(!resuming);
    }
}

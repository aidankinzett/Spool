//! The receiving half of LAN transfer: browse a peer's catalogue, fetch
//! a manifest, stream every file to a `.partial` staging dir with blake3
//! verification + HTTP-range resume, then rename into place and register
//! a new library entry. Single in-flight install slot with a cooperative
//! cancel flag.

use super::server::safe_join;
use super::{PeerFile, PeerGame, PeerGameManifest};
use crate::config::ConfigData;
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
/// How long the install slot keeps showing a terminal `done`/`error`/`canceled`
/// state before it frees, so a poll-based consumer (the Decky plugin's
/// `GET /lan/download`) can observe the terminal snapshot rather than seeing the
/// slot blink straight to empty.
const TERMINAL_STATE_GRACE: Duration = Duration::from_secs(2);

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
    /// Claims the single install slot for `p` and returns an RAII guard
    /// whose `Drop` releases it. The guard owns an `Arc<Self>` (not a
    /// borrow) so `begin_install` can move it into the detached transfer
    /// task — keeping the slot occupied for the *whole* operation,
    /// including the manifest fetch, rather than just the moment between
    /// claim and the early `drop`. While the slot is held a second
    /// `try_start` is rejected, `request_cancel` can find the active
    /// session, and `update` routes terminal errors to the UI.
    fn try_start(self: &Arc<Self>, p: DownloadProgress) -> AppResult<DownloadGuard> {
        let token = p.install_token.clone();
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
        Ok(DownloadGuard {
            state: self.clone(),
            token,
        })
    }

    /// Marks the current install as cancelled iff `token` matches. The
    /// download loop will notice on its next poll and abort cleanly.
    /// Returns true if a cancel was actually requested (token matched
    /// an in-flight install).
    pub fn request_cancel(&self, token: &str) -> bool {
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

    pub fn snapshot(&self) -> Option<DownloadProgress> {
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
    pub fn set(&self, value: Option<DownloadProgress>) {
        if let Ok(mut g) = self.current.lock() {
            *g = value;
        }
    }

    /// Clear the slot iff the in-flight install matches `token`. The
    /// guard against clearing the wrong install protects the case where
    /// the user kicked off a second install during the 2 s grace period
    /// after the first one finished.
    pub fn clear_if_token(&self, token: &str) {
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
///
/// Owns an `Arc<LanDownloadState>` (not a borrow) so it can be moved into
/// the detached transfer task and live for the whole install. The clear
/// is token-gated for the same reason `clear_if_token` is: the install
/// task publishes its terminal "done"/"error" state and sleeps a 2 s
/// grace period before releasing the slot, during which the user may have
/// started a *new* install. An unconditional clear in `Drop` would wipe
/// that second install's slot; gating on our own token leaves it alone.
struct DownloadGuard {
    state: Arc<LanDownloadState>,
    token: String,
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.state.clear_if_token(&self.token);
    }
}

/// Context bundle passed to the download worker functions instead of
/// `AppHandle`. Holds everything the hot-path needs to check cancellation,
/// emit progress, and throttle bandwidth — without tying the code to the
/// Tauri event bus. Both the GUI path (`start_peer_install`) and the
/// headless plugin server (`begin_install`) build one of these; only the
/// `on_progress` closure differs.
struct TransferCtx {
    http: reqwest::Client,
    state: Arc<LanDownloadState>,
    on_progress: Arc<dyn Fn(&DownloadProgress) + Send + Sync>,
}

/// Resolves where new LAN installs land. Defaults to
/// `<app_data>/lan-games` when the user hasn't set `lan_install_dir`
/// in config — matches the convention of every other Spool path.
fn install_root_from(config: &ConfigData) -> AppResult<PathBuf> {
    if config.lan.install_dir.is_empty() {
        Ok(paths::app_data_dir().join("lan-games"))
    } else {
        Ok(PathBuf::from(&config.lan.install_dir))
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

/// Returns the first manifest path (a file path or the exe path) that would
/// escape the install directory, or `None` if every path is safe to write.
///
/// Manifest paths come straight off the network in `PeerGameManifest`. A
/// malicious or compromised peer can send `../` components (or an absolute /
/// Windows-prefixed path) to make the receiver write outside the install
/// root — an arbitrary file write, e.g. into an autostart/config location.
/// The host validates outbound paths with [`safe_join`]; the receiver must
/// do the same on the way in, *before* any directory is created or byte
/// written. blake3 verification is no defence — the attacker controls the
/// hash too. See issue #267.
fn first_unsafe_manifest_path(manifest: &PeerGameManifest) -> Option<&str> {
    // The root is irrelevant to the safety check (we only care whether the
    // join is rejected), so an empty root keeps this a pure function.
    let probe = Path::new("");
    for file in &manifest.files {
        if safe_join(probe, &file.path).is_none() {
            return Some(&file.path);
        }
    }
    if let Some(rel) = manifest.exe_relative_path.as_deref() {
        if safe_join(probe, rel).is_none() {
            return Some(rel);
        }
    }
    None
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

fn emit_progress(ctx: &TransferCtx, progress: &DownloadProgress) {
    (ctx.on_progress)(progress);
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
pub fn current_peer_download(
    state: State<'_, Arc<LanDownloadState>>,
) -> Option<DownloadProgress> {
    state.snapshot()
}

/// Requests cancellation of an in-flight install. The download task
/// polls the cancel flag between chunks, cleans up its `.partial` dir,
/// then emits a final `lan:download` with `status: "canceled"`. Returns
/// `true` if the token matched an active install, `false` if there was
/// nothing to cancel (no in-flight transfer, or different token).
#[tauri::command]
pub fn cancel_peer_install(
    state: State<'_, Arc<LanDownloadState>>,
    install_token: String,
) -> bool {
    state.request_cancel(&install_token)
}

/// Kicks off a peer install from the Tauri GUI. Delegates all shared
/// logic to `begin_install`, wiring it with Tauri-specific callbacks
/// (event emission and post-install artwork fetch).
#[tauri::command]
pub async fn start_peer_install(
    app: AppHandle,
    state: State<'_, Arc<LanDownloadState>>,
    peer_addr: String,
    peer_port: u16,
    game_id: String,
) -> AppResult<String> {
    if peer_port == 0 {
        return Err(AppError::Other(
            "peer is discovery-only (no file server)".into(),
        ));
    }

    let http = (*app.state::<reqwest::Client>()).clone();
    let download_state = (*state).clone();

    let (max_bps, install_root) = {
        let cfg = app.state::<crate::config::SharedConfig>();
        let data = cfg.lock().map_err(|_| AppError::LockPoisoned)?.data.clone();
        let root = install_root_from(&data)?;
        let bps = data.lan.download_max_mbps * 1_000_000.0 / 8.0;
        (bps, root)
    };

    let library = (*app.state::<SharedLibrary>()).clone();

    let app_for_changed = app.clone();
    let on_library_changed: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |id: &str| {
            let _ = app_for_changed.emit("library:changed", id);
        });

    let app_for_progress = app.clone();
    let on_progress: Arc<dyn Fn(&DownloadProgress) + Send + Sync> =
        Arc::new(move |p: &DownloadProgress| {
            if let Err(e) = app_for_progress.emit("lan:download", p) {
                tracing::warn!(error = %e, "failed to emit lan:download");
            }
        });

    // Post-install artwork: fetch from the peer (cover + hero), then fall
    // back to SteamGridDB. Runs in its own task so the UI gets the "done"
    // event immediately and the cover just appears once it lands.
    let peer_addr_for_art = peer_addr.clone();
    let app_for_art = app.clone();
    let on_success: Arc<dyn Fn(String, PeerGameManifest) + Send + Sync> =
        Arc::new(move |new_id: String, manifest: PeerGameManifest| {
            let app = app_for_art.clone();
            let peer_addr = peer_addr_for_art.clone();
            tauri::async_runtime::spawn(async move {
                let got_cover = fetch_peer_artwork(
                    &app,
                    &new_id,
                    &manifest.safe_name,
                    &peer_addr,
                    peer_port,
                    &manifest.game_id,
                )
                .await;
                if !got_cover {
                    if let Err(e) =
                        crate::steamgriddb::fetch_and_save_cover(&app, &new_id).await
                    {
                        tracing::warn!(
                            game_id = %new_id,
                            error = %e,
                            "cover fetch failed (peer 404 + SteamGridDB fallback)"
                        );
                    }
                }
            });
        });

    begin_install(
        peer_addr,
        peer_port,
        game_id,
        http,
        download_state,
        on_progress,
        max_bps,
        install_root,
        library,
        on_library_changed,
        Some(on_success),
    )
    .await
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
    ctx: Arc<TransferCtx>,
    bytes_done: Arc<AtomicU64>,
    last_emit: Arc<Mutex<Instant>>,
    max_bps: f64,
) -> AppResult<()> {
    // Validated up front in `run_install`; re-checked here so this write
    // site can never escape the staging dir even if reached another way.
    let target = safe_join(&partial_dir, &file.path)
        .ok_or_else(|| AppError::Other(format!("unsafe file path in manifest: {:?}", file.path)))?;
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
        if ctx.state.is_canceled() {
            return Err(ctx.state.cancel_error());
        }

        if attempt > 0 {
            // Roll back the bytes we credited in the previous failed
            // attempt so the progress bar doesn't overcount. The retry
            // will re-credit from the updated partial file size.
            bytes_done.fetch_sub(bytes_added, Ordering::Relaxed);
            bytes_added = 0;
            let rolled_back = bytes_done.load(Ordering::Relaxed);

            let delay = RETRY_BASE_DELAY.saturating_mul(2u32.pow(attempt - 1));
            tracing::info!(
                path = %file.path,
                attempt,
                max = MAX_DOWNLOAD_RETRIES,
                delay_ms = delay.as_millis(),
                "LAN install: retrying after network error"
            );
            let msg = format!("Reconnecting… (attempt {attempt}/{MAX_DOWNLOAD_RETRIES})");
            if let Some(snap) = ctx.state.update(|p| {
                p.current_file = msg.clone();
                p.bytes_done = rolled_back;
            }) {
                emit_progress(&ctx, &snap);
            }

            // Interruptible sleep — cancel still works during backoff.
            tokio::time::sleep(delay).await;
            if ctx.state.is_canceled() {
                return Err(ctx.state.cancel_error());
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
            // If the manifest includes a hash, verify the on-disk file before
            // accepting it — a previous interrupted write can leave a
            // full-length but corrupt file.
            if !file.hash.is_empty() {
                let mut f = tokio::fs::File::open(&target)
                    .await
                    .map_err(|e| AppError::Other(format!("open {target:?}: {e}")))?;
                let mut hasher = blake3::Hasher::new();
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let n = f
                        .read(&mut buf)
                        .await
                        .map_err(|e| AppError::Other(format!("read {target:?}: {e}")))?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                }
                drop(f);
                let actual = hasher.finalize().to_hex().to_string();
                if actual == file.hash {
                    bytes_done.fetch_add(file.size, Ordering::Relaxed);
                    maybe_emit_progress(&ctx, &bytes_done, &last_emit, &file.path);
                    return Ok(());
                }
                // Hash mismatch — truncate so the normal download path
                // re-fetches from scratch on this same attempt.
                tracing::warn!(
                    path = %file.path,
                    expected = %file.hash,
                    actual = %actual,
                    "LAN install: full-size partial hash mismatch, re-fetching"
                );
                let _ = tokio::fs::File::create(&target).await;
                // Fall through to the normal GET path with resume_from = 0.
            } else {
                bytes_done.fetch_add(file.size, Ordering::Relaxed);
                maybe_emit_progress(&ctx, &bytes_done, &last_emit, &file.path);
                return Ok(());
            }
        }
        let resume_from = if existing_size < file.size {
            existing_size
        } else {
            0
        };

        let mut request = ctx.http.get(&url);
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
        if let Some(snap) = ctx.state.update(|p| {
            p.status = "transferring".into();
            p.current_file = file.path.clone();
            p.bytes_done = bytes_done.load(Ordering::Relaxed);
        }) {
            emit_progress(&ctx, &snap);
        }

        let mut stream = resp.bytes_stream();
        let mut stream_error: Option<reqwest::Error> = None;
        'chunks: while let Some(chunk_result) = stream.next().await {
            if ctx.state.is_canceled() {
                // Drop the file before any directory cleanup —
                // Windows refuses to remove a dir with open handles.
                drop(out);
                return Err(ctx.state.cancel_error());
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
            maybe_emit_progress(&ctx, &bytes_done, &last_emit, &file.path);

            // Bandwidth throttle — shared counter keeps all parallel
            // tasks collectively under the cap.
            if let Some(sleep) = ctx.state.throttle_required(bd_after, max_bps) {
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
    ctx: &TransferCtx,
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
    if let Some(snap) = ctx.state.update(|p| {
        p.bytes_done = bd;
        p.current_file = current_file.to_string();
    }) {
        emit_progress(ctx, &snap);
    }
}

/// Heavy lifting — runs in the spawned task.
/// Streams all files, verifies hashes, renames partial → final, creates
/// the library entry. Returns the new entry's id on success.
#[allow(clippy::too_many_arguments)]
async fn run_install(
    ctx: Arc<TransferCtx>,
    peer_addr: String,
    peer_port: u16,
    mut manifest: PeerGameManifest,
    max_bps: f64,
    install_root: PathBuf,
    library: SharedLibrary,
    on_library_changed: Arc<dyn Fn(&str) + Send + Sync>,
) -> AppResult<String> {
    // Refuse a poisoned manifest before creating any directory or writing a
    // single byte. See `first_unsafe_manifest_path` for the why.
    if let Some(bad) = first_unsafe_manifest_path(&manifest) {
        return Err(AppError::Other(format!(
            "refusing LAN install: manifest path escapes the install dir: {bad:?}"
        )));
    }

    tokio::fs::create_dir_all(&install_root)
        .await
        .map_err(|e| AppError::Other(format!("create install root: {e}")))?;

    // Resume detection: if a `.partial` exists at the preferred name
    // we pick up where we left off rather than allocating a fresh
    // `<name> (2)` install.
    let (final_dir, partial_dir, resuming) =
        resolve_install_dirs(&install_root, &manifest.safe_name);
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
    // soon as the worker is actually running.
    if let Some(snap) = ctx.state.update(|p| {
        p.status = "transferring".into();
    }) {
        emit_progress(&ctx, &snap);
    }

    // Shared counters for the parallel file downloads.
    let bytes_done = Arc::new(AtomicU64::new(0));
    let last_emit = Arc::new(Mutex::new(Instant::now() - PROGRESS_EMIT_INTERVAL * 2));

    let manifest_game_id = manifest.game_id.clone();
    let session_id_for_url = ctx
        .state
        .snapshot()
        .map(|p| p.install_token)
        .unwrap_or_default();
    let game_name_for_url = manifest.game_name.clone();

    // Move the file list out of the manifest rather than cloning it — the
    // rest of `run_install` only reads scalar manifest fields after this, and
    // a multi-thousand-entry game manifest is needlessly large to duplicate.
    let file_futures = std::mem::take(&mut manifest.files).into_iter().map(|file| {
        let partial_dir = partial_dir.clone();
        let ctx = ctx.clone();
        let bytes_done = bytes_done.clone();
        let last_emit = last_emit.clone();
        let peer_addr = peer_addr.clone();
        let game_id = manifest_game_id.clone();
        let session = session_id_for_url.clone();
        let game_name = game_name_for_url.clone();
        async move {
            let encoded = file
                .path
                .split('/')
                .map(|seg| urlencoding::encode(seg).into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let url = format!(
                "http://{peer_addr}:{peer_port}/games/{game_id}/files/{encoded}?session={}&game_name={}",
                urlencoding::encode(&session),
                urlencoding::encode(&game_name),
            );
            download_one_file(file, partial_dir, url, ctx, bytes_done, last_emit, max_bps).await
        }
    });

    // Heartbeat: poll the source's /cancel-check every ~3 s so a
    // host-initiated cancel takes effect promptly even between file
    // fetches.
    let heartbeat = {
        let ctx_hb = ctx.clone();
        let session = session_id_for_url.clone();
        let game_id = manifest_game_id.clone();
        let peer_addr = peer_addr.clone();
        let hb_client = ctx.http.clone();
        tokio::spawn(async move {
            let url = format!(
                "http://{peer_addr}:{peer_port}/games/{game_id}/cancel-check?session={}",
                urlencoding::encode(&session)
            );
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                if ctx_hb.state.is_canceled() {
                    return;
                }
                if let Ok(resp) = hb_client
                    .get(&url)
                    .timeout(Duration::from_secs(3))
                    .send()
                    .await
                {
                    if resp.status() == reqwest::StatusCode::GONE {
                        tracing::info!("LAN install: host cancelled the upload");
                        ctx_hb.state.request_host_cancel();
                        return;
                    }
                }
            }
        })
    };

    let mut stream =
        futures_util::stream::iter(file_futures).buffer_unordered(LAN_PARALLEL_FILES);
    let mut maybe_err: Option<AppError> = None;
    while let Some(result) = stream.next().await {
        if ctx.state.is_canceled() {
            maybe_err = Some(ctx.state.cancel_error());
            break;
        }
        if let Err(e) = result {
            maybe_err = Some(e);
            break;
        }
    }
    drop(stream);
    heartbeat.abort();
    if let Some(e) = maybe_err {
        if e.is_canceled() {
            let _ = tokio::fs::remove_dir_all(&partial_dir).await;
        }
        return Err(e);
    }

    // Final progress flush — make sure the UI shows 100% before the
    // terminal "done" event.
    let final_bd = bytes_done.load(Ordering::Relaxed);
    if let Some(snap) = ctx.state.update(|p| p.bytes_done = final_bd) {
        emit_progress(&ctx, &snap);
    }

    // All files landed — flip the staging dir into its real location.
    tokio::fs::rename(&partial_dir, &final_dir)
        .await
        .map_err(|e| AppError::Other(format!("finalise install dir: {e}")))?;

    // Build the library entry.
    let exe_path = manifest
        .exe_relative_path
        .as_deref()
        .and_then(|rel| safe_join(&final_dir, rel))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let folder_path = final_dir.to_string_lossy().to_string();
    let install_size_mb = (manifest.total_bytes as f64) / (1024.0 * 1024.0);

    // Re-add reuse: if an *uninstalled* entry already exists for this game
    // (same steam id, else same name), reinstall it in place so its catalog
    // number, playtime, art, and save backups carry over instead of creating a
    // duplicate — mirroring the desktop Add flow. Otherwise insert a new entry.
    let game_id = if let Some(existing) = library
        .find_reusable_entry(manifest.steam_id, &manifest.game_name)
        .await?
    {
        let mut fields: Vec<(&str, serde_json::Value)> = vec![
            ("installed", serde_json::json!(true)),
            ("exe_path", serde_json::json!(exe_path)),
            ("game_folder_path", serde_json::json!(folder_path)),
            ("install_size_mb", serde_json::json!(install_size_mb)),
            ("install_source", serde_json::json!("lan")),
            (
                "lan_install_source_device_id",
                serde_json::json!(manifest.source_device_id.clone()),
            ),
            (
                "lan_install_source_device_name",
                serde_json::json!(manifest.source_device_name.clone()),
            ),
        ];
        // Refresh manifest-derived metadata only when the peer supplied it, so
        // an existing entry's identification isn't blanked by a sparse share.
        if manifest.steam_id.is_some() {
            fields.push(("steam_id", serde_json::json!(manifest.steam_id)));
        }
        if manifest.gog_id.is_some() {
            fields.push(("gog_id", serde_json::json!(manifest.gog_id)));
        }
        if manifest.lutris_slug.is_some() {
            fields.push(("lutris_slug", serde_json::json!(manifest.lutris_slug.clone())));
        }
        if manifest.manifest_install_dir.is_some() {
            fields.push((
                "manifest_install_dir",
                serde_json::json!(manifest.manifest_install_dir.clone()),
            ));
        }
        if !manifest.save_paths.is_empty() {
            fields.push(("save_paths", serde_json::json!(manifest.save_paths.clone())));
        }
        library.update_fields(&existing.id, &fields).await?;
        existing.id
    } else {
        let new_id = uuid::Uuid::new_v4().to_string();
        let entry = GameEntry {
            id: new_id.clone(),
            // 0 → insert() assigns the next catalog number atomically.
            catalog_number: 0,
            game_name: manifest.game_name.clone(),
            exe_path,
            safe_name: manifest.safe_name.clone(),
            added_at: Some(Utc::now()),
            game_folder_path: Some(folder_path),
            steam_id: manifest.steam_id,
            gog_id: manifest.gog_id,
            lutris_slug: manifest.lutris_slug.clone(),
            manifest_install_dir: manifest.manifest_install_dir.clone(),
            save_paths: manifest.save_paths.clone(),
            developer: manifest.developer.clone(),
            publisher: manifest.publisher.clone(),
            genres: manifest.genres.clone(),
            release_date: manifest.release_date,
            install_size_mb,
            install_source: "lan".to_string(),
            lan_install_source_device_id: Some(manifest.source_device_id.clone()),
            lan_install_source_device_name: Some(manifest.source_device_name.clone()),
            ..GameEntry::default()
        };
        library.insert(entry).await?;
        new_id
    };

    on_library_changed(&game_id);
    Ok(game_id)
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
    let client: reqwest::Client = (*app.state::<reqwest::Client>()).clone();

    let covers_dir = paths::covers_dir();
    if tokio::fs::create_dir_all(&covers_dir).await.is_err() {
        return false;
    }

    let cover_url = format!("http://{peer_addr}:{peer_port}/games/{source_game_id}/cover");
    let hero_url = format!("http://{peer_addr}:{peer_port}/games/{source_game_id}/hero");

    let (cover_path, hero_path) = tokio::join!(
        fetch_and_save_peer_image(&client, &cover_url, &covers_dir, safe_name, ""),
        fetch_and_save_peer_image(&client, &hero_url, &covers_dir, safe_name, "-hero"),
    );

    if cover_path.is_none() && hero_path.is_none() {
        return false;
    }

    let accent = if let Some(p) = cover_path.as_ref() {
        let p = p.clone();
        tokio::task::spawn_blocking(move || crate::steamgriddb::extract_vibrant_color(&p))
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    let cover = cover_path.as_ref().map(|p| p.to_string_lossy().to_string());
    let hero = hero_path.as_ref().map(|p| p.to_string_lossy().to_string());
    let _ = app
        .state::<SharedLibrary>()
        .set_art(new_game_id, cover.as_deref(), hero.as_deref(), accent.as_deref())
        .await;
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

/// Orchestrates a full LAN install from any context — Tauri GUI or
/// headless plugin server. Acquires the single-slot guard, fetches the
/// manifest, prefetches the cover image, then spawns a task that streams
/// every file with blake3 verification + resume, renames the staging dir,
/// and registers the new library entry.
///
/// Progress is delivered via `on_progress`. In the GUI path this closure
/// emits `lan:download` Tauri events; in the headless path it's a no-op
/// and the plugin UI polls `GET /lan/download` instead.
///
/// `on_success` (optional) is called with the new game's id and the
/// manifest once the install completes — the GUI path uses this to spawn
/// a post-install artwork fetch.
///
/// Emits the terminal "canceled" state for an install cancelled during
/// the manifest fetch (before the transfer task is spawned), then returns
/// the matching `cancel_error()`. Mirrors exactly what the spawned
/// transfer task publishes on cancel — `status: "canceled"`, zeroed
/// progress, no message/new_game_id — so the UI doesn't stay stuck on the
/// "Fetching manifest…" placeholder. Identity fields are taken from the
/// slot snapshot (the placeholder set in `try_start`) since the manifest
/// hasn't landed yet. The caller hands the held `DownloadGuard` to
/// `spawn_slot_grace` so the slot keeps showing this terminal state for the
/// grace window before it frees.
fn emit_terminal_cancel(
    download_state: &Arc<LanDownloadState>,
    on_progress: &Arc<dyn Fn(&DownloadProgress) + Send + Sync>,
    peer_addr: &str,
    game_id: &str,
) -> AppResult<String> {
    let err = download_state.cancel_error();
    tracing::info!(
        by_host = matches!(err, AppError::HostCanceled),
        "LAN install cancelled during manifest fetch",
    );
    let snap = download_state.snapshot();
    let terminal = DownloadProgress {
        install_token: snap
            .as_ref()
            .map(|p| p.install_token.clone())
            .unwrap_or_default(),
        source_device_id: snap
            .as_ref()
            .map(|p| p.source_device_id.clone())
            .unwrap_or_default(),
        source_device_name: snap
            .as_ref()
            .map(|p| p.source_device_name.clone())
            .unwrap_or_else(|| peer_addr.to_string()),
        source_game_id: snap
            .as_ref()
            .map(|p| p.source_game_id.clone())
            .unwrap_or_else(|| game_id.to_string()),
        game_name: snap
            .as_ref()
            .map(|p| p.game_name.clone())
            .unwrap_or_default(),
        bytes_done: 0,
        bytes_total: snap.as_ref().map(|p| p.bytes_total).unwrap_or(0),
        current_file: String::new(),
        status: "canceled".into(),
        message: None,
        new_game_id: None,
        bytes_per_second: 0.0,
        cover_image_path: snap.and_then(|p| p.cover_image_path),
    };
    download_state.set(Some(terminal.clone()));
    on_progress(&terminal);
    Err(err)
}

/// Hold the install slot for [`TERMINAL_STATE_GRACE`] in a detached task, then
/// release it (the guard's token-gated `Drop` clears the slot). The
/// manifest-phase early-return paths in `begin_install` publish a terminal
/// `canceled`/`error` snapshot and then return; without this the `slot_guard`
/// local would drop the instant `begin_install` returns, clearing the slot
/// before a polling consumer (the Decky plugin) could read the terminal state.
/// Mirrors the grace the spawned transfer task already gives the success and
/// late-failure paths.
fn spawn_slot_grace(guard: DownloadGuard) {
    tokio::spawn(async move {
        tokio::time::sleep(TERMINAL_STATE_GRACE).await;
        drop(guard);
    });
}

/// Returns the `install_token` (UUID) once the transfer has been queued.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn begin_install(
    peer_addr: String,
    peer_port: u16,
    game_id: String,
    http: reqwest::Client,
    download_state: Arc<LanDownloadState>,
    on_progress: Arc<dyn Fn(&DownloadProgress) + Send + Sync>,
    max_bps: f64,
    install_root: PathBuf,
    library: SharedLibrary,
    on_library_changed: Arc<dyn Fn(&str) + Send + Sync>,
    on_success: Option<Arc<dyn Fn(String, PeerGameManifest) + Send + Sync>>,
) -> AppResult<String> {
    if peer_port == 0 {
        return Err(AppError::Other(
            "peer is discovery-only (no file server)".into(),
        ));
    }

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
    // Claim the single install slot and hold it for the whole operation.
    // The guard is moved into the spawned transfer task below, so the
    // slot stays occupied across the manifest fetch (up to
    // MANIFEST_FETCH_TIMEOUT): a concurrent `begin_install` is rejected,
    // `request_cancel` can find this session, and a manifest-fetch
    // failure routes through `update`/`set` to emit a terminal event.
    // The guard's token-gated `Drop` releases the slot on every exit
    // (success, error, cancel, panic, or the early returns below).
    let slot_guard = download_state.try_start(placeholder.clone())?;
    on_progress(&placeholder);

    // Fetch the manifest. Uses MANIFEST_FETCH_TIMEOUT because the host
    // blake3-hashes the folder on first request (~1 s/GB). Pass our
    // session token so the sender can register the upload session
    // immediately and show the game in their Transfers panel while we
    // are still in the "Fetching manifest…" phase.
    let manifest_url = format!(
        "http://{peer_addr}:{peer_port}/games/{game_id}/manifest?session={}",
        urlencoding::encode(&install_token),
    );
    let manifest_fetch = async {
        let resp = http
            .get(&manifest_url)
            .timeout(MANIFEST_FETCH_TIMEOUT)
            .send()
            .await
            .map_err(|e| {
                AppError::Other(format!("GET manifest: {}", format_reqwest_error(&e)))
            })?;
        if !resp.status().is_success() {
            return Err(AppError::Other(format!(
                "peer responded {} to /manifest",
                resp.status()
            )));
        }
        resp.json::<PeerGameManifest>()
            .await
            .map_err(|e| AppError::Other(format!("parse manifest: {e}")))
    };

    // Race the manifest fetch (bounded by MANIFEST_FETCH_TIMEOUT, up to
    // ~5 minutes on a multi-GB game the host has to hash) against a
    // short cancel-poll. The fetch's HTTP request itself isn't
    // cancellable, so without this a user who taps Cancel on a wedged
    // "Fetching manifest…" would wait the full timeout. Since the slot
    // is held by *this* install and `try_start` cleared the cancel flag,
    // an observed `is_canceled()` means our own token was cancelled via
    // `request_cancel`. `tokio::select!` drops the losing branch's
    // future when one resolves, so a cancel drops the in-flight request.
    let manifest_result: AppResult<PeerGameManifest> = tokio::select! {
        biased;
        () = async {
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            loop {
                tick.tick().await;
                if download_state.is_canceled() {
                    return;
                }
            }
        } => Err(download_state.cancel_error()),
        m = manifest_fetch => m,
    };

    let manifest = match manifest_result {
        // A cancel can also land in the gap between the fetch resolving
        // and the spawn below — re-check here so it surfaces as a
        // cancellation rather than proceeding into the transfer.
        Ok(m) if !download_state.is_canceled() => m,
        Ok(_) => {
            let r = emit_terminal_cancel(&download_state, &on_progress, &peer_addr, &game_id);
            spawn_slot_grace(slot_guard);
            return r;
        }
        Err(e) if e.is_canceled() => {
            let r = emit_terminal_cancel(&download_state, &on_progress, &peer_addr, &game_id);
            spawn_slot_grace(slot_guard);
            return r;
        }
        Err(e) => {
            // The slot is still held, so `update` finds the placeholder and the
            // UI gets a terminal "error" event instead of staying stuck on
            // "Fetching manifest…". Hand the guard to a grace holder so the slot
            // keeps showing "error" briefly for poll-based consumers before it
            // frees, rather than dropping the instant we return.
            if let Some(snap) = download_state.update(|p| {
                p.status = "error".into();
                p.message = Some(format!("{e}"));
            }) {
                on_progress(&snap);
            }
            spawn_slot_grace(slot_guard);
            return Err(e);
        }
    };

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
    download_state.set(Some(progress.clone()));
    on_progress(&progress);

    // Prefetch the cover image in the background so the transfer-panel
    // row has a thumbnail to render while files stream. Best-effort.
    {
        let http_c = http.clone();
        let ds = download_state.clone();
        let op = on_progress.clone();
        let cover_token = install_token.clone();
        let safe_name = manifest.safe_name.clone();
        let cover_peer_addr = peer_addr.clone();
        let cover_source_id = manifest.game_id.clone();
        tokio::spawn(async move {
            let covers_dir = paths::covers_dir();
            if tokio::fs::create_dir_all(&covers_dir).await.is_err() {
                return;
            }
            let url = format!(
                "http://{cover_peer_addr}:{peer_port}/games/{cover_source_id}/cover"
            );
            let Some(path) =
                fetch_and_save_peer_image(&http_c, &url, &covers_dir, &safe_name, "").await
            else {
                return;
            };
            let path_str = path.to_string_lossy().to_string();
            let matched = ds
                .snapshot()
                .map(|p| p.install_token == cover_token)
                .unwrap_or(false);
            if !matched {
                return;
            }
            if let Some(snap) = ds.update(|p| {
                p.cover_image_path = Some(path_str);
            }) {
                op(&snap);
            }
        });
    }

    let ctx = Arc::new(TransferCtx {
        http,
        state: download_state.clone(),
        on_progress: on_progress.clone(),
    });

    // Spawn the heavy transfer + library-registration work. `slot_guard`
    // moves in here so the slot stays held until this task ends; its Drop
    // (token-gated) releases it after the terminal-state grace period,
    // even if `run_install` panics.
    tokio::spawn(async move {
        let _slot_guard = slot_guard;
        let result = run_install(
            ctx.clone(),
            peer_addr,
            peer_port,
            manifest.clone(),
            max_bps,
            install_root,
            library,
            on_library_changed,
        )
        .await;

        // Preserve the prefetched cover across the terminal event.
        let cover_carry = ctx.state.snapshot().and_then(|p| p.cover_image_path);

        let final_progress = match result {
            Ok(ref new_id) => {
                // Fire post-install artwork hook before emitting "done".
                if let Some(hook) = &on_success {
                    hook(new_id.clone(), manifest.clone());
                }
                DownloadProgress {
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
                    new_game_id: Some(new_id.clone()),
                    bytes_per_second: 0.0,
                    cover_image_path: cover_carry.clone(),
                }
            }
            Err(e) => {
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

        ctx.state.set(Some(final_progress.clone()));
        (ctx.on_progress)(&final_progress);
        // Brief grace period so the UI can pick up the terminal state
        // before the slot clears. `_slot_guard`'s Drop then releases the
        // slot (token-gated, so a new install started during this window
        // is left untouched) as the task ends.
        tokio::time::sleep(TERMINAL_STATE_GRACE).await;
    });

    Ok(return_token)
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

    fn pf(path: &str) -> PeerFile {
        PeerFile {
            path: path.to_string(),
            size: 0,
            hash: String::new(),
            mtime_unix_ms: 0,
        }
    }

    #[test]
    fn manifest_paths_accept_normal_relative_paths() {
        let m = PeerGameManifest {
            files: vec![pf("saves/profile.sav"), pf("data\\bin\\game.dat")],
            exe_relative_path: Some("bin/game.exe".to_string()),
            ..Default::default()
        };
        assert_eq!(first_unsafe_manifest_path(&m), None);
    }

    #[test]
    fn manifest_paths_reject_parent_traversal_in_file() {
        let m = PeerGameManifest {
            files: vec![pf("ok.dat"), pf("../../../../etc/cron.d/evil")],
            ..Default::default()
        };
        assert_eq!(
            first_unsafe_manifest_path(&m),
            Some("../../../../etc/cron.d/evil")
        );
    }

    #[test]
    fn manifest_paths_reject_absolute_and_backslash_escape() {
        let abs = PeerGameManifest {
            files: vec![pf("/etc/passwd")],
            ..Default::default()
        };
        assert!(first_unsafe_manifest_path(&abs).is_some());

        let backslash = PeerGameManifest {
            files: vec![pf("a\\..\\..\\Start Menu\\evil.lnk")],
            ..Default::default()
        };
        assert!(first_unsafe_manifest_path(&backslash).is_some());
    }

    #[test]
    fn manifest_paths_reject_traversal_in_exe_path() {
        let m = PeerGameManifest {
            files: vec![pf("game.dat")],
            exe_relative_path: Some("../../evil.exe".to_string()),
            ..Default::default()
        };
        assert_eq!(first_unsafe_manifest_path(&m), Some("../../evil.exe"));
    }
}

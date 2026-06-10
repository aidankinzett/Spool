//! Move a game's install folder to another drive / library folder.
//!
//! Like Steam's "Move install folder…": the user picks a destination library
//! folder (Settings → Library folders) and the game's files are relocated there,
//! the library entry is repointed, and any launcher/Steam shortcut that baked in
//! the old exe path is regenerated. Saves and Proton prefixes are untouched —
//! they live in Spool's app-data, independent of the install location.
//!
//! Mechanics:
//!   * Same-filesystem destination → a single atomic `rename` (instant).
//!   * Cross-drive destination (the common case) → recursive copy into a
//!     `<dest>.partial` staging dir, verify file count + sizes, rename into
//!     place, repoint the entry, then delete the source. The source is only
//!     removed after the copy is fully verified, so an interrupted move never
//!     loses data — at worst it leaves a `.partial` dir to retry.
//!
//! Single in-flight move slot with a cooperative cancel flag, mirroring the LAN
//! install model in `lan/install.rs`. The per-game run lock (`proc_lock`) is held
//! for the whole move so a game can't launch — or be wiped — mid-relocation.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::{GameEntry, SharedLibrary};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

/// Minimum gap between `move:progress` emissions during the copy. The recursive
/// copy reports after every file; without throttling a game full of tiny files
/// would flood the IPC channel.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

/// errno returned by `rename(2)` / `MoveFile` when source and destination are on
/// different filesystems — the signal to fall back to copy + delete.
#[cfg(windows)]
const CROSS_DEVICE_ERRNO: i32 = 17; // ERROR_NOT_SAME_DEVICE
#[cfg(not(windows))]
const CROSS_DEVICE_ERRNO: i32 = 18; // EXDEV

/// Snapshot of an in-flight (or just-finished) move. Emitted as `move:progress`
/// and held in [`MoveState`] so a late-mounting UI can catch up.
#[derive(Debug, Clone, Serialize)]
pub struct MoveProgress {
    pub game_id: String,
    pub game_name: String,
    pub copied_bytes: u64,
    pub total_bytes: u64,
    /// "preparing" | "copying" | "finalizing" | "done" | "error" | "canceled"
    pub status: String,
    pub message: Option<String>,
    /// Destination install folder once known, for display.
    pub dest_folder: Option<String>,
}

/// Single-slot in-flight move tracker. One move at a time keeps the UX and disk
/// IO predictable — same model as `LanDownloadState`.
#[derive(Default)]
pub struct MoveState {
    current: Mutex<Option<MoveProgress>>,
    cancel_flag: AtomicBool,
}

impl MoveState {
    /// Claims the move slot for `game_id`, returning an RAII guard that frees it
    /// on drop (even on panic). Rejects a second concurrent move.
    fn try_start(self: &Arc<Self>, progress: MoveProgress) -> AppResult<MoveGuard> {
        let mut guard = self.current.lock().map_err(|_| AppError::LockPoisoned)?;
        if guard.is_some() {
            return Err(AppError::Other(
                "Another move is already in progress. Wait for it to finish.".into(),
            ));
        }
        self.cancel_flag.store(false, Ordering::Relaxed);
        *guard = Some(progress);
        Ok(MoveGuard { state: self.clone() })
    }

    /// Requests cancellation iff `game_id` matches the in-flight move. The copy
    /// loop polls the flag between files. Returns true if a move was cancelled.
    pub fn request_cancel(&self, game_id: &str) -> bool {
        let guard = match self.current.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match guard.as_ref() {
            Some(p) if p.game_id == game_id => {
                self.cancel_flag.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    fn is_canceled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> Option<MoveProgress> {
        self.current.lock().ok().and_then(|g| g.clone())
    }

    fn set(&self, value: Option<MoveProgress>) {
        if let Ok(mut g) = self.current.lock() {
            *g = value;
        }
    }

    fn update<F: FnOnce(&mut MoveProgress)>(&self, f: F) -> Option<MoveProgress> {
        let mut guard = self.current.lock().ok()?;
        if let Some(p) = guard.as_mut() {
            f(p);
            return Some(p.clone());
        }
        None
    }
}

/// RAII guard clearing the move slot when the operation ends. Mirrors
/// `runner::RunGuard` — without it a panicked move would jam the slot until
/// restart.
struct MoveGuard {
    state: Arc<MoveState>,
}

impl Drop for MoveGuard {
    fn drop(&mut self) {
        self.state.set(None);
    }
}

/// Snapshot of the active move (if any), for a UI mounting mid-transfer.
#[tauri::command]
pub fn current_move(state: State<'_, Arc<MoveState>>) -> Option<MoveProgress> {
    state.snapshot()
}

/// Requests cancellation of the in-flight move for `game_id`. Returns true if a
/// matching move was running. The copy loop cleans up its `.partial` dir on the
/// way out, leaving the source intact.
#[tauri::command]
pub fn cancel_move(state: State<'_, Arc<MoveState>>, game_id: String) -> bool {
    state.request_cancel(&game_id)
}

fn emit(app: &AppHandle, state: &MoveState, mutate: impl FnOnce(&mut MoveProgress)) {
    if let Some(snap) = state.update(mutate) {
        if let Err(e) = app.emit("move:progress", &snap) {
            tracing::warn!(error = %e, "failed to emit move:progress");
        }
    }
}

/// Moves game `id`'s install folder into `dest_folder` (a library folder). On
/// success returns the updated entry and emits `library:changed`.
///
/// Validation: the game must have an install folder that exists; its exe (when
/// set) must live inside that folder so it can be repointed; the destination
/// must be a different location with enough free space.
#[tauri::command]
pub async fn move_game_install(
    app: AppHandle,
    library: State<'_, SharedLibrary>,
    config: State<'_, SharedConfig>,
    move_state: State<'_, Arc<MoveState>>,
    uploads: State<'_, crate::lan::LanUploadsState>,
    id: String,
    dest_folder: String,
) -> AppResult<GameEntry> {
    let library: SharedLibrary = (*library).clone();
    let move_state: Arc<MoveState> = (*move_state).clone();

    // Snapshot the entry before any IO.
    let entry = library
        .find(&id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game with id {id} not found")))?;
    let game_name = entry.game_name.clone();

    let src_folder = entry
        .game_folder_path
        .clone()
        .filter(|f| !f.trim().is_empty())
        .ok_or_else(|| AppError::Other("This game has no install folder to move.".into()))?;
    let src = PathBuf::from(&src_folder);
    if !src.is_dir() {
        return Err(AppError::Other(format!(
            "Install folder doesn't exist on disk: {src_folder}"
        )));
    }

    // The exe must sit inside the install folder so we can repoint it after the
    // move. An empty exe (uninstalled-then-folder-only edge) is allowed and stays
    // empty; an exe outside the folder is refused rather than guessed.
    let rel_exe = if entry.exe_path.trim().is_empty() {
        None
    } else {
        let exe = PathBuf::from(&entry.exe_path);
        match relative_inside(&exe, &src) {
            Some(rel) => Some(rel),
            None => {
                return Err(AppError::Other(
                    "The game's executable is outside its install folder, so it can't be moved automatically. Move it by hand and re-point the install folder.".into(),
                ))
            }
        }
    };

    // Destination = <library folder>/<source folder name>, preserving the
    // on-disk folder name so the relative exe path stays valid.
    let base = src
        .file_name()
        .ok_or_else(|| AppError::Other("Couldn't read the install folder name.".into()))?;
    let dest_root = PathBuf::from(dest_folder.trim());
    let dest = dest_root.join(base);

    // Reject no-op / colliding destinations.
    if paths_equal(&src, &dest) {
        return Err(AppError::Other("The game is already in that folder.".into()));
    }
    if dest.exists() {
        return Err(AppError::Other(format!(
            "A folder named '{}' already exists in the destination.",
            base.to_string_lossy()
        )));
    }

    // Reject a destination inside the game's own folder — copying a tree into its
    // own subtree would recurse and fill the disk. Canonicalize both sides first
    // so a symlinked/junctioned library folder that resolves inside the install
    // dir can't slip past a purely lexical check (falls back to the lexical paths
    // when a side doesn't resolve yet, e.g. a not-yet-created destination).
    let src_real = std::fs::canonicalize(&src).unwrap_or_else(|_| src.clone());
    let dest_root_real = std::fs::canonicalize(&dest_root).unwrap_or_else(|_| dest_root.clone());
    if dest_root_real.starts_with(&src_real) {
        return Err(AppError::Other(
            "That library folder is inside the game's own install folder. Pick a different drive or folder.".into(),
        ));
    }

    // Refuse to relocate files a peer is mid-download on — the LAN server
    // serves `/file` straight out of `game_folder_path`, so deleting the source
    // after the copy would abort the peer's transfer.
    if uploads.has_active_upload(&id) {
        return Err(AppError::Other(
            "This game is being shared to another device right now. Wait for the transfer to finish (or cancel it) and try again.".into(),
        ));
    }

    // Claim the single move slot + the per-game run lock for the whole move, so
    // the game can't launch or be wiped while its files are in flight. The total
    // size is unknown until we know this is a cross-drive copy — the same-
    // filesystem fast path needs neither a size walk nor a free-space check, so
    // both are deferred into `run_move`'s copy branch.
    let _slot = move_state.try_start(MoveProgress {
        game_id: id.clone(),
        game_name: game_name.clone(),
        copied_bytes: 0,
        total_bytes: 0,
        status: "preparing".into(),
        message: None,
        dest_folder: Some(dest.to_string_lossy().to_string()),
    })?;
    let _run_lock = crate::proc_lock::try_acquire_run(&id)?.ok_or_else(|| {
        AppError::Other(
            "This game is busy — it's running, or finishing a save backup. Close it and try again."
                .into(),
        )
    })?;

    emit(&app, &move_state, |p| p.status = "copying".into());

    // Move the bytes. Fast path: a same-filesystem rename (instant). Fallback:
    // size + free-space check, then copy into a `.partial` dir, verify, and swap
    // into place. The source is NOT deleted by `run_move` — we delete it only
    // after the entry is repointed below, so a failure between the copy and the
    // DB write can never leave the entry pointing at a deleted folder.
    let outcome = match run_move(&app, &move_state, &src, &dest).await {
        Ok(o) => o,
        Err(e) => {
            let msg = e.to_string();
            let status = if e.is_canceled() { "canceled" } else { "error" };
            emit(&app, &move_state, |p| {
                p.status = status.into();
                p.message = Some(msg);
            });
            return Err(e);
        }
    };

    emit(&app, &move_state, |p| p.status = "finalizing".into());

    // Repoint the entry: new folder, new exe (joined under the new folder), and —
    // for the copy path, where we walked the tree — a refreshed install size. The
    // fast-path rename leaves the size untouched (a move doesn't change it).
    let dest_str = dest.to_string_lossy().to_string();
    let new_exe = match &rel_exe {
        Some(rel) => dest.join(rel).to_string_lossy().to_string(),
        None => String::new(),
    };
    let mut fields = vec![
        ("game_folder_path", serde_json::json!(dest_str)),
        ("exe_path", serde_json::json!(new_exe)),
    ];
    // A Wine-prefix override living inside the install folder travels with it —
    // repoint it too, or the entry would reference the deleted source and the
    // next launch would silently create a fresh empty prefix.
    if let Some(rel) = entry
        .wine_prefix_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .and_then(|p| relative_inside(Path::new(p), &src))
    {
        let new_prefix = dest.join(rel).to_string_lossy().to_string();
        fields.push(("wine_prefix_path", serde_json::json!(new_prefix)));
    }
    if let Some(total_bytes) = outcome.total_bytes {
        let install_size_mb = crate::size_backfill::bytes_to_mb(total_bytes);
        fields.push(("install_size_mb", serde_json::json!(install_size_mb)));
    }
    if let Err(e) = library.update_fields(&id, &fields).await {
        // The same-filesystem fast path already consumed the source (the copy
        // path hasn't deleted it yet — `source_to_delete` is still `Some`). Roll
        // the rename back so the entry keeps pointing at a folder that exists.
        if outcome.source_to_delete.is_none() {
            if let Err(rb) = tokio::fs::rename(&dest, &src).await {
                tracing::warn!(
                    error = %rb,
                    "move: rollback rename failed after DB repoint error — files are at the new path but the entry still points at the old one",
                );
            }
        } else {
            // Copy path: the source is still intact, so delete the verified
            // copy at the destination — leaving it would strand the disk space
            // and block every retry on the `dest.exists()` precondition.
            if let Err(rb) = tokio::fs::remove_dir_all(&dest).await {
                tracing::warn!(
                    error = %rb,
                    "move: couldn't remove the copied destination after DB repoint error",
                );
            }
        }
        return Err(e);
    }

    // The entry now points at the new location, so it's safe to delete the old
    // copy (cross-device path only — the rename already consumed the source).
    // Best-effort: a failed delete only leaves reclaimable disk, not a broken game.
    if let Some(old_src) = outcome.source_to_delete {
        match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&old_src)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "move: couldn't delete source after copy"),
            Err(e) => tracing::warn!(error = %e, "move: source delete task join failed"),
        }
    }

    // Regenerate anything that baked in the old absolute exe path. Best-effort —
    // the move itself succeeded and the entry is already correct, so a failure
    // here is logged, not fatal.
    regenerate_shortcuts(&library, &config, &entry, &new_exe).await;

    // `<base>` custom-save templates expand from `game_folder_path`, so
    // re-project ludusavi's customGames block now rather than waiting for the
    // next launch preflight — otherwise a manual or Decky game-stop backup in
    // that window would back up the deleted old path.
    crate::custom_saves::sync_best_effort(&app).await;

    emit(&app, &move_state, |p| {
        p.copied_bytes = p.total_bytes;
        p.status = "done".into();
    });
    if let Err(e) = app.emit("library:changed", &id) {
        tracing::warn!(error = %e, "library:changed emit failed after move");
    }

    library
        .find(&id)
        .await?
        .ok_or_else(|| AppError::Other("game vanished after move".into()))
}

/// Outcome of [`run_move`].
struct MoveOutcome {
    /// `Some(bytes)` when the cross-device copy path ran (the tree was walked);
    /// `None` for the same-filesystem rename fast path.
    total_bytes: Option<u64>,
    /// The source folder to delete once the entry is repointed — `Some` only for
    /// the copy path (the rename already consumed the source).
    source_to_delete: Option<PathBuf>,
}

/// Performs the actual relocation, leaving the source in place for the caller to
/// delete after the library entry is repointed. Errors leave the source intact.
async fn run_move(
    app: &AppHandle,
    state: &Arc<MoveState>,
    src: &Path,
    dest: &Path,
) -> AppResult<MoveOutcome> {
    // Ensure the destination's parent exists for both the rename and the copy.
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Other(format!("create dest root {parent:?}: {e}")))?;
    }

    // Fast path: atomic rename. Succeeds instantly within one filesystem (no size
    // walk or free-space check needed); fails with a cross-device errno when src
    // and dest are on different drives.
    match std::fs::rename(src, dest) {
        Ok(()) => {
            emit(app, state, |p| p.copied_bytes = p.total_bytes);
            return Ok(MoveOutcome { total_bytes: None, source_to_delete: None });
        }
        Err(e) if e.raw_os_error() == Some(CROSS_DEVICE_ERRNO) => {
            tracing::info!("move: cross-device, falling back to copy + delete");
        }
        Err(e) => return Err(AppError::Other(format!("move (rename): {e}"))),
    }

    // Cross-device: size the source, then confirm the destination has room before
    // copying a single byte. Both run off the async runtime (sysinfo's disk
    // refresh and the recursive stat-walk can block). The (count, bytes) pair
    // doubles as the copy-verification fingerprint below — the run lock holds
    // the tree still for the whole move, so pre-copy stats stay valid.
    let src_stats = {
        let src = src.to_path_buf();
        tokio::task::spawn_blocking(move || crate::size_backfill::directory_stats(&src))
            .await
            .map_err(|e| AppError::Other(format!("size walk join failed: {e}")))?
    };
    let total_bytes = src_stats.1;
    let dest_root = dest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dest.to_path_buf());
    let free = {
        let dest_root = dest_root.clone();
        tokio::task::spawn_blocking(move || crate::drives::free_space_for(&dest_root))
            .await
            .map_err(|e| AppError::Other(format!("free-space probe join failed: {e}")))?
    };
    // Require headroom beyond the file-byte total: cluster rounding and
    // directory metadata make the on-disk footprint larger, so an exact fit
    // would run for the whole copy and then die with "no space left on device".
    let needed = total_bytes.saturating_add((total_bytes / 100).max(256 * 1024 * 1024));
    if free > 0 && free < needed {
        return Err(AppError::Other(format!(
            "Not enough free space at the destination ({} free, {} needed).",
            human_bytes(free),
            human_bytes(needed)
        )));
    }
    // Publish the now-known total so the progress bar has a denominator.
    emit(app, state, |p| p.total_bytes = total_bytes);

    // Copy into a `.partial` staging dir (sibling of dest). Build the name by
    // appending the suffix to the full folder name rather than `with_extension`,
    // which would mangle a folder name that contains a dot.
    let partial = {
        let mut name = dest.file_name().unwrap_or_default().to_os_string();
        name.push(".partial");
        dest.with_file_name(name)
    };
    if partial.exists() {
        tokio::fs::remove_dir_all(&partial)
            .await
            .map_err(|e| AppError::Other(format!("clear stale partial {partial:?}: {e}")))?;
    }

    let app_for_copy = app.clone();
    let state_for_copy = state.clone();
    let src_copy = src.to_path_buf();
    let partial_copy = partial.clone();
    let copy_result: AppResult<()> = tokio::task::spawn_blocking(move || {
        let last_emit = Mutex::new(Instant::now() - PROGRESS_EMIT_INTERVAL);
        let mut copied: u64 = 0;
        copy_dir_recursive(&src_copy, &partial_copy, &mut copied, &state_for_copy, &|done| {
            // Throttled progress emit from the blocking copy thread.
            let should = match last_emit.lock() {
                Ok(mut le) if le.elapsed() >= PROGRESS_EMIT_INTERVAL => {
                    *le = Instant::now();
                    true
                }
                _ => false,
            };
            if should {
                emit(&app_for_copy, &state_for_copy, |p| p.copied_bytes = done);
            }
        })
    })
    .await
    .map_err(|e| AppError::Other(format!("copy task join failed: {e}")))?;

    if let Err(e) = copy_result {
        // Clean up the partial dir; source is untouched.
        let _ = tokio::fs::remove_dir_all(&partial).await;
        return Err(e);
    }

    // Verify the copy before the caller deletes the source: equal file count +
    // total size against the pre-copy fingerprint (the run lock guarantees the
    // source hasn't changed, so re-walking it would only repeat the first walk).
    let partial_for_verify = partial.clone();
    let verified = tokio::task::spawn_blocking(move || {
        crate::size_backfill::directory_stats(&partial_for_verify) == src_stats
    })
    .await
    .map_err(|e| AppError::Other(format!("verify task join failed: {e}")))?;
    if !verified {
        let _ = tokio::fs::remove_dir_all(&partial).await;
        return Err(AppError::Other(
            "Copy verification failed (file count or size mismatch). The original was left untouched.".into(),
        ));
    }

    // Swap the staging dir into place. On failure, clean up the partial so a
    // retry starts fresh; the source is still intact.
    if let Err(e) = tokio::fs::rename(&partial, dest).await {
        let _ = tokio::fs::remove_dir_all(&partial).await;
        return Err(AppError::Other(format!("finalise move dir: {e}")));
    }
    Ok(MoveOutcome {
        total_bytes: Some(total_bytes),
        source_to_delete: Some(src.to_path_buf()),
    })
}

/// Recursively copies `src` into `dst`, summing copied bytes into `copied` and
/// reporting the running total via `on_progress`. Polls `cancel`'s flag between
/// entries so a cancel aborts promptly. Symlinks and regular files both go through
/// `std::fs::copy` (which follows file symlinks, matching the `follow_links`
/// directory-size walk); a symlink to a directory will error, aborting the move
/// with the source intact.
fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    copied: &mut u64,
    cancel: &MoveState,
    on_progress: &dyn Fn(u64),
) -> AppResult<()> {
    std::fs::create_dir_all(dst)
        .map_err(|e| AppError::Other(format!("mkdir {dst:?}: {e}")))?;
    for entry in std::fs::read_dir(src).map_err(|e| AppError::Other(format!("readdir {src:?}: {e}")))? {
        if cancel.is_canceled() {
            return Err(AppError::Canceled);
        }
        let entry = entry.map_err(|e| AppError::Other(format!("readdir entry: {e}")))?;
        let file_type = entry
            .file_type()
            .map_err(|e| AppError::Other(format!("file_type: {e}")))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to, copied, cancel, on_progress)?;
        } else {
            let n = std::fs::copy(&from, &to)
                .map_err(|e| AppError::Other(format!("copy {from:?} → {to:?}: {e}")))?;
            *copied += n;
            on_progress(*copied);
        }
    }
    Ok(())
}

/// Regenerates launcher stubs / Steam shortcuts that embed the absolute exe path.
/// Both are best-effort and only run when the entry actually had that integration.
async fn regenerate_shortcuts(
    library: &SharedLibrary,
    config: &SharedConfig,
    entry: &GameEntry,
    new_exe: &str,
) {
    // Armoury Crate launcher stub (Windows): re-stamp it with the new exe path.
    // The caller emits `library:changed` once the whole move is finalised.
    if entry
        .launcher_exe_path
        .as_deref()
        .map(|p| !p.trim().is_empty())
        .unwrap_or(false)
    {
        // `.ok()` drops the poisoned-lock guard before the await below.
        let spool_exe = config.lock().ok().map(|c| c.data.spool_exe.clone());
        match spool_exe {
            Some(spool_exe) => {
                if let Err(e) =
                    crate::launcher::write_launcher(library, &spool_exe, &entry.id).await
                {
                    tracing::warn!(error = %e, "move: failed to regenerate Armoury launcher");
                }
            }
            // A poisoned config lock: skip the re-stamp rather than write a
            // stub with an empty spool path (the original launcher command
            // errors out in this state too).
            None => tracing::warn!("move: config lock poisoned; skipping Armoury launcher re-stamp"),
        }
    }

    // Steam shortcut: re-point its launch options at the new exe. The name is
    // unchanged, so the app id is stable — reuse the rename reconciler.
    if let Some(app_id) = entry.steam_app_id {
        if let Err(e) =
            crate::steam::reconcile_renamed_game(app_id, &entry.game_name, new_exe).await
        {
            tracing::warn!(error = %e, "move: failed to update Steam shortcut after move");
        }
    }
}

/// Returns `exe` as a relative `PathBuf` under `folder`, or `None` if `exe` is
/// not inside `folder`. Keeps only normal path components.
///
/// On Windows, where the filesystem is case-insensitive, a plain `strip_prefix`
/// (which matches components case-sensitively) would wrongly reject an exe whose
/// stored casing differs from the folder's — e.g. `C:\Games\Foo` vs
/// `c:\games\foo\game.exe`, common when one path came from a file dialog and the
/// other from manifest detection. So on a case-sensitivity mismatch we retry on
/// ASCII-lowercased copies and slice the matching tail off the *original* exe
/// (ASCII-lowercasing never changes byte length, so the offsets line up) to keep
/// the real on-disk casing in the result.
fn relative_inside(exe: &Path, folder: &Path) -> Option<PathBuf> {
    if let Ok(rel) = exe.strip_prefix(folder) {
        return collect_normal(rel);
    }
    // Windows only (case-insensitive FS): retry on ASCII-lowercased copies. Pure
    // string work — no Windows-only APIs — so it's gated with `cfg!(windows)`
    // rather than `#[cfg]`, keeping it compiled and lint-checked on every OS.
    if cfg!(windows) {
        let exe_s = exe.to_str()?;
        let folder_l = folder.to_str()?.to_ascii_lowercase();
        let folder_l = folder_l.trim_end_matches(['\\', '/']);
        if let Some(rest) = exe_s.to_ascii_lowercase().strip_prefix(folder_l) {
            // The match must end on a component boundary — without this check,
            // folder `C:\Games\My` would claim `C:\Games\MyGame\…` as inside.
            if rest.starts_with(['\\', '/']) {
                let rest = rest.trim_start_matches(['\\', '/']);
                if !rest.is_empty() {
                    // Same byte length as the lowercased tail → slice the original.
                    let tail = &exe_s[exe_s.len() - rest.len()..];
                    return collect_normal(Path::new(tail));
                }
            }
        }
    }
    None
}

/// Collects the components of `rel` into a `PathBuf`. `None` when there are
/// none (so the folder-itself case yields `None`) or when any component isn't
/// a plain name — a `..` means the exe doesn't sit plainly inside the folder,
/// and silently dropping it would repoint the exe at the wrong file after the
/// move (or defeat the caller's outside-the-folder refusal entirely).
fn collect_normal(rel: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in rel.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// True when two paths refer to the same location, comparing canonical forms
/// when both exist and falling back to a literal comparison otherwise.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Compact human-readable byte size for error messages (e.g. "12.3 GB").
fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_inside_strips_folder_prefix() {
        let folder = Path::new("/games/MyGame");
        let exe = Path::new("/games/MyGame/bin/game.exe");
        assert_eq!(relative_inside(exe, folder), Some(PathBuf::from("bin/game.exe")));
    }

    #[test]
    fn relative_inside_rejects_outside_and_self() {
        let folder = Path::new("/games/MyGame");
        assert_eq!(relative_inside(Path::new("/games/Other/game.exe"), folder), None);
        // The folder itself has no relative remainder.
        assert_eq!(relative_inside(folder, folder), None);
    }

    #[test]
    fn relative_inside_rejects_parent_components() {
        let folder = Path::new("/games/MyGame");
        // `..` inside the remainder: collapsing it would repoint the exe at a
        // path that doesn't exist after the move.
        assert_eq!(
            relative_inside(Path::new("/games/MyGame/bin/../game.exe"), folder),
            None
        );
        // `..` escaping the folder: the exe is genuinely outside.
        assert_eq!(
            relative_inside(Path::new("/games/MyGame/../Other/game.exe"), folder),
            None
        );
    }

    #[test]
    fn human_bytes_scales() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn copy_dir_recursive_replicates_tree() {
        use crate::size_backfill::directory_stats;
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(src.path().join("sub")).unwrap();
        std::fs::write(src.path().join("sub/b.bin"), b"world!").unwrap();

        let dst = tempfile::tempdir().unwrap();
        let target = dst.path().join("copy");
        let state = MoveState::default();
        let mut copied = 0u64;
        copy_dir_recursive(src.path(), &target, &mut copied, &state, &|_| {}).unwrap();

        assert_eq!(copied, 11);
        assert_eq!(directory_stats(src.path()), directory_stats(&target));
        assert_eq!(std::fs::read(target.join("sub/b.bin")).unwrap(), b"world!");
    }

    #[test]
    fn copy_dir_recursive_honours_cancel() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("a.txt"), b"hello").unwrap();
        let dst = tempfile::tempdir().unwrap();
        let state = MoveState::default();
        state.cancel_flag.store(true, Ordering::Relaxed);
        let mut copied = 0u64;
        let err = copy_dir_recursive(src.path(), &dst.path().join("c"), &mut copied, &state, &|_| {})
            .unwrap_err();
        assert!(err.is_canceled());
    }

    #[cfg(windows)]
    #[test]
    fn relative_inside_is_case_insensitive_on_windows() {
        // Stored exe casing differs from the install folder's — still inside.
        let folder = Path::new(r"C:\Games\MyGame");
        let exe = Path::new(r"c:\games\mygame\bin\game.exe");
        assert_eq!(relative_inside(exe, folder), Some(PathBuf::from(r"bin\game.exe")));
        // A genuinely different folder is still rejected.
        assert_eq!(relative_inside(Path::new(r"C:\Other\game.exe"), folder), None);
        // A sibling folder sharing a name prefix is not "inside".
        assert_eq!(
            relative_inside(Path::new(r"c:\games\mygamedeluxe\game.exe"), Path::new(r"C:\Games\MyGame")),
            None
        );
    }
}

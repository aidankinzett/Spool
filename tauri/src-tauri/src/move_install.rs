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

    // Free-space check (skip when the destination is on the same filesystem as
    // the source — a rename moves the data without consuming extra space).
    let total_bytes = {
        let src = src.clone();
        tokio::task::spawn_blocking(move || crate::size_backfill::directory_size(&src))
            .await
            .map_err(|e| AppError::Other(format!("size walk join failed: {e}")))?
    };
    let free = crate::drives::folder_free_space(dest_root.to_string_lossy().to_string());
    if free > 0 && free < total_bytes {
        return Err(AppError::Other(format!(
            "Not enough free space at the destination ({} free, {} needed).",
            human_bytes(free),
            human_bytes(total_bytes)
        )));
    }

    // Claim the single move slot + the per-game run lock for the whole move, so
    // the game can't launch or be wiped while its files are in flight.
    let _slot = move_state.try_start(MoveProgress {
        game_id: id.clone(),
        game_name: game_name.clone(),
        copied_bytes: 0,
        total_bytes,
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

    // Move the bytes. Fast path: a same-filesystem rename. Fallback: copy into a
    // `.partial` dir, verify, swap into place, delete the source.
    let copied_in_place = run_move(&app, &move_state, src.clone(), dest.clone()).await;

    let copied_in_place = match copied_in_place {
        Ok(v) => v,
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
    let _ = copied_in_place; // both paths land the files at `dest`

    emit(&app, &move_state, |p| p.status = "finalizing".into());

    // Repoint the entry: new folder, new exe (joined under the new folder), and
    // refreshed install size.
    let dest_str = dest.to_string_lossy().to_string();
    let new_exe = match &rel_exe {
        Some(rel) => dest.join(rel).to_string_lossy().to_string(),
        None => String::new(),
    };
    let install_size_mb = (total_bytes as f64) / (1024.0 * 1024.0);
    library
        .update_fields(
            &id,
            &[
                ("game_folder_path", serde_json::json!(dest_str)),
                ("exe_path", serde_json::json!(new_exe)),
                ("install_size_mb", serde_json::json!(install_size_mb)),
            ],
        )
        .await?;

    // Regenerate anything that baked in the old absolute exe path. Best-effort —
    // the move itself succeeded and the entry is already correct, so a failure
    // here is logged, not fatal.
    regenerate_shortcuts(&app, &library, &config, &entry, &new_exe).await;

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

/// Performs the actual relocation. Returns `true` when the fast-path rename moved
/// the folder in place (no source delete needed), `false` when the copy+verify
/// path was used (source already deleted here). Errors leave the source intact.
async fn run_move(
    app: &AppHandle,
    state: &Arc<MoveState>,
    src: PathBuf,
    dest: PathBuf,
) -> AppResult<bool> {
    // Ensure the destination's parent exists for both the rename and the copy.
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Other(format!("create dest root {parent:?}: {e}")))?;
    }

    // Fast path: atomic rename. Succeeds instantly within one filesystem; fails
    // with a cross-device errno when src and dest are on different drives.
    match std::fs::rename(&src, &dest) {
        Ok(()) => {
            emit(app, state, |p| p.copied_bytes = p.total_bytes);
            return Ok(true);
        }
        Err(e) if e.raw_os_error() == Some(CROSS_DEVICE_ERRNO) => {
            tracing::info!("move: cross-device, falling back to copy + delete");
        }
        Err(e) => return Err(AppError::Other(format!("move (rename): {e}"))),
    }

    // Cross-device copy into a `.partial` staging dir (sibling of dest). Build
    // the name by appending the suffix to the full folder name rather than
    // `with_extension`, which would mangle a folder name that contains a dot.
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
    let src_copy = src.clone();
    let partial_copy = partial.clone();
    let copy_result: AppResult<()> = tokio::task::spawn_blocking(move || {
        let cancel = CancelView { state: &state_for_copy };
        let last_emit = Mutex::new(Instant::now() - PROGRESS_EMIT_INTERVAL);
        let mut copied: u64 = 0;
        copy_dir_recursive(&src_copy, &partial_copy, &mut copied, &cancel, &|done| {
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

    // Verify the copy before deleting the source: equal file count + total size.
    let (src_for_verify, partial_for_verify) = (src.clone(), partial.clone());
    let verified = tokio::task::spawn_blocking(move || {
        let a = dir_stats(&src_for_verify);
        let b = dir_stats(&partial_for_verify);
        a == b
    })
    .await
    .map_err(|e| AppError::Other(format!("verify task join failed: {e}")))?;
    if !verified {
        let _ = tokio::fs::remove_dir_all(&partial).await;
        return Err(AppError::Other(
            "Copy verification failed (file count or size mismatch). The original was left untouched.".into(),
        ));
    }

    // Swap the staging dir into place, then delete the now-copied source.
    // Deletion is best-effort: the entry already points at the new location, so
    // a failed delete only leaves reclaimable disk behind, not a broken game.
    tokio::fs::rename(&partial, &dest)
        .await
        .map_err(|e| AppError::Other(format!("finalise move dir: {e}")))?;
    match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&src)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "move: couldn't delete source after copy"),
        Err(e) => tracing::warn!(error = %e, "move: source delete task join failed"),
    }
    Ok(false)
}

/// Borrowed view of the move's cancel flag, passed into the blocking copy.
struct CancelView<'a> {
    state: &'a Arc<MoveState>,
}
impl CancelView<'_> {
    fn is_canceled(&self) -> bool {
        self.state.is_canceled()
    }
}

/// Recursively copies `src` into `dst`, summing copied bytes into `copied` and
/// reporting the running total via `on_progress`. Polls `cancel` between entries
/// so a cancel aborts promptly. Symlinks and regular files both go through
/// `std::fs::copy` (which follows file symlinks, matching the `follow_links`
/// directory-size walk); a symlink to a directory will error, aborting the move
/// with the source intact.
fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    copied: &mut u64,
    cancel: &CancelView,
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

/// (file count, total bytes) for a directory tree — the verification fingerprint
/// compared between source and the copied `.partial` before the source is
/// deleted. Uses the same follow-symlinks walk as the size estimate so the two
/// numbers are computed identically.
fn dir_stats(path: &Path) -> (u64, u64) {
    let mut count = 0u64;
    let mut bytes = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(true) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            count += 1;
            bytes += meta.len();
        }
    }
    (count, bytes)
}

/// Regenerates launcher stubs / Steam shortcuts that embed the absolute exe path.
/// Both are best-effort and only run when the entry actually had that integration.
async fn regenerate_shortcuts(
    app: &AppHandle,
    library: &SharedLibrary,
    config: &SharedConfig,
    entry: &GameEntry,
    new_exe: &str,
) {
    // Armoury Crate launcher stub (Windows): re-stamp it with the new exe path.
    if entry
        .launcher_exe_path
        .as_deref()
        .map(|p| !p.trim().is_empty())
        .unwrap_or(false)
    {
        let spool_exe = config
            .lock()
            .map(|c| c.data.spool_exe.clone())
            .unwrap_or_default();
        match crate::launcher::write_launcher(library, &spool_exe, &entry.id).await {
            Ok(_) => {
                let _ = app.emit("library:changed", &entry.id);
            }
            Err(e) => tracing::warn!(error = %e, "move: failed to regenerate Armoury launcher"),
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
fn relative_inside(exe: &Path, folder: &Path) -> Option<PathBuf> {
    let rel = exe.strip_prefix(folder).ok()?;
    let parts: Vec<&std::ffi::OsStr> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut out = PathBuf::new();
    for p in parts {
        out.push(p);
    }
    Some(out)
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
    fn human_bytes_scales() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn dir_stats_counts_files_and_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("sub/b.bin"), b"world!").unwrap();
        assert_eq!(dir_stats(tmp.path()), (2, 11));
    }

    #[test]
    fn copy_dir_recursive_replicates_tree() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(src.path().join("sub")).unwrap();
        std::fs::write(src.path().join("sub/b.bin"), b"world!").unwrap();

        let dst = tempfile::tempdir().unwrap();
        let target = dst.path().join("copy");
        let state = Arc::new(MoveState::default());
        let cancel = CancelView { state: &state };
        let mut copied = 0u64;
        copy_dir_recursive(src.path(), &target, &mut copied, &cancel, &|_| {}).unwrap();

        assert_eq!(copied, 11);
        assert_eq!(dir_stats(src.path()), dir_stats(&target));
        assert_eq!(std::fs::read(target.join("sub/b.bin")).unwrap(), b"world!");
    }

    #[test]
    fn copy_dir_recursive_honours_cancel() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("a.txt"), b"hello").unwrap();
        let dst = tempfile::tempdir().unwrap();
        let state = Arc::new(MoveState::default());
        state.cancel_flag.store(true, Ordering::Relaxed);
        let cancel = CancelView { state: &state };
        let mut copied = 0u64;
        let err = copy_dir_recursive(src.path(), &dst.path().join("c"), &mut copied, &cancel, &|_| {})
            .unwrap_err();
        assert!(err.is_canceled());
    }
}

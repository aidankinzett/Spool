//! Cross-process advisory locks for side effects the database can't guard.
//!
//! The per-process run-lock in `runner.rs` (`RunState`) only serialises game
//! launches *within a single Spool process*. Several Spool processes routinely
//! run at once on one machine — the tray GUI, an attached `spool --run`, and the
//! Decky `spool --headless-server` (which runs game-stop backups for the plugin).
//! ludusavi's backup directory and the rclone
//! remote folder are a single shared tree, so two of those processes running
//! `ludusavi backup` / `cloud upload` at the same time can corrupt the backup
//! dir or last-writer-win on the remote and lose a save. The database is safe
//! (SQLite WAL) — this guards the *side effects* the database can't.
//!
//! There's also a **per-game run lock** ([`try_acquire_run`]): the run workflow
//! holds it for a whole play session and a disk-wipe (uninstall / delete) holds
//! it for the wipe, so a wipe can never delete a game's files out from under a
//! session in *another* Spool process (e.g. the Decky badge "Remove from disk"
//! while the game is playing in Game Mode) — something the in-process `RunState`
//! can't see.
//!
//! Implemented as an OS advisory file lock (`flock` on Unix, `LockFileEx` on
//! Windows, via `std::fs::File`'s native locking) on a single marker file under
//! the app dir. It's machine-wide and cross-process, and the OS releases it
//! automatically when the holding process exits — including a crash or a Steam
//! force-kill in Game Mode — so a dead holder never deadlocks the next backup.
//!
//! Acquisition is **fail-safe**: callers take the lock *before* touching
//! ludusavi or the remote and, if it stays contended past the timeout, give up
//! rather than run unlocked. Running unlocked would do the exact concurrent
//! ludusavi/rclone write the lock exists to prevent — and the live save is on
//! local disk regardless, so the right move is to defer and retry, not to risk
//! corrupting the backup or clobbering the remote. A timeout means nothing was
//! written (so there's nothing half-finished to reconcile) and that another
//! Spool process holds the lock — i.e. is *already* backing up. Contention is
//! itself uncommon: one game runs at a time, and the Decky forced-close
//! fallback only fires *after* the attached run is already dead.

use crate::error::{AppError, AppResult};
use crate::paths;
use std::fs::{File, TryLockError};
use std::path::PathBuf;
use std::time::Duration;

/// Held guard over a machine-wide advisory file lock. Dropping it releases
/// the lock; so does the process exiting (the OS frees the advisory lock).
pub struct FileLock {
    file: File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Best-effort: unlock failures are unrecoverable here and the OS frees
        // the lock on process exit regardless.
        let _ = self.file.unlock();
    }
}

/// Poll-acquire the advisory lock on `path`, returning `busy_msg` as an error on
/// timeout (or a distinct error if the file can't be opened). The poll-and-sleep
/// (instead of a blocking `lock()`) keeps a bounded timeout the OS can't give us
/// directly, and degrades a same-process re-entry to a timeout instead of a hard
/// hang.
///
/// `flock` is per *open file description*, so two guards over the *same* path
/// held at once within one process would block each other until this timeout.
/// Don't nest calls on the same lock. (Different lock files are independent, so
/// holding the backup lock and the control-plane lock together is fine.)
async fn acquire_at(path: PathBuf, timeout: Duration, busy_msg: &str) -> AppResult<FileLock> {
    let file = File::create(&path)
        .map_err(|e| AppError::Other(format!("lock: open {}: {e}", path.display())))?;

    let step = Duration::from_millis(200);
    let mut waited = Duration::ZERO;
    loop {
        // Non-blocking attempt; the blocking `lock()` can't be cancelled by the
        // tokio timeout, so we poll and sleep between tries instead.
        match file.try_lock() {
            Ok(()) => return Ok(FileLock { file }),
            Err(TryLockError::WouldBlock) => {
                if waited >= timeout {
                    return Err(AppError::Other(busy_msg.to_string()));
                }
                tokio::time::sleep(step).await;
                waited += step;
            }
            Err(TryLockError::Error(e)) => {
                return Err(AppError::Other(format!("lock: {e}")));
            }
        }
    }
}

/// Acquire the machine-wide backup/upload lock, polling until a holder in
/// another process finishes or `timeout` elapses. Returns an error on timeout
/// (or if the lock file can't be opened) — callers fail the backup and let the
/// user retry rather than run unlocked. Held across the whole ludusavi backup +
/// rclone upload, so contention can last as long as a backup.
pub async fn acquire_backup(timeout: Duration) -> AppResult<FileLock> {
    acquire_at(
        paths::backup_lock_file(),
        timeout,
        "Another backup is already running on this device. Try again in a moment.",
    )
    .await
}

/// Single non-blocking attempt to take the advisory lock on `path` (creating the
/// file and any missing parent dir). `Ok(Some)` = acquired, `Ok(None)` = held by
/// another open file description (another process, or another guard on the same
/// path in this one), `Err` only if the file can't be opened.
fn try_acquire_at(path: PathBuf) -> AppResult<Option<FileLock>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Other(format!("lock: mkdir {}: {e}", parent.display())))?;
    }
    let file = File::create(&path)
        .map_err(|e| AppError::Other(format!("lock: open {}: {e}", path.display())))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(FileLock { file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(AppError::Other(format!("lock: {e}"))),
    }
}

/// Non-blocking attempt to take the machine-wide backup/upload lock. Returns
/// `Ok(Some(guard))` when it's free and was acquired, `Ok(None)` when another
/// Spool process currently holds it, and `Err` only if the lock file can't be
/// opened. The launch restore phase uses this to detect a concurrent backup
/// (e.g. the Decky forced-close backup via the headless server) so it can show a
/// "waiting for backup" splash message before blocking on [`acquire_backup`].
pub fn try_acquire_backup() -> AppResult<Option<FileLock>> {
    try_acquire_at(paths::backup_lock_file())
}

/// Non-blocking attempt to take the machine-wide **per-game run lock**. The run
/// workflow holds it across a whole play session; a disk-wipe (uninstall /
/// delete) holds it across the wipe. `Ok(Some(guard))` = acquired (free);
/// `Ok(None)` = a live session — or a concurrent wipe — in *some* Spool process
/// holds it; `Err` only if the lock file can't be opened. The OS frees it when
/// the holder exits, so a crash or Game-Mode force-kill can't wedge it.
///
/// Because `flock` is per open file description, a second `try_acquire_run` on
/// the same `game_id` within one process also returns `None` while the first
/// guard is alive — so the run-vs-wipe exclusion holds intra-process too. Don't
/// nest two run-lock guards for the same game in one call chain.
pub fn try_acquire_run(game_id: &str) -> AppResult<Option<FileLock>> {
    try_acquire_at(paths::run_lock_file(game_id))
}

/// Best-effort removal of a game's run-lock marker file once its library entry
/// is permanently retired (forget / delete-from-disk), so the per-game files
/// don't accumulate. Called from [`crate::library::Library::remove`] — never on
/// uninstall, which keeps the entry (and may run the game again).
///
/// Safe because retirement is *terminal*: `flock` lives on the open file
/// description, not the directory entry, so unlinking leaves any current holder's
/// lock intact, and the id is gone for good — every add mints a fresh UUID and
/// never reuses a retired id, so nothing recreates this marker to be locked
/// again. (A recreated marker would be a *new* inode, lockable independently of
/// an old holder's — which is exactly why cleanup only runs at terminal
/// retirement, not on uninstall.) Missing file / non-fatal errors are ignored —
/// a leftover zero-byte marker is harmless and just reused on the next launch.
pub fn remove_run_lock(game_id: &str) {
    let path = paths::run_lock_file(game_id);
    if let Err(e) = std::fs::remove_file(&path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::debug!(path = %path.display(), error = %e, "couldn't remove run-lock file");
        }
    }
}

/// Acquire the machine-wide control-plane lock, serialising the brief
/// read-modify-write of this device's cross-device blob
/// (`_spool/devices/<id>.json`) against other Spool processes on the machine.
///
/// Deliberately separate from [`acquire_backup`]: that lock is held across a
/// whole backup + upload (and the soft-deferred-backup path can't get it),
/// whereas this is held only for a millisecond-scale `cat` -> `rcat`, so
/// contention is tiny. The two are different files, so a backup path can hold
/// both at once without self-blocking.
pub async fn acquire_control_plane(timeout: Duration) -> AppResult<FileLock> {
    acquire_at(
        paths::control_plane_lock_file(),
        timeout,
        "Another Spool process is updating cross-device state. Try again in a moment.",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_lock_is_mutually_exclusive_per_game() {
        // Unique id so the lock file can't collide with other tests / runs.
        let id = "test-run-lock-self-exclusion-7f3a";
        let held = try_acquire_run(id).unwrap();
        assert!(held.is_some(), "first acquire of a free run lock succeeds");
        // A second attempt while the first guard is alive reports contention —
        // `flock` is per open file description, so this holds even in-process,
        // which is exactly the run-vs-wipe exclusion we rely on.
        assert!(
            try_acquire_run(id).unwrap().is_none(),
            "second acquire while held returns None"
        );
        drop(held);
        assert!(
            try_acquire_run(id).unwrap().is_some(),
            "re-acquire after release succeeds"
        );
    }

    #[test]
    fn run_locks_for_different_games_are_independent() {
        let _a = try_acquire_run("test-run-lock-indep-a-9b2c").unwrap().unwrap();
        // A different game id is a different lock file → independently acquirable.
        assert!(
            try_acquire_run("test-run-lock-indep-b-9b2c").unwrap().is_some(),
            "different games don't block each other"
        );
    }
}

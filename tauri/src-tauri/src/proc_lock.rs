//! Cross-process advisory lock for the save backup/upload critical section.
//!
//! The per-process run-lock in `runner.rs` (`RunState`) only serialises game
//! launches *within a single Spool process*. Several Spool processes routinely
//! run at once on one machine — the tray GUI, an attached `spool --run`, the
//! Decky `spool --headless-server`, and one-shot `spool --backup` /
//! `--release-lock` fallbacks. ludusavi's backup directory and the rclone
//! remote folder are a single shared tree, so two of those processes running
//! `ludusavi backup` / `cloud upload` at the same time can corrupt the backup
//! dir or last-writer-win on the remote and lose a save. The database is safe
//! (SQLite WAL) — this guards the *side effects* the database can't.
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

/// Non-blocking attempt to take the machine-wide backup/upload lock. Returns
/// `Ok(Some(guard))` when it's free and was acquired, `Ok(None)` when another
/// Spool process currently holds it, and `Err` only if the lock file can't be
/// opened. The launch restore phase uses this to detect a concurrent backup
/// (e.g. the Decky forced-close `--backup` fallback) so it can show a
/// "waiting for backup" splash message before blocking on [`acquire_backup`].
pub fn try_acquire_backup() -> AppResult<Option<FileLock>> {
    let path = paths::backup_lock_file();
    let file = File::create(&path)
        .map_err(|e| AppError::Other(format!("lock: open {}: {e}", path.display())))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(FileLock { file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(AppError::Other(format!("lock: {e}"))),
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

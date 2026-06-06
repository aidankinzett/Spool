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
use std::time::Duration;

/// Held guard over the machine-wide backup/upload lock. Dropping it releases
/// the lock; so does the process exiting (the OS frees the advisory lock).
pub struct BackupLock {
    file: File,
}

impl Drop for BackupLock {
    fn drop(&mut self) {
        // Best-effort: unlock failures are unrecoverable here and the OS frees
        // the lock on process exit regardless.
        let _ = self.file.unlock();
    }
}

/// Acquire the machine-wide backup/upload lock, polling until a holder in
/// another process finishes or `timeout` elapses. Returns an error on timeout
/// (or if the lock file can't be opened) — callers fail the backup and let the
/// user retry rather than run unlocked. The poll-and-sleep (instead of a
/// blocking `lock()`) keeps a bounded timeout the OS can't give us directly,
/// and degrades a same-process re-entry to a timeout instead of a hard hang.
///
/// `flock` is per *open file description*, so two `BackupLock`s held at once
/// within the same process would block each other until this timeout. Don't
/// nest calls: the backup paths that take this lock never call into one another.
pub async fn acquire_backup(timeout: Duration) -> AppResult<BackupLock> {
    let path = paths::backup_lock_file();
    let file = File::create(&path).map_err(|e| {
        AppError::Other(format!("backup lock: open {}: {e}", path.display()))
    })?;

    let step = Duration::from_millis(200);
    let mut waited = Duration::ZERO;
    loop {
        // Non-blocking attempt; the blocking `lock()` can't be cancelled by the
        // tokio timeout, so we poll and sleep between tries instead.
        match file.try_lock() {
            Ok(()) => return Ok(BackupLock { file }),
            Err(TryLockError::WouldBlock) => {
                if waited >= timeout {
                    return Err(AppError::Other(
                        "Another backup is already running on this device. Try again in a moment."
                            .into(),
                    ));
                }
                tokio::time::sleep(step).await;
                waited += step;
            }
            Err(TryLockError::Error(e)) => {
                return Err(AppError::Other(format!("backup lock: {e}")));
            }
        }
    }
}

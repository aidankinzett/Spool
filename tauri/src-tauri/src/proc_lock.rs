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
//! The lock is **best-effort serialisation, not a hard mutex**: callers acquire
//! it, hold the guard across their ludusavi/rclone work, and proceed even if
//! acquisition times out (skipping a backup loses a save, which is worse than a
//! rare unsynchronised one). Contention is itself uncommon — one game runs at a
//! time, and the Decky forced-close fallback only fires *after* the attached
//! run is already dead.

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
/// (or if the lock file can't be opened) so the caller can log-and-continue
/// rather than block a backup forever — the OS would in any case free the lock
/// if the holder had died.
///
/// `flock` is per *open file description*, so two `BackupLock`s held at once
/// within the same process would deadlock against each other. Don't nest calls:
/// the backup paths that take this lock never call into one another.
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
                        "backup lock: timed out waiting for another Spool process".into(),
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

//! Active-session record for SteamOS Game-Mode launches.
//!
//! Attached `--run` mode writes this at launch and flips `backed_up = true`
//! once a backup completes (Spool's own post-session backup, or the plugin
//! server's game-stop backup). The Decky plugin reads it on the game-stop event:
//! if `backed_up` is still false, Steam force-killed Spool before it backed up,
//! so the plugin asks the headless server to back up as a fallback.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Orders read-modify-write of the active-session record between threads of
/// *this* process. The suspend watcher (checkpointing `suspended_secs` on
/// resume) and the run workflow (flipping `backed_up` / clearing the record)
/// both mutate the file, and an unguarded read-then-write could interleave so
/// one clobbers the other's field. Held only across the synchronous read+write
/// (no await), so it can't deadlock.
///
/// This mutex is invisible to *other* processes, though — and the game-stop
/// (`/session/game-stopped`) flow runs in the separate headless server while the
/// attached `--run` workflow is still finishing its own backup, so both can do a
/// read-modify-write at once. [`lock_session`] adds a machine-wide advisory file
/// lock on top of this mutex to order those cross-process writers too.
static SESSION_LOCK: Mutex<()> = Mutex::new(());

/// Combined guard over [`SESSION_LOCK`] (orders this process's threads) and a
/// machine-wide advisory file lock on `active-session.lock` (orders the *other*
/// Spool processes — chiefly the attached suspend watcher vs. the headless
/// game-stop backup). Dropping it releases both; the OS also frees the file lock
/// when the process exits, so a crash/force-kill can't wedge it.
struct SessionGuard {
    // Drop order is declaration order: release the file lock, then the mutex.
    _file: Option<std::fs::File>,
    _mutex: std::sync::MutexGuard<'static, ()>,
}

/// Take the in-process mutex and the cross-process file lock for one short
/// read-modify-write of the record. Both are held only across the synchronous
/// read+write below (no await). If the lock file can't be opened or locked we
/// proceed with just the in-process guard rather than fail the write — a rare
/// lost update beats dropping the unsynced-session signal a backup-flag flip
/// carries. The blocking `lock()` is safe: every holder does one short RMW and
/// releases, and the OS frees it on exit.
fn lock_session() -> SessionGuard {
    let mutex = SESSION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = crate::paths::session_lock_file();
    // Make sure the app data dir exists first, so File::create can't fail (and
    // silently skip the cross-process lock) on a fresh install where it hasn't
    // been created yet. Best-effort — a real error surfaces at File::create.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::File::create(&path)
        .ok()
        .and_then(|f| f.lock().ok().map(|()| f));
    SessionGuard {
        _file: file,
        _mutex: mutex,
    }
}

/// Set once when this process writes an active-session record (only attached
/// `--run` launches do — see [`write_start`]). The in-process play-session
/// recorder consults it before adopting the record's `started_at` as the
/// session-id seed: a desktop process never wrote a record, so any record it
/// finds is a stale leftover from a past attached session it must not adopt
/// (else it would key a fresh session on an old start and dedupe it away).
static WROTE_START: AtomicBool = AtomicBool::new(false);

/// True when this process wrote the active-session record (i.e. it's the
/// attached launch the record belongs to). Desktop launches return false.
pub fn wrote_start_this_process() -> bool {
    WROTE_START.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSession {
    pub game: String,
    pub steam_appid: u32,
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub backed_up: bool,
    /// Total seconds the system has spent suspended so far this session.
    /// Checkpointed by the suspend watcher on each resume so it survives a
    /// Game-Mode force-kill; the forced-close backup subtracts it from the
    /// wall-clock duration so sleep time isn't counted as play time. Defaults
    /// to 0 for records written before this field existed.
    #[serde(default)]
    pub suspended_secs: i64,
}

/// Steam's CRC-based appid for a non-Steam shortcut. MUST match
/// `steam::upsert_spool_shortcut`'s computation so the value equals the appid
/// Steam reports to the plugin: `calculate_app_id("\"<exe>\"", game_name)`.
pub fn compute_steam_appid(spool_exe: &str, game_name: &str) -> u32 {
    crate::steam::compute_shortcut_app_id(game_name, spool_exe)
}

fn write_start_at(
    path: &Path,
    game: &str,
    steam_appid: u32,
    started_at: DateTime<Utc>,
) -> AppResult<String> {
    let guard = lock_session();
    write_record_at(&guard, path, game, steam_appid, started_at)
}

/// Writes a fresh record. Taking the guard makes the caller hold the lock.
fn write_record_at(
    _lock: &SessionGuard,
    path: &Path,
    game: &str,
    steam_appid: u32,
    started_at: DateTime<Utc>,
) -> AppResult<String> {
    let session_id = format!("{steam_appid}-{}", started_at.timestamp_millis());
    let rec = ActiveSession {
        game: game.to_string(),
        steam_appid,
        session_id: session_id.clone(),
        started_at,
        backed_up: false,
        suspended_secs: 0,
    };
    crate::paths::write_atomic(path, &serde_json::to_vec_pretty(&rec)?, false)?;
    Ok(session_id)
}

fn read_at(path: &Path) -> Option<ActiveSession> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write the session record for a launch starting now. Only attached `--run`
/// launches call this, so it also flags this process as the record's owner so
/// the play-session recorder will adopt its `started_at` (see [`WROTE_START`]).
pub fn write_start(game: &str, steam_appid: u32) -> AppResult<String> {
    let id = write_start_at(
        &crate::paths::active_session_file(),
        game,
        steam_appid,
        Utc::now(),
    )?;
    WROTE_START.store(true, Ordering::Relaxed);
    Ok(id)
}

/// Read the current session record, if any.
#[allow(dead_code)]
pub fn read() -> Option<ActiveSession> {
    read_at(&crate::paths::active_session_file())
}

/// Mark the current session's backup as done, but only when the on-disk
/// `session_id` still matches `expected_id`. No-op when no record exists or a
/// newer session has started since the backup was triggered (so a late-finishing
/// backup can't flip a different session's flag).
#[allow(dead_code)]
pub fn mark_backed_up_if(expected_id: &str) {
    mark_backed_up_if_at(&crate::paths::active_session_file(), expected_id);
}

#[allow(dead_code)]
fn mark_backed_up_if_at(path: &Path, expected_id: &str) {
    let _guard = lock_session();
    if let Some(mut rec) = read_at(path) {
        if rec.session_id == expected_id {
            rec.backed_up = true;
            if let Ok(bytes) = serde_json::to_vec_pretty(&rec) {
                let _ = crate::paths::write_atomic(path, &bytes, false);
            }
        }
    }
}

/// Checkpoint the running suspended-seconds total into the active-session
/// record so it survives a Game-Mode force-kill. Guarded by game name (the
/// suspend watcher knows the game, not the session id) so a newer session for a
/// different game can't be clobbered — only one game runs at a time, and the
/// watcher is aborted/killed at session end, so no stale writer survives. The
/// forced-close backup reads this and subtracts it from wall-clock playtime.
/// No-op when no record exists or it names a different game.
///
/// Only the Linux suspend watcher calls this, so it's dead on other platforms.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn record_suspended_secs(game_name: &str, total_secs: i64) {
    record_suspended_secs_at(&crate::paths::active_session_file(), game_name, total_secs);
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn record_suspended_secs_at(path: &Path, game_name: &str, total_secs: i64) {
    let _guard = lock_session();
    if let Some(mut rec) = read_at(path) {
        if rec.game == game_name {
            rec.suspended_secs = total_secs;
            if let Ok(bytes) = serde_json::to_vec_pretty(&rec) {
                let _ = crate::paths::write_atomic(path, &bytes, false);
            }
        }
    }
}

/// Delete the active-session record, but only when the on-disk `session_id`
/// still matches `expected_id`. Called once a session is fully reconciled
/// (backed up locally AND the saves reached the cloud, or no cloud is
/// configured) so a later read can't act on a stale, already-synced session —
/// while the id guard makes sure a backup completing late can't wipe a *newer*
/// session that has since started. No-op when no record exists or the id has
/// moved on. (#280)
pub fn clear_if(expected_id: &str) {
    clear_if_at(&crate::paths::active_session_file(), expected_id);
}

fn clear_if_at(path: &Path, expected_id: &str) {
    let _guard = lock_session();
    if let Some(rec) = read_at(path) {
        if rec.session_id == expected_id {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// True from the moment a launch attempt starts spawning the game until the
/// attempt is found to have failed. The attempt guard reads it when it drops.
///
/// One flag for the whole process is enough: `launch_game_inner_steal` arms the
/// guard only after taking the single in-process launch slot, so attempts never
/// overlap, and the previous attempt's guard has dropped before the next one
/// resets the flag.
static GAME_STARTED: AtomicBool = AtomicBool::new(false);

/// Called just before the game is spawned. From here the session record has to
/// outlive a panic or cancellation of the launch, because the game may be
/// running and the Decky forced-close backup depends on the record.
pub fn mark_game_started() {
    GAME_STARTED.store(true, Ordering::Relaxed);
}

/// Called when the spawn failed or the game crashed straight away: the attempt
/// is treated as never started, and its guard removes the record.
pub fn mark_launch_failed() {
    GAME_STARTED.store(false, Ordering::Relaxed);
}

/// Starts a launch attempt. When the returned guard drops and the game was
/// never started, the session record is removed (see [`abandon_unlaunched_at`]).
/// Hold it for the whole attempt, after the launch slot has been acquired.
pub fn begin_attempt() -> AttemptGuard {
    GAME_STARTED.store(false, Ordering::Relaxed);
    AttemptGuard
}

/// Removes this process's session record if the launch attempt ends without the
/// game having been started — whether it returned an error, panicked or was
/// cancelled before the spawn.
pub struct AttemptGuard;

impl Drop for AttemptGuard {
    fn drop(&mut self) {
        if !GAME_STARTED.load(Ordering::Relaxed) {
            abandon_unlaunched_at(
                &crate::paths::active_session_file(),
                wrote_start_this_process(),
            );
        }
    }
}

/// Drops the record for a launch that failed before the game started (the cloud was
/// unreachable, another device holds the session, the restore failed, the exe
/// wouldn't start, …). The record is written when the attached `--run` starts,
/// ahead of the workflow, with `backed_up = false`; if it stayed on disk, Steam
/// reporting the shortcut stopped would make the Decky plugin run its
/// forced-close backup against a save that was never restored, which can make a
/// stale local save the newest backup.
///
/// Only removes a record this process wrote (`owned`) that isn't already
/// flagged backed up. [`ensure_started`] writes it again if the workflow is
/// retried in the same process, which happens after a cloud conflict is
/// resolved.
fn abandon_unlaunched_at(path: &Path, owned: bool) {
    if !owned {
        return;
    }
    let _guard = lock_session();
    if let Some(rec) = read_at(path) {
        if !rec.backed_up {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Makes sure a live record exists for a game that is about to be spawned. A
/// first attempt already has the one written at startup, so this does nothing;
/// a retry after the attempt was abandoned writes it again, so the forced-close
/// fallback still covers the retried session. No-op unless this process is an
/// attached `--run` launch that wrote the record.
pub fn ensure_started(game_name: &str) {
    if !wrote_start_this_process() {
        return;
    }
    let Some(exe) = crate::paths::spool_executable() else {
        tracing::warn!("no Spool executable path; can't rewrite the active-session record");
        return;
    };
    let appid = compute_steam_appid(&exe.to_string_lossy(), game_name);
    if let Err(e) = ensure_started_at(
        &crate::paths::active_session_file(),
        game_name,
        appid,
        Utc::now(),
    ) {
        tracing::warn!(error = %e, "failed to rewrite active-session record");
    }
}

/// Writes the record unless a live one (same game, not yet backed up) is
/// already there. One lock covers the check and the write.
fn ensure_started_at(
    path: &Path,
    game: &str,
    steam_appid: u32,
    now: DateTime<Utc>,
) -> AppResult<bool> {
    let guard = lock_session();
    if read_at(path).is_some_and(|rec| rec.game == game && !rec.backed_up) {
        return Ok(false);
    }
    write_record_at(&guard, path, game, steam_appid, now)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_mark() {
        let dir = std::env::temp_dir().join(format!("spool-session-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("active-session.json");

        let now = chrono::DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let id = write_start_at(&path, "Hades", 0x8000_0001, now).unwrap();
        assert!(id.starts_with("2147483649-"));

        let rec = read_at(&path).expect("record written");
        assert_eq!(rec.game, "Hades");
        assert!(!rec.backed_up);

        // A non-matching id must not flip the flag…
        mark_backed_up_if_at(&path, "not-this-session");
        assert!(!read_at(&path).unwrap().backed_up);
        // …the matching id does.
        mark_backed_up_if_at(&path, &id);
        assert!(read_at(&path).unwrap().backed_up);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn writes_are_atomic_and_leave_no_tmp() {
        // The record is written via a tmp→rename so a crash mid-write can't
        // leave a truncated file the forced-close fallback then fails to parse.
        // After a write the dir holds only the final file, no leftover tmp. (#6)
        let dir = std::env::temp_dir().join(format!("spool-session-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("active-session.json");
        let now = Utc::now();

        let id = write_start_at(&path, "Hades", 0x8000_0001, now).unwrap();
        record_suspended_secs_at(&path, "Hades", 120);
        mark_backed_up_if_at(&path, &id);

        // Record parses cleanly and reflects every write.
        let rec = read_at(&path).expect("record parses");
        assert_eq!(rec.suspended_secs, 120);
        assert!(rec.backed_up);

        // No `*.tmp.*` sibling survived any of the writes.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "stray tmp files: {leftovers:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn record_suspended_secs_updates_matching_game_only() {
        let dir = std::env::temp_dir().join(format!("spool-session-susp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("active-session.json");
        let now = Utc::now();
        write_start_at(&path, "Hades", 0x8000_0001, now).unwrap();

        // Default is zero until the watcher checkpoints a total.
        assert_eq!(read_at(&path).unwrap().suspended_secs, 0);

        // A different game must not touch this record…
        record_suspended_secs_at(&path, "Celeste", 999);
        assert_eq!(read_at(&path).unwrap().suspended_secs, 0);
        // …the matching game writes the absolute total (overwrite, not add).
        record_suspended_secs_at(&path, "Hades", 249);
        assert_eq!(read_at(&path).unwrap().suspended_secs, 249);
        record_suspended_secs_at(&path, "Hades", 600);
        assert_eq!(read_at(&path).unwrap().suspended_secs, 600);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mark_when_absent_is_noop() {
        let path = std::env::temp_dir().join("spool-session-absent-xyz.json");
        std::fs::remove_file(&path).ok();
        mark_backed_up_if_at(&path, "anything"); // must not panic
        assert!(read_at(&path).is_none());
    }

    #[test]
    fn clear_if_matches_id_then_removes() {
        let dir = std::env::temp_dir().join(format!("spool-session-clear-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("active-session.json");
        let now = Utc::now();

        let id = write_start_at(&path, "Hades", 0x8000_0001, now).unwrap();
        // A stale (different) id must NOT remove a newer session's record.
        clear_if_at(&path, "some-other-session-id");
        assert!(read_at(&path).is_some(), "mismatched id must be a no-op");
        // The matching id removes it.
        clear_if_at(&path, &id);
        assert!(read_at(&path).is_none(), "matching id removes the record");
        // Idempotent when already gone.
        clear_if_at(&path, &id); // must not panic

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn appid_matches_steam_shortcut_formula() {
        let quoted = format!("\"{}\"", "/home/u/spool-launcher.sh");
        let expected = steam_shortcuts_util::app_id_generator::calculate_app_id(&quoted, "Hades");
        assert_eq!(
            compute_steam_appid("/home/u/spool-launcher.sh", "Hades"),
            expected
        );
        assert_eq!(
            crate::steam::compute_shortcut_app_id("Hades", "/home/u/spool-launcher.sh"),
            expected
        );
    }

    fn temp_record(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("spool-session-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("active-session.json");
        let _ = std::fs::remove_file(&path);
        (dir, path)
    }

    #[test]
    fn abandoning_a_launch_that_never_started_removes_its_record() {
        let (dir, path) = temp_record("abandon");
        write_start_at(&path, "Hades", 0x8000_0001, Utc::now()).unwrap();
        abandon_unlaunched_at(&path, true);
        assert!(read_at(&path).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn abandoning_leaves_records_it_does_not_own_or_that_are_backed_up() {
        let (dir, path) = temp_record("abandon-keep");
        let id = write_start_at(&path, "Hades", 0x8000_0001, Utc::now()).unwrap();

        // A desktop process never wrote the record, so it must not touch it.
        abandon_unlaunched_at(&path, false);
        assert!(read_at(&path).is_some());

        // A record already flagged backed up is a finished session, not a
        // failed launch.
        mark_backed_up_if_at(&path, &id);
        abandon_unlaunched_at(&path, true);
        assert!(read_at(&path).unwrap().backed_up);

        // Nothing on disk is fine.
        let _ = std::fs::remove_file(&path);
        abandon_unlaunched_at(&path, true);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ensure_started_rewrites_an_abandoned_record_for_a_retry() {
        let (dir, path) = temp_record("ensure");
        let first = chrono::DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let retry = first + chrono::Duration::minutes(3);

        let id = write_start_at(&path, "Hades", 0x8000_0001, first).unwrap();
        // A first attempt keeps the record written at startup.
        assert!(!ensure_started_at(&path, "Hades", 0x8000_0001, retry).unwrap());
        assert_eq!(read_at(&path).unwrap().started_at, first);

        // After the failed attempt was abandoned, the retry writes a fresh one.
        abandon_unlaunched_at(&path, true);
        assert!(ensure_started_at(&path, "Hades", 0x8000_0001, retry).unwrap());
        let rec = read_at(&path).unwrap();
        assert_eq!(rec.started_at, retry);
        assert!(!rec.backed_up);

        // A record already flagged backed up is not a live session: replace it.
        mark_backed_up_if_at(&path, &rec.session_id);
        assert!(ensure_started_at(&path, "Hades", 0x8000_0001, retry).unwrap());
        assert!(!read_at(&path).unwrap().backed_up);
        let _ = id;

        // A leftover record for another game is replaced.
        assert!(ensure_started_at(&path, "Celeste", 0x8000_0002, retry).unwrap());
        assert_eq!(read_at(&path).unwrap().game, "Celeste");
        std::fs::remove_dir_all(&dir).ok();
    }
}

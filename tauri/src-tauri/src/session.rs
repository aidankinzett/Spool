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
    let quoted_exe = format!("\"{}\"", spool_exe.replace('"', "\\\""));
    steam_shortcuts_util::app_id_generator::calculate_app_id(&quoted_exe, game_name)
}

fn write_start_at(path: &Path, game: &str, steam_appid: u32, started_at: DateTime<Utc>) -> AppResult<String> {
    let session_id = format!("{steam_appid}-{}", started_at.timestamp_millis());
    let rec = ActiveSession {
        game: game.to_string(),
        steam_appid,
        session_id: session_id.clone(),
        started_at,
        backed_up: false,
        suspended_secs: 0,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&rec)?)?;
    Ok(session_id)
}

fn read_at(path: &Path) -> Option<ActiveSession> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write the session record for a launch starting now.
pub fn write_start(game: &str, steam_appid: u32) -> AppResult<String> {
    write_start_at(&crate::paths::active_session_file(), game, steam_appid, Utc::now())
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
    if let Some(mut rec) = read_at(path) {
        if rec.session_id == expected_id {
            rec.backed_up = true;
            if let Ok(bytes) = serde_json::to_vec_pretty(&rec) {
                let _ = std::fs::write(path, bytes);
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
pub fn record_suspended_secs(game_name: &str, total_secs: i64) {
    record_suspended_secs_at(&crate::paths::active_session_file(), game_name, total_secs);
}

fn record_suspended_secs_at(path: &Path, game_name: &str, total_secs: i64) {
    if let Some(mut rec) = read_at(path) {
        if rec.game == game_name {
            rec.suspended_secs = total_secs;
            if let Ok(bytes) = serde_json::to_vec_pretty(&rec) {
                let _ = std::fs::write(path, bytes);
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
    if let Some(rec) = read_at(path) {
        if rec.session_id == expected_id {
            let _ = std::fs::remove_file(path);
        }
    }
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
        let expected =
            steam_shortcuts_util::app_id_generator::calculate_app_id(&quoted, "Hades");
        assert_eq!(compute_steam_appid("/home/u/spool-launcher.sh", "Hades"), expected);
    }
}

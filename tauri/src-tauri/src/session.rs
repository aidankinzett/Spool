//! Active-session record for SteamOS Game-Mode launches.
//!
//! Attached `--run` mode writes this at launch and flips `backed_up = true`
//! once a backup completes (Spool's own post-session backup, or a headless
//! `spool --backup`). A future Decky plugin reads it on the game-stop event:
//! if `backed_up` is still false, Steam force-killed Spool before it backed
//! up, so the plugin spawns `spool --backup` as a fallback.

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

fn mark_backed_up_at(path: &Path) {
    if let Some(mut rec) = read_at(path) {
        rec.backed_up = true;
        if let Ok(bytes) = serde_json::to_vec_pretty(&rec) {
            let _ = std::fs::write(path, bytes);
        }
    }
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

/// Mark the current session's backup as done. No-op when no record exists.
pub fn mark_backed_up() {
    mark_backed_up_at(&crate::paths::active_session_file());
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

        mark_backed_up_at(&path);
        assert!(read_at(&path).unwrap().backed_up);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mark_when_absent_is_noop() {
        let path = std::env::temp_dir().join("spool-session-absent-xyz.json");
        std::fs::remove_file(&path).ok();
        mark_backed_up_at(&path); // must not panic
        assert!(read_at(&path).is_none());
    }

    #[test]
    fn appid_matches_steam_shortcut_formula() {
        let quoted = format!("\"{}\"", "/home/u/spool-launcher.sh");
        let expected =
            steam_shortcuts_util::app_id_generator::calculate_app_id(&quoted, "Hades");
        assert_eq!(compute_steam_appid("/home/u/spool-launcher.sh", "Hades"), expected);
    }
}

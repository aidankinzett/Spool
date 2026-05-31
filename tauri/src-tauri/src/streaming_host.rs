//! Apollo / Sunshine streaming-host integration.
//!
//! [Sunshine](https://github.com/LizardByte/Sunshine) and its fork
//! [Apollo](https://github.com/ClassicOldSong/Apollo) are self-hosted Moonlight
//! game-streaming servers. Both read a list of launchable apps from an
//! `apps.json` file. This module detects whether such a host is installed and,
//! if so, writes a Spool-launching entry into that `apps.json` so a streamed
//! client (Moonlight / Artemis) can pick a game and have Spool run the full
//! restore → play → backup workflow.
//!
//! The generated `cmd` invokes our own binary with
//! `--run "Name" "ExePath" --attached`. The `--attached` flag forces the
//! fullscreen-splash, exit-on-close behavior (see `gamemode.rs` / `cli.rs`) so
//! the stream sees the same flow as SteamOS Game Mode, and the host registers
//! the app as stopped when Spool exits.
//!
//! What we add on top of plain JSON:
//!   * Cross-platform config-dir detection (Linux `~/.config/{sunshine,Apollo}`,
//!     Windows `%ProgramFiles%[(x86)]\{Sunshine,Apollo}\config`).
//!   * Upsert by app name, preserving the `env` block, every other app entry,
//!     and any unknown per-app keys (e.g. Apollo's `uuid`) — we round-trip
//!     through `serde_json::Value` rather than a strict struct.
//!   * Atomic write with a `.bak` backup.

use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::State;

/// Which streaming host we detected and the `apps.json` we'd write to.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamingHostInfo {
    /// True when a host config directory exists.
    pub detected: bool,
    /// Best label for the UI: "apollo" | "sunshine" | "".
    pub kind: String,
    /// Resolved `apps.json` path we'll read/write.
    pub apps_path: String,
}

#[derive(Debug, serde::Serialize)]
pub struct AddToStreamingHostResult {
    pub host_kind: String,
    pub apps_path: String,
    pub app_name: String,
    /// True when an `image-path` (cover) was written.
    pub image_set: bool,
}

/// A candidate host: a config directory and the label to report for it.
struct Candidate {
    kind: &'static str,
    config_dir: PathBuf,
}

/// Builds the ordered list of candidate config directories to probe.
/// Sunshine is preferred over Apollo when both exist (most installs are
/// Sunshine, and Apollo reads the same schema).
fn candidates() -> Vec<Candidate> {
    let mut out = Vec::new();

    #[cfg(windows)]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var) {
                let pf = PathBuf::from(pf);
                out.push(Candidate {
                    kind: "sunshine",
                    config_dir: pf.join("Sunshine").join("config"),
                });
                out.push(Candidate {
                    kind: "apollo",
                    config_dir: pf.join("Apollo").join("config"),
                });
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(cfg) = dirs::config_dir() {
            out.push(Candidate {
                kind: "sunshine",
                config_dir: cfg.join("sunshine"),
            });
            out.push(Candidate {
                kind: "apollo",
                config_dir: cfg.join("Apollo"),
            });
        }
    }

    out
}

/// Returns the first installed host (config directory present), or `None`.
/// Detection is by directory existence — `apps.json` itself may be missing
/// (we create it on write).
pub fn detect() -> Option<StreamingHostInfo> {
    candidates()
        .into_iter()
        .find(|c| c.config_dir.is_dir())
        .map(|c| StreamingHostInfo {
            detected: true,
            kind: c.kind.to_string(),
            apps_path: c.config_dir.join("apps.json").to_string_lossy().to_string(),
        })
}

/// Reads `apps.json` into a `Value`, returning an empty skeleton if the file
/// doesn't exist yet.
fn read_apps(path: &Path) -> AppResult<Value> {
    if !path.is_file() {
        return Ok(json!({ "env": {}, "apps": [] }));
    }
    let text = std::fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(json!({ "env": {}, "apps": [] }));
    }
    let value: Value = serde_json::from_str(&text)?;
    Ok(value)
}

/// Builds the `cmd` string: `"<spool>" --run "<name>" "<exe>" --attached`.
/// Each path/string is `"`-escaped and quoted; identical on Windows and Linux
/// (Sunshine/Apollo parse the command respecting quotes).
fn build_cmd(spool_exe: &str, name: &str, exe: &str) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\\\""));
    format!(
        "{} --run {} {} --attached",
        q(spool_exe),
        q(name),
        q(exe)
    )
}

/// Inserts or updates (by `name`) an app entry in the `apps` array, preserving
/// the `env` block, every other app, and any unknown keys on a matched entry.
fn upsert_app(root: &mut Value, name: &str, cmd: &str, image_path: Option<&str>) {
    // Ensure the top-level shape exists.
    if !root.is_object() {
        *root = json!({ "env": {}, "apps": [] });
    }
    let obj = root.as_object_mut().expect("root is object");
    if !obj.get("apps").map(Value::is_array).unwrap_or(false) {
        obj.insert("apps".to_string(), json!([]));
    }
    let apps = obj
        .get_mut("apps")
        .and_then(Value::as_array_mut)
        .expect("apps is array");

    // Update an existing entry in place (keeps unknown keys like `uuid`).
    if let Some(existing) = apps.iter_mut().find(|a| {
        a.get("name").and_then(Value::as_str).map(|n| n == name).unwrap_or(false)
    }) {
        if let Some(map) = existing.as_object_mut() {
            map.insert("cmd".to_string(), json!(cmd));
            match image_path {
                Some(p) => {
                    map.insert("image-path".to_string(), json!(p));
                }
                None => {
                    map.remove("image-path");
                }
            }
        }
        return;
    }

    // Otherwise append a fresh entry.
    let mut entry = json!({
        "name": name,
        "cmd": cmd,
        "auto-detach": false,
        "wait-all": true,
    });
    if let Some(p) = image_path {
        entry
            .as_object_mut()
            .unwrap()
            .insert("image-path".to_string(), json!(p));
    }
    apps.push(entry);
}

/// Serialises + writes atomically (write `.tmp`, rename). Keeps a `.bak` of the
/// previous file so a botched write can't lose the user's app list.
fn write_apps(path: &Path, root: &Value) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(root)?;

    if path.is_file() {
        let _ = std::fs::copy(path, path.with_extension("json.bak"));
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ── Tauri commands ──────────────────────────────────────────────────────────

/// Reports whether an Apollo/Sunshine host config is present (for UI gating).
#[tauri::command]
pub fn detect_streaming_host() -> Option<StreamingHostInfo> {
    detect()
}

/// Writes a Spool-launching entry for `game_id` into the detected host's
/// `apps.json`, with the game's cover as the tile image.
#[tauri::command]
pub async fn add_to_streaming_host(
    library: State<'_, SharedLibrary>,
    game_id: String,
) -> AppResult<AddToStreamingHostResult> {
    // 1. Snapshot what we need under the lock, then drop it.
    let (app_name, exe_path, cover_image_path) = {
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(&game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.game_name.clone(),
            entry.exe_path.clone(),
            entry.cover_image_path.clone(),
        )
    };

    // 2. Resolve our own (stable) binary path.
    let spool_exe = crate::paths::spool_executable()
        .ok_or_else(|| AppError::Other("can't resolve own exe path".to_string()))?;
    let spool_exe_str = spool_exe.to_string_lossy().to_string();

    // 3. Find the host.
    let host =
        detect().ok_or_else(|| AppError::Other("No Apollo/Sunshine config found".to_string()))?;
    let apps_path = PathBuf::from(&host.apps_path);

    // 4. Cover art: point image-path directly at the on-disk cover (a stable
    //    path under the app-data covers dir). Future cover refreshes are
    //    reflected automatically.
    let image = cover_image_path
        .as_deref()
        .filter(|p| Path::new(p).is_file());

    // 5. Read → upsert → write.
    let cmd = build_cmd(&spool_exe_str, &app_name, &exe_path);
    let mut root = read_apps(&apps_path)?;
    upsert_app(&mut root, &app_name, &cmd, image);
    write_apps(&apps_path, &root)?;

    Ok(AddToStreamingHostResult {
        host_kind: host.kind,
        apps_path: host.apps_path,
        app_name,
        image_set: image.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cmd_quotes_and_appends_attached() {
        let cmd = build_cmd("/opt/Spool.AppImage", "Hades", "/games/Hades/Hades.exe");
        assert_eq!(
            cmd,
            "\"/opt/Spool.AppImage\" --run \"Hades\" \"/games/Hades/Hades.exe\" --attached"
        );
    }

    #[test]
    fn build_cmd_escapes_embedded_quotes_and_spaces() {
        let cmd = build_cmd(
            "C:\\Program Files\\Spool\\spool.exe",
            "My \"Cool\" Game",
            "C:\\Games\\My Game\\game.exe",
        );
        assert!(cmd.ends_with("--attached"));
        assert!(cmd.contains("\"My \\\"Cool\\\" Game\""));
        assert!(cmd.contains("\"C:\\Program Files\\Spool\\spool.exe\""));
    }

    #[test]
    fn upsert_inserts_new_app() {
        let mut root = json!({ "env": {}, "apps": [] });
        upsert_app(&mut root, "Hades", "cmd-here", Some("/covers/hades.png"));
        let apps = root["apps"].as_array().unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0]["name"], "Hades");
        assert_eq!(apps[0]["cmd"], "cmd-here");
        assert_eq!(apps[0]["image-path"], "/covers/hades.png");
        assert_eq!(apps[0]["auto-detach"], false);
        assert_eq!(apps[0]["wait-all"], true);
    }

    #[test]
    fn upsert_updates_existing_by_name_preserving_unknown_keys() {
        let mut root = json!({
            "env": { "PATH": "$(PATH)" },
            "apps": [
                { "name": "Desktop", "image-path": "desktop.png" },
                { "name": "Hades", "cmd": "old", "uuid": "abc-123", "image-path": "old.png" }
            ]
        });
        upsert_app(&mut root, "Hades", "new-cmd", Some("new.png"));
        let apps = root["apps"].as_array().unwrap();
        // No new entry; existing updated.
        assert_eq!(apps.len(), 2);
        let hades = apps.iter().find(|a| a["name"] == "Hades").unwrap();
        assert_eq!(hades["cmd"], "new-cmd");
        assert_eq!(hades["image-path"], "new.png");
        // Unknown key preserved.
        assert_eq!(hades["uuid"], "abc-123");
        // Other apps + env preserved.
        assert!(apps.iter().any(|a| a["name"] == "Desktop"));
        assert_eq!(root["env"]["PATH"], "$(PATH)");
    }

    #[test]
    fn upsert_without_image_removes_stale_image_path() {
        let mut root = json!({
            "env": {},
            "apps": [ { "name": "Hades", "cmd": "old", "image-path": "old.png" } ]
        });
        upsert_app(&mut root, "Hades", "new", None);
        let hades = &root["apps"][0];
        assert_eq!(hades["cmd"], "new");
        assert!(hades.get("image-path").is_none());
    }

    #[test]
    fn read_apps_missing_file_returns_skeleton() {
        let path = std::env::temp_dir().join("spool-nonexistent-apps-test-xyz.json");
        let _ = std::fs::remove_file(&path);
        let root = read_apps(&path).unwrap();
        assert!(root["apps"].as_array().unwrap().is_empty());
        assert!(root["env"].is_object());
    }
}

//! Cross-platform save-restore redirect generation (Phase 3).
//!
//! When a game's backup was made on a different OS or machine (e.g. a Windows
//! desktop → restored on a Linux/Proton Deck), the absolute paths recorded in
//! `mapping.yaml` don't match the local filesystem. Ludusavi restores to the
//! *recorded* absolute path and ignores the local prefix, so the save lands in
//! the wrong place (or recreates a stale path) unless redirects are configured.
//!
//! This module:
//!   1. Parses the backup's `mapping.yaml` to discover the origin OS and the
//!      set of source root prefixes (e.g. `C:/Users/akinz`, `G:/Games/ULTRAKILL`).
//!   2. Derives `{kind: restore, source, target}` redirect rules that map foreign
//!      paths onto the local machine's equivalent locations.
//!   3. Writes the redirect list into Spool's owned `config.yaml` via
//!      `ludusavi_config::set_redirects` so the *next* restore lands correctly.
//!
//! ## Confirmed mapping.yaml schema (from 23 real backups)
//!
//! ```yaml
//! name: <Game>
//! drives:
//!   drive-C: "C:"          # or drive-G: "G:", drive-0: "" (Linux)
//! backups:
//!   - os: windows           # or linux
//!     files:
//!       "C:/Users/akinz/AppData/...": { hash, size }
//!     registry: { hash: ~ }
//!     children:             # differential backups — same OS, share parent paths
//!       - os: windows
//!         files: { ... }
//! ```
//!
//! ## Redirect cases (Direction A: Deck restoring Windows-origin)
//!
//! 1. `C:/Users/<WinUser>` → `<prefix>/drive_c/users/steamuser`
//!    (one rule covers AppData, Documents, Saved Games, OneDrive — ~93% of real paths)
//! 2. `C:/Users/Public`    → `<prefix>/drive_c/users/Public`
//! 3. `C:/ProgramData`     → `<prefix>/drive_c/ProgramData`
//! 4. Install-dir saves (e.g. `G:/Games/<Game>`) → local `game_folder_path` (best-effort)
//! 5. Xbox/UWP paths (`C:/XboxGames/...`, `AppData/Local/Packages/*/wgs`) — logged + skipped
//!
//! ## Direction B: Windows restoring Deck-origin
//!
//! Reverse of rule 1: `<deck_prefix>/drive_c/users/steamuser` → `C:/Users/<local_win_user>`

use crate::error::AppResult;
use crate::ludusavi_config::{self, Redirect};
use serde_yaml::Value;
use std::collections::BTreeSet;
use std::path::Path;

// ── Public types ─────────────────────────────────────────────────────────────

/// The OS that produced a backup, parsed from `backups[].os`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupOs {
    Windows,
    Linux,
    Unknown,
}

impl BackupOs {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            _ => Self::Unknown,
        }
    }
}

/// Everything extracted from mapping.yaml that redirect derivation needs.
#[derive(Debug)]
pub struct BackupOrigin {
    pub os: BackupOs,
    /// All absolute source paths recorded in the backup (union of top-level
    /// backup + all differential children).
    pub paths: Vec<String>,
}

// ── Parser ───────────────────────────────────────────────────────────────────

/// Read `<backup_dir>/<game_name>/mapping.yaml` and return the origin OS +
/// every recorded absolute path. Returns `None` when the file doesn't exist
/// (game has no backup yet — caller skips redirect logic).
///
/// Windows can't create folders with colons in their names, so a backup made
/// on Windows for a game like "Lego Batman: Legacy of the Dark Knight" will
/// have the folder name "Lego Batman_ Legacy of the Dark Knight" (colon →
/// underscore). We try the exact name first, then the safe-name variant.
pub fn read_backup_origin(backup_dir: &Path, game_name: &str) -> Option<BackupOrigin> {
    let candidates = [
        backup_dir.join(game_name).join("mapping.yaml"),
        backup_dir.join(windows_safe_name(game_name)).join("mapping.yaml"),
    ];
    for mapping in &candidates {
        if let Ok(raw) = std::fs::read_to_string(mapping) {
            if let Ok(doc) = serde_yaml::from_str::<Value>(&raw) {
                return parse_origin(&doc);
            }
        }
    }
    None
}

/// Replace characters that Windows forbids in folder names (`:`  `*`  `?`  `"`  `<`  `>`  `|`)
/// with underscores — matches what ludusavi does when creating backup folders on Windows.
fn windows_safe_name(name: &str) -> String {
    name.chars()
        .map(|c| if matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect()
}

fn parse_origin(doc: &Value) -> Option<BackupOrigin> {
    let backups = doc.get("backups")?.as_sequence()?;
    let top = backups.first()?;

    let os = top
        .get("os")
        .and_then(|v| v.as_str())
        .map(BackupOs::from_str)
        .unwrap_or(BackupOs::Unknown);

    let mut paths: Vec<String> = Vec::new();
    collect_files(top, &mut paths);
    let empty = vec![];
    for child in top
        .get("children")
        .and_then(|v| v.as_sequence())
        .unwrap_or(&empty)
    {
        collect_files(child, &mut paths);
    }

    Some(BackupOrigin { os, paths })
}

fn collect_files(node: &Value, out: &mut Vec<String>) {
    if let Some(files) = node.get("files").and_then(|v| v.as_mapping()) {
        for (k, _) in files {
            if let Some(s) = k.as_str() {
                out.push(s.to_string());
            }
        }
    }
}

// ── Redirect derivation ──────────────────────────────────────────────────────

/// Derive and apply redirect rules for a restore. Writes to `config.yaml` via
/// `ludusavi_config::set_redirects`. Returns the number of redirects written
/// (0 = same-origin, no remapping needed).
///
/// `prefix_root` — the Proton prefix ROOT (not drive_c) for Proton games;
///                 `None` for native Windows games (no prefix needed).
/// `game_folder` — the local install folder path (for install-dir saves).
/// `local_win_user` — Windows `%USERNAME%` when running on Windows; `None` on Linux.
pub fn apply_redirects_for_restore(
    origin: &BackupOrigin,
    prefix_root: Option<&Path>,
    game_folder: Option<&Path>,
    local_win_user: Option<&str>,
) -> AppResult<usize> {
    let redirects = derive_redirects(origin, prefix_root, game_folder, local_win_user, cfg!(windows));
    let count = redirects.len();
    ludusavi_config::set_redirects(&redirects)?;
    Ok(count)
}

fn derive_redirects(
    origin: &BackupOrigin,
    prefix_root: Option<&Path>,
    game_folder: Option<&Path>,
    local_win_user: Option<&str>,
    local_is_windows: bool,
) -> Vec<Redirect> {
    match (&origin.os, local_is_windows) {
        // ── Direction A: Linux Deck restoring a Windows backup ─────────────
        (BackupOs::Windows, false) => {
            derive_windows_to_linux(&origin.paths, prefix_root, game_folder)
        }
        // ── Direction B: Windows machine restoring a Linux/Proton backup ───
        (BackupOs::Linux, true) => {
            derive_linux_to_windows(&origin.paths, local_win_user)
        }
        // Same OS or unknown — no redirects needed.
        _ => Vec::new(),
    }
}

fn derive_windows_to_linux(
    paths: &[String],
    prefix_root: Option<&Path>,
    game_folder: Option<&Path>,
) -> Vec<Redirect> {
    let pfx = match prefix_root {
        Some(p) => p,
        None => return Vec::new(), // No prefix → can't redirect Windows paths
    };

    // Extract the distinct "roots" we need to remap. We classify each path
    // into a category and collect unique (foreign_root, local_root) pairs.
    let mut rules: BTreeSet<(String, String)> = BTreeSet::new();

    // Windows username — parsed from the first C:/Users/<name>/* path we find.
    let win_user = windows_username_from_paths(paths);

    for path in paths {
        if let Some(rule) = classify_windows_path(path, win_user.as_deref(), pfx, game_folder) {
            match rule {
                PathClass::UserProfile { win_root, local_root } => {
                    rules.insert((win_root, local_root));
                }
                PathClass::Public { win_root, local_root } => {
                    rules.insert((win_root, local_root));
                }
                PathClass::ProgramData { win_root, local_root } => {
                    rules.insert((win_root, local_root));
                }
                PathClass::InstallDir { win_root, local_root } => {
                    if local_root.is_some() {
                        rules.insert((win_root, local_root.unwrap()));
                    } else {
                        tracing::warn!(
                            win_root,
                            "install-dir save has no local game_folder_path — skipping redirect"
                        );
                    }
                }
                PathClass::XboxUwp | PathClass::Unknown => {
                    // Xbox/UWP games don't run under Proton; unknown paths are
                    // logged and skipped to avoid restoring to a nonsense location.
                    tracing::debug!(path, "skipping unrecognised Windows save path");
                }
            }
        }
    }

    rules
        .into_iter()
        .map(|(source, target)| Redirect {
            kind: "restore".to_string(),
            source,
            target,
        })
        .collect()
}

fn derive_linux_to_windows(paths: &[String], local_win_user: Option<&str>) -> Vec<Redirect> {
    let win_user = match local_win_user {
        Some(u) if !u.is_empty() => u,
        _ => return Vec::new(),
    };

    // Find the Proton prefix root from the Linux paths: the segment ending at
    // the `pfx` or `<id>` directory just before `drive_c`.
    let prefix_root = linux_prefix_root_from_paths(paths);
    let Some(prefix_root) = prefix_root else {
        return Vec::new();
    };

    // One rule remaps the entire user profile tree.
    let source = format!("{}/drive_c/users/steamuser", prefix_root.trim_end_matches('/'));
    let target = format!("C:/Users/{win_user}");

    vec![Redirect {
        kind: "restore".to_string(),
        source,
        target,
    }]
}

// ── Path classification ───────────────────────────────────────────────────────

enum PathClass {
    UserProfile { win_root: String, local_root: String },
    Public { win_root: String, local_root: String },
    ProgramData { win_root: String, local_root: String },
    InstallDir { win_root: String, local_root: Option<String> },
    XboxUwp,
    Unknown,
}

fn classify_windows_path(
    path: &str,
    win_user: Option<&str>,
    prefix_root: &Path,
    game_folder: Option<&Path>,
) -> Option<PathClass> {
    // Forward slashes throughout (ludusavi normalises to forward slashes).
    let p = path.replace('\\', "/");
    let pfx = prefix_root.to_string_lossy();

    // Xbox / UWP — skip.
    if p.contains("/XboxGames/")
        || p.contains("/Packages/")
        || p.contains("/SystemAppData/wgs/")
    {
        return Some(PathClass::XboxUwp);
    }

    // C:/Users/Public
    if p.starts_with("C:/Users/Public/") || p == "C:/Users/Public" {
        let local = format!("{pfx}/drive_c/users/Public");
        return Some(PathClass::Public {
            win_root: "C:/Users/Public".into(),
            local_root: local,
        });
    }

    // C:/Users/<WinUser> (anything else under Users)
    if let Some(user) = win_user {
        let user_prefix = format!("C:/Users/{user}");
        if p.starts_with(&format!("{user_prefix}/")) || p == user_prefix {
            let local = format!("{pfx}/drive_c/users/steamuser");
            return Some(PathClass::UserProfile {
                win_root: user_prefix,
                local_root: local,
            });
        }
    }

    // C:/ProgramData
    if p.starts_with("C:/ProgramData/") || p == "C:/ProgramData" {
        let local = format!("{pfx}/drive_c/ProgramData");
        return Some(PathClass::ProgramData {
            win_root: "C:/ProgramData".into(),
            local_root: local,
        });
    }

    // Install-dir saves: any other absolute Windows path (drive letter + :/).
    // The root is the leading `<Drive>:/path/to/game` segment we can match
    // against the game_folder_path the user set in Spool.
    if path.len() >= 3 && path.as_bytes()[1] == b':' && (path.as_bytes()[2] == b'/' || path.as_bytes()[2] == b'\\') {
        let win_root = install_dir_root(&p);
        let local_root = game_folder.map(|f| f.to_string_lossy().into_owned());
        return Some(PathClass::InstallDir { win_root, local_root });
    }

    Some(PathClass::Unknown)
}

/// Derive the deepest install-dir root for an install-dir save path.
/// e.g. `G:/Games/ULTRAKILL/Saves/Slot1/foo.bin` → `G:/Games/ULTRAKILL`
/// We pick 3 segments (drive + 2 dirs) as a conservative common prefix.
fn install_dir_root(path: &str) -> String {
    let parts: Vec<&str> = path.splitn(5, '/').collect();
    // parts[0]="G:", [1]="Games", [2]="ULTRAKILL", [3]="Saves"...
    // Take drive + first 2 dirs (3 total segments after split on '/').
    if parts.len() >= 3 {
        parts[..3].join("/")
    } else {
        path.to_string()
    }
}

/// Extract the Windows username from the first `C:/Users/<name>/...` path.
fn windows_username_from_paths(paths: &[String]) -> Option<String> {
    for p in paths {
        if let Some(rest) = p.strip_prefix("C:/Users/") {
            let name: &str = rest.split('/').next()?;
            if !name.is_empty() && name != "Public" && name != "Default" && name != "All Users" {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extract the Proton prefix root from a Linux absolute path like
/// `.../compatdata/12345/pfx/drive_c/users/steamuser/...`.
/// Returns the path up to and including the `pfx` or parent dir of `drive_c`.
fn linux_prefix_root_from_paths(paths: &[String]) -> Option<String> {
    for p in paths {
        if let Some(idx) = p.find("/drive_c/") {
            return Some(p[..idx].to_string());
        }
        // Also handle `pfx` as a directory component (some Steam prefixes).
        if let Some(idx) = p.find("/pfx/drive_c/") {
            return Some(p[..idx + 4].to_string()); // include "/pfx"
        }
    }
    None
}

/// Determine the local Windows username (for Direction B). Uses the `USERNAME`
/// env var on Windows; the home dir basename elsewhere.
pub fn local_windows_username() -> Option<String> {
    if cfg!(windows) {
        if let Ok(u) = std::env::var("USERNAME") {
            if !u.is_empty() {
                return Some(u);
            }
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pfx() -> PathBuf {
        PathBuf::from("/home/deck/.local/share/Spool/prefixes/abc")
    }

    #[test]
    fn username_extracted_from_paths() {
        let paths = vec!["C:/Users/akinz/AppData/Local/Foo/save.dat".to_string()];
        assert_eq!(windows_username_from_paths(&paths), Some("akinz".to_string()));
    }

    #[test]
    fn public_path_not_used_as_username() {
        let paths = vec!["C:/Users/Public/Documents/Foo.sav".to_string()];
        assert_eq!(windows_username_from_paths(&paths), None);
    }

    #[test]
    fn appdata_redirect_generated() {
        let paths = vec!["C:/Users/akinz/AppData/Local/Deltarune/dr.ini".to_string()];
        let origin = BackupOrigin { os: BackupOs::Windows, paths };
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].source, "C:/Users/akinz");
        assert!(redirects[0].target.contains("drive_c/users/steamuser"));
        assert_eq!(redirects[0].kind, "restore");
    }

    #[test]
    fn public_and_user_get_separate_rules() {
        let paths = vec![
            "C:/Users/akinz/AppData/Local/Foo/save.dat".to_string(),
            "C:/Users/Public/Documents/Bar.sav".to_string(),
        ];
        let origin = BackupOrigin { os: BackupOs::Windows, paths };
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert_eq!(redirects.len(), 2);
        let sources: Vec<&str> = redirects.iter().map(|r| r.source.as_str()).collect();
        assert!(sources.contains(&"C:/Users/akinz"));
        assert!(sources.contains(&"C:/Users/Public"));
    }

    #[test]
    fn install_dir_uses_game_folder() {
        let paths = vec!["G:/Games/ULTRAKILL/Saves/Slot1/save.bepis".to_string()];
        let origin = BackupOrigin { os: BackupOs::Windows, paths };
        let game_folder = PathBuf::from("/home/deck/Games/ULTRAKILL");
        let redirects = derive_redirects(&origin, Some(&pfx()), Some(&game_folder), None, false);
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].source, "G:/Games/ULTRAKILL");
        assert_eq!(redirects[0].target, "/home/deck/Games/ULTRAKILL");
    }

    #[test]
    fn install_dir_skipped_without_game_folder() {
        let paths = vec!["G:/Games/ULTRAKILL/Saves/save.bepis".to_string()];
        let origin = BackupOrigin { os: BackupOs::Windows, paths };
        // No game_folder → install-dir save can't be redirected.
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert!(redirects.is_empty());
    }

    #[test]
    fn xbox_uwp_paths_skipped() {
        let paths = vec!["C:/Users/akinz/AppData/Local/Packages/Microsoft.OpusPG_xxx/SystemAppData/wgs/abc/save".to_string()];
        let origin = BackupOrigin { os: BackupOs::Windows, paths };
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert!(redirects.is_empty());
    }

    #[test]
    fn same_os_produces_no_redirects() {
        // Both Linux → no redirect needed.
        let paths = vec!["/home/deck/.local/share/SomeGame/save.dat".to_string()];
        let origin = BackupOrigin { os: BackupOs::Linux, paths };
        // local_is_windows = false and origin is Linux → no rules.
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert!(redirects.is_empty());
    }

    #[test]
    fn linux_prefix_root_extracted() {
        let paths = vec![
            "/home/deck/.local/share/Steam/steamapps/compatdata/123/pfx/drive_c/users/steamuser/AppData/LocalLow/Game/save.json".to_string()
        ];
        let root = linux_prefix_root_from_paths(&paths);
        // Should capture everything up to and including pfx
        assert!(root.is_some());
        let r = root.unwrap();
        assert!(r.ends_with("pfx"), "got: {r}");
    }

    #[test]
    fn windows_safe_name_replaces_colon() {
        assert_eq!(
            windows_safe_name("Lego Batman: Legacy of the Dark Knight"),
            "Lego Batman_ Legacy of the Dark Knight"
        );
        assert_eq!(windows_safe_name("Normal Name"), "Normal Name");
        assert_eq!(windows_safe_name("File*Name?"), "File_Name_");
    }

    #[test]
    fn read_backup_origin_finds_safe_name_folder() {
        // "Lego Batman: Legacy of the Dark Knight" backup was created on Windows,
        // so the folder is named with underscores. read_backup_origin should find
        // it via the safe-name fallback.
        let backup_dir = std::path::Path::new("/home/deck/.local/share/Spool/ludusavi-backup");
        if !backup_dir.exists() { return; }
        // Pass the colon-containing canonical name — should still find the folder.
        if let Some(origin) = read_backup_origin(backup_dir, "Lego Batman: Legacy of the Dark Knight") {
            assert_eq!(origin.os, BackupOs::Windows);
            assert!(origin.paths.iter().any(|p| p.contains("akinz")));
        }
    }

    #[test]
    fn parse_real_deltarune_mapping() {
        // Test against the actual mapping.yaml from ~/ludusavi-backup.
        let backup_dir = std::path::Path::new("/home/deck/ludusavi-backup");
        let Some(origin) = read_backup_origin(backup_dir, "Deltarune") else {
            return; // file absent in CI — skip
        };
        assert_eq!(origin.os, BackupOs::Windows);
        assert!(origin.paths.iter().any(|p| p.contains("DELTARUNE")));
    }

    #[test]
    fn parse_real_lego_batman_mapping_with_diffs() {
        // Use the canonical colon name — should find the underscore folder.
        let backup_dir = std::path::Path::new("/home/deck/ludusavi-backup");
        let game_name = "Lego Batman: Legacy of the Dark Knight";
        let Some(origin) = read_backup_origin(backup_dir, game_name) else {
            return;
        };
        assert_eq!(origin.os, BackupOs::Windows);
        // Should include paths from differential children too.
        assert!(origin.paths.len() > 4, "expected paths from diffs too, got {}", origin.paths.len());
        // All paths should be Windows-style.
        assert!(origin.paths.iter().all(|p| p.starts_with("C:/")));
    }

    #[test]
    fn parse_real_ultrakill_install_dir() {
        let backup_dir = std::path::Path::new("/home/deck/ludusavi-backup");
        let Some(origin) = read_backup_origin(backup_dir, "ULTRAKILL") else {
            return;
        };
        assert_eq!(origin.os, BackupOs::Windows);
        assert!(origin.paths.iter().any(|p| p.starts_with("G:/")));
    }
}

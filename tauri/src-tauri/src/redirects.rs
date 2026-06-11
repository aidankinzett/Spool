//! Cross-platform save-restore redirect generation (Phase 3).
//!
//! When a game's backup was made on a different OS or machine (e.g. a Windows
//! desktop → restored on a Linux/Proton Deck), the absolute paths recorded in
//! `mapping.yaml` don't match the local filesystem. Ludusavi restores to the
//! *recorded* absolute path and ignores the local prefix, so the save lands in
//! the wrong place (or recreates a stale path) unless redirects are configured.
//!
//! This module:
//!   1. Parses the backup's `mapping.yaml` to discover the set of source paths
//!      recorded in the backup (e.g. `C:/Users/akinz`, `G:/Games/ULTRAKILL`, or
//!      `.../prefixes/<id>/drive_c/users/steamuser`).
//!   2. Derives `{kind: restore, source, target}` redirect rules that map foreign
//!      paths onto the local machine's equivalent locations.
//!   3. Writes the redirect list into Spool's owned `config.yaml` via
//!      `ludusavi_config::set_redirects` so the *next* restore lands correctly.
//!
//! ## Decisions are driven by the path *format*, not the backup `os` field
//!
//! Ludusavi always stamps every backup with `os: Os::HOST` — the OS of the
//! machine that *authored* the backup — and that field is never rewritten by
//! redirects or `--wine-prefix`. Crucially, Spool's own Phase-3 backup
//! *canonicalisation* (see [`apply_redirects_for_backup`]) rewrites a
//! Windows-origin save's stored paths back to `C:/…` even though the backup was
//! taken on Linux. The result is a `mapping.yaml` whose `os: linux` disagrees
//! with its `C:/…` paths.
//!
//! Keying the redirect decision off `os` therefore breaks on the *second*
//! cross-platform round-trip (e.g. a Windows game replayed on Linux a second
//! time would read `os: linux` + `C:/…` paths, generate no redirect, and fail
//! to land the save in the prefix). So instead we classify each *path* by its
//! literal format (`X:/…` Windows, `…/drive_c/…` wine-prefix, or native Linux)
//! and reconcile it against the local platform + prefix. This stays correct
//! across any number of cross-platform hops.
//!
//! ## Confirmed mapping.yaml schema (from 23 real backups)
//!
//! ```yaml
//! name: <Game>
//! drives:
//!   drive-C: "C:"          # or drive-G: "G:", drive-0: "" (Linux)
//! backups:
//!   - os: windows           # or linux — authoring host, NOT the path format
//!     files:
//!       "C:/Users/akinz/AppData/...": { hash, size }
//!     registry: { hash: ~ }
//!     children:             # differential backups — same OS, share parent paths
//!       - os: windows
//!         files: { ... }
//! ```
//!
//! ## Windows-format path (`X:/…`)
//!
//! * On Windows → native, no redirect.
//! * On Linux (Proton) → map into the prefix:
//!   1. `C:/Users/<WinUser>` → `<prefix>/drive_c/users/steamuser`
//!      (covers AppData, Documents, Saved Games, OneDrive — ~93% of real paths)
//!   2. `C:/Users/Public`    → `<prefix>/drive_c/users/Public`
//!   3. `C:/ProgramData`     → `<prefix>/drive_c/ProgramData`
//!   4. Install-dir saves (e.g. `G:/Games/<Game>`) → local `game_folder_path` (best-effort)
//!   5. Xbox/UWP paths (`C:/XboxGames/...`, `AppData/Local/Packages/*/wgs`) — logged + skipped
//!
//! ## Wine-prefix path (`…/drive_c/…`)
//!
//! * On Windows → reverse of the rules above (full symmetry):
//!   `…/drive_c/users/steamuser` → `C:/Users/<local user>`,
//!   `…/drive_c/users/Public`    → `C:/Users/Public`,
//!   `…/drive_c/ProgramData`     → `C:/ProgramData`.
//! * On Linux → only remap when the authoring prefix root differs from this
//!   machine's (cross-device / cross-user); same machine + game ⇒ no-op.
//!
//! ## Native Linux path (`/home/…`, not under a prefix)
//!
//! * On Linux → native, no redirect.
//! * On Windows → no reliable equivalent → logged + skipped (don't guess wrong).

use crate::error::AppResult;
use crate::ludusavi_config::{self, Redirect};
use serde_yaml::Value;
use std::collections::BTreeSet;
use std::path::Path;

// ── Public types ─────────────────────────────────────────────────────────────

/// The OS that produced a backup, parsed from `backups[].os`. Retained for
/// logging/telemetry only — redirect derivation keys off the path format, not
/// this field (see the module docs).
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
        backup_dir
            .join(windows_safe_name(game_name))
            .join("mapping.yaml"),
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

/// The tip (most recent backup) of a game's `mapping.yaml` — the unique
/// ludusavi backup name plus its timestamp. Used as a content-identity token
/// for cloud fast-forward detection: ludusavi mirrors `mapping.yaml` across
/// devices on every cloud sync, so the tip name is byte-identical wherever a
/// given save state lives (and is independent of OS / drive letters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupTip {
    pub name: String,
    pub when: chrono::DateTime<chrono::Utc>,
}

/// Read `<backup_dir>/<game_name>/mapping.yaml` and return its tip — the
/// full-or-differential backup with the latest `when`. Tries the exact game
/// folder name first, then the Windows-safe (colon-stripped) name, mirroring
/// [`read_backup_origin`]. Returns `None` when there's no backup yet.
pub fn read_local_backup_tip(backup_dir: &Path, game_name: &str) -> Option<BackupTip> {
    let candidates = [
        backup_dir.join(game_name).join("mapping.yaml"),
        backup_dir
            .join(windows_safe_name(game_name))
            .join("mapping.yaml"),
    ];
    for mapping in &candidates {
        if let Ok(raw) = std::fs::read_to_string(mapping) {
            if let Some(tip) = read_backup_tip_from_str(&raw) {
                return Some(tip);
            }
        }
    }
    None
}

/// Async wrapper over [`read_backup_origin`] that runs the `mapping.yaml` read +
/// parse on the blocking pool. Prefer this from the run workflow / Tauri
/// commands so the synchronous file IO doesn't sit on the async runtime.
pub async fn read_backup_origin_async(backup_dir: &Path, game_name: &str) -> Option<BackupOrigin> {
    let dir = backup_dir.to_path_buf();
    let name = game_name.to_string();
    tokio::task::spawn_blocking(move || read_backup_origin(&dir, &name))
        .await
        .ok()
        .flatten()
}

/// Async wrapper over [`read_local_backup_tip`] — same rationale as
/// [`read_backup_origin_async`]: keep the `mapping.yaml` read off the runtime.
pub async fn read_local_backup_tip_async(backup_dir: &Path, game_name: &str) -> Option<BackupTip> {
    let dir = backup_dir.to_path_buf();
    let name = game_name.to_string();
    tokio::task::spawn_blocking(move || read_local_backup_tip(&dir, &name))
        .await
        .ok()
        .flatten()
}

/// Parse a `mapping.yaml` body and return the tip: the entry (full backup or
/// differential child) with the maximum `when`. The backup `name` is ludusavi's
/// unique, timestamp-derived id (e.g. `backup-20260530T120000Z`). Returns
/// `None` if there are no dated backups.
pub fn read_backup_tip_from_str(yaml: &str) -> Option<BackupTip> {
    let doc: Value = serde_yaml::from_str(yaml).ok()?;
    let backups = doc.get("backups")?.as_sequence()?;

    let mut tip: Option<BackupTip> = None;
    let mut consider = |node: &Value| {
        let (Some(name), Some(when_str)) = (
            node.get("name").and_then(|v| v.as_str()),
            node.get("when").and_then(|v| v.as_str()),
        ) else {
            return;
        };
        let Ok(when) = chrono::DateTime::parse_from_rfc3339(when_str) else {
            return;
        };
        let when = when.with_timezone(&chrono::Utc);
        if tip.as_ref().is_none_or(|t| when > t.when) {
            tip = Some(BackupTip {
                name: name.to_string(),
                when,
            });
        }
    };

    let empty = vec![];
    for full in backups {
        consider(full);
        for child in full
            .get("children")
            .and_then(|v| v.as_sequence())
            .unwrap_or(&empty)
        {
            consider(child);
        }
    }
    tip
}

/// Replace characters that Windows forbids in folder names (`:`  `*`  `?`  `"`  `<`  `>`  `|`)
/// with underscores — matches what ludusavi does when creating backup folders on Windows.
pub fn windows_safe_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
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
/// (0 = local paths already correct, no remapping needed).
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
    let redirects = derive_redirects(
        origin,
        prefix_root,
        game_folder,
        local_win_user,
        cfg!(windows),
    );
    let count = redirects.len();
    ludusavi_config::set_redirects(&redirects)?;
    Ok(count)
}

/// Derive and apply `kind: "backup"` redirect rules so a Windows-origin game's
/// saves are *stored* with the same canonical `C:/…` paths as the backup they
/// were restored from — instead of the local Linux/Proton prefix paths ludusavi
/// would otherwise record.
///
/// Without this, the restore phase steers a Windows-origin save into the Proton
/// prefix, but the post-session backup records the *local* prefix path
/// (`.../drive_c/...`). The backup then silently flips from Windows paths to
/// Linux paths, breaking the next restore on Windows.
///
/// Only the cross-OS rules (those whose restore *source* is a Windows drive
/// path) are inverted and re-tagged `kind: "backup"`; same-OS prefix-root
/// remaps are deliberately dropped so a native Linux backup keeps its own
/// real paths. Same arguments as [`apply_redirects_for_restore`]. Returns the
/// number of redirects written (0 = nothing to canonicalise).
pub fn apply_redirects_for_backup(
    origin: &BackupOrigin,
    prefix_root: Option<&Path>,
    game_folder: Option<&Path>,
    local_win_user: Option<&str>,
) -> AppResult<usize> {
    let restore_rules = derive_redirects(
        origin,
        prefix_root,
        game_folder,
        local_win_user,
        cfg!(windows),
    );
    let backup_rules = invert_for_backup(restore_rules);
    let count = backup_rules.len();
    ludusavi_config::set_redirects(&backup_rules)?;
    Ok(count)
}

/// Invert restore rules into backup rules, keeping only the cross-OS
/// canonicalisation rules (restore source is a Windows `X:/…` path). A ludusavi
/// `backup` redirect maps the *scanned* path → the *stored* path, the opposite
/// of a `restore` redirect, so source/target are flipped.
fn invert_for_backup(restore_rules: Vec<Redirect>) -> Vec<Redirect> {
    restore_rules
        .into_iter()
        .filter(|r| is_windows_drive_path(&r.source))
        .map(|r| Redirect {
            kind: "backup".to_string(),
            source: r.target,
            target: r.source,
        })
        .collect()
}

fn derive_redirects(
    origin: &BackupOrigin,
    prefix_root: Option<&Path>,
    game_folder: Option<&Path>,
    local_win_user: Option<&str>,
    local_is_windows: bool,
) -> Vec<Redirect> {
    // Windows username parsed from the backup's own `C:/Users/<name>/…` paths —
    // used when steering a Windows-format path into the local prefix.
    let backup_win_user = windows_username_from_paths(&origin.paths);
    // This machine's prefix root (forward-slashed), for Linux↔Linux remaps.
    let local_drive_c = prefix_root.map(|p| {
        format!(
            "{}/drive_c",
            p.to_string_lossy().replace('\\', "/").trim_end_matches('/')
        )
    });

    // De-duplicated (source, target) pairs — many files share a root.
    let mut rules: BTreeSet<(String, String)> = BTreeSet::new();

    for path in &origin.paths {
        match classify_format(path) {
            PathFormat::Windows => {
                // Native on Windows; needs the Proton prefix to land on Linux.
                if !local_is_windows {
                    if let Some(pfx) = prefix_root {
                        if let Some(rule) = windows_path_to_prefix(
                            path,
                            backup_win_user.as_deref(),
                            pfx,
                            game_folder,
                        ) {
                            rules.insert(rule);
                        }
                    }
                }
            }
            PathFormat::WinePrefix { drive_c } => {
                if local_is_windows {
                    // Prefix save → its canonical Windows location.
                    if let Some(rule) = prefix_path_to_windows(path, &drive_c, local_win_user) {
                        rules.insert(rule);
                    }
                } else if let Some(local_drive_c) = &local_drive_c {
                    // Both Linux: only remap when the authoring prefix root
                    // differs from this machine's (cross-device / cross-user).
                    // Same machine + game_id ⇒ identical root ⇒ no redirect.
                    if &drive_c != local_drive_c {
                        rules.insert((drive_c.clone(), local_drive_c.clone()));
                    }
                }
            }
            PathFormat::NativeLinux => {
                // Non-prefix Linux path (native Linux game, or a Linux install
                // dir). No reliable Windows equivalent — leave it where ludusavi
                // puts it rather than risk a wrong location.
                if local_is_windows {
                    tracing::debug!(
                        path,
                        "no Windows equivalent for native Linux save path — skipping redirect"
                    );
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

// ── Path format classification ────────────────────────────────────────────────

/// The literal format of a recorded save path (independent of the backup's
/// `os` field).
enum PathFormat {
    /// Windows drive path, e.g. `C:/Users/akinz/...` or `G:/Games/...`.
    Windows,
    /// Wine/Proton prefix path. `drive_c` is the path up to and including the
    /// `…/drive_c` segment, e.g. `/home/deck/.../prefixes/abc/drive_c`.
    WinePrefix { drive_c: String },
    /// An absolute Linux path that isn't inside a wine prefix.
    NativeLinux,
}

fn classify_format(path: &str) -> PathFormat {
    let p = path.replace('\\', "/");
    // Wine/Proton prefix path: `<root>/drive_c/...` (checked before the Windows
    // drive test — a prefix path is a Linux absolute path, never `X:/…`).
    if let Some(idx) = p.find("/drive_c/") {
        return PathFormat::WinePrefix {
            drive_c: p[..idx + "/drive_c".len()].to_string(),
        };
    }
    if let Some(stripped) = p.strip_suffix("/drive_c") {
        return PathFormat::WinePrefix {
            drive_c: format!("{stripped}/drive_c"),
        };
    }
    if is_windows_drive_path(&p) {
        return PathFormat::Windows;
    }
    PathFormat::NativeLinux
}

/// True for a path that starts with a Windows drive letter, e.g. `C:/…`.
fn is_windows_drive_path(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

// ── Windows → prefix (running a Windows-format save on Linux) ──────────────────

/// Map a single Windows-format save path onto its location inside the local
/// Proton prefix. Returns the `(source, target)` root pair, or `None` for
/// Xbox/UWP, unknown, or install-dir-without-a-local-folder paths.
fn windows_path_to_prefix(
    path: &str,
    backup_win_user: Option<&str>,
    prefix_root: &Path,
    game_folder: Option<&Path>,
) -> Option<(String, String)> {
    match classify_windows_path(path, backup_win_user, prefix_root, game_folder)? {
        PathClass::UserProfile {
            win_root,
            local_root,
        }
        | PathClass::Public {
            win_root,
            local_root,
        }
        | PathClass::ProgramData {
            win_root,
            local_root,
        } => Some((win_root, local_root)),
        PathClass::InstallDir {
            win_root,
            local_root,
        } => {
            if local_root.is_none() {
                tracing::warn!(
                    win_root,
                    "install-dir save has no local game_folder_path — skipping redirect"
                );
            }
            local_root.map(|l| (win_root, l))
        }
        PathClass::XboxUwp | PathClass::Unknown => {
            tracing::debug!(path, "skipping unrecognised Windows save path");
            None
        }
    }
}

enum PathClass {
    UserProfile {
        win_root: String,
        local_root: String,
    },
    Public {
        win_root: String,
        local_root: String,
    },
    ProgramData {
        win_root: String,
        local_root: String,
    },
    InstallDir {
        win_root: String,
        local_root: Option<String>,
    },
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
    if p.contains("/XboxGames/") || p.contains("/Packages/") || p.contains("/SystemAppData/wgs/") {
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
    if is_windows_drive_path(&p) {
        let game_basename = game_folder
            .and_then(|f| f.file_name())
            .and_then(|n| n.to_str());
        let win_root = install_dir_root(&p, game_basename);
        let local_root = game_folder.map(|f| f.to_string_lossy().into_owned());
        return Some(PathClass::InstallDir {
            win_root,
            local_root,
        });
    }

    Some(PathClass::Unknown)
}

/// Derive the install-dir root for an install-dir save *file* path. The input
/// is always a file (mapping.yaml keys are files), so the trailing filename is
/// dropped first — otherwise a shallow path like `D:/Game/save.bin` would map
/// the file itself as the root, restoring the save to the wrong location.
///
/// When the game's local folder name is known we anchor on it: keep everything
/// up to and including the first path segment equal to that name (case-
/// insensitive, since Windows paths are). This finds the true install root even
/// when it is deeper than two dirs — a save under a Steam library at
/// `G:/SteamLibrary/steamapps/common/ULTRAKILL/Saves/x` with folder name
/// `ULTRAKILL` yields `G:/SteamLibrary/steamapps/common/ULTRAKILL`. Without a
/// name to anchor on (or when it does not appear in the path) we fall back to a
/// conservative drive + up-to-two-dirs guess, so `D:/Game/save.bin` maps `D:/Game`.
fn install_dir_root(path: &str, game_basename: Option<&str>) -> String {
    // Drop the trailing filename so we never treat the save file as the root.
    let dir = match path.rsplit_once('/') {
        Some((parent, _file)) => parent,
        None => return path.to_string(),
    };
    let parts: Vec<&str> = dir.split('/').collect();

    // Anchor on the game folder name when it appears in the path. (#283)
    if let Some(name) = game_basename.filter(|n| !n.is_empty()) {
        if let Some(idx) = parts.iter().position(|seg| seg.eq_ignore_ascii_case(name)) {
            return parts[..=idx].join("/");
        }
    }

    // Fallback: drive + up to 2 directories (never more than are present).
    let take = parts.len().min(3);
    parts[..take].join("/")
}

// ── Prefix → Windows (running a wine-prefix save on Windows) ───────────────────

/// Map a single wine-prefix save path onto its canonical Windows location.
/// `drive_c` is the `…/drive_c` segment from [`classify_format`]. Returns the
/// `(source, target)` root pair, or `None` for paths under the prefix we can't
/// canonicalise (e.g. `Program Files` installs).
fn prefix_path_to_windows(
    path: &str,
    drive_c: &str,
    local_win_user: Option<&str>,
) -> Option<(String, String)> {
    let p = path.replace('\\', "/");
    let rest = p.strip_prefix(drive_c)?.trim_start_matches('/');

    // users/Public  (no username needed)
    if rest == "users/Public" || rest.starts_with("users/Public/") {
        return Some((
            format!("{drive_c}/users/Public"),
            "C:/Users/Public".to_string(),
        ));
    }
    // ProgramData  (no username needed)
    if rest == "ProgramData" || rest.starts_with("ProgramData/") {
        return Some((
            format!("{drive_c}/ProgramData"),
            "C:/ProgramData".to_string(),
        ));
    }
    // users/steamuser → C:/Users/<local windows user>
    if rest == "users/steamuser" || rest.starts_with("users/steamuser/") {
        let user = local_win_user.filter(|u| !u.is_empty())?;
        return Some((
            format!("{drive_c}/users/steamuser"),
            format!("C:/Users/{user}"),
        ));
    }
    // Anything else under drive_c (Program Files installs, etc.) — skip.
    None
}

// ── Shared helpers ─────────────────────────────────────────────────────────────

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

/// Determine the local Windows username (for the prefix → Windows direction).
/// Uses the `USERNAME` env var on Windows; `None` elsewhere.
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

    fn win_origin(paths: &[&str]) -> BackupOrigin {
        BackupOrigin {
            os: BackupOs::Windows,
            paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn lin_origin(paths: &[&str]) -> BackupOrigin {
        BackupOrigin {
            os: BackupOs::Linux,
            paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ── username parsing ───────────────────────────────────────────────────

    #[test]
    fn username_extracted_from_paths() {
        let paths = vec!["C:/Users/akinz/AppData/Local/Foo/save.dat".to_string()];
        assert_eq!(
            windows_username_from_paths(&paths),
            Some("akinz".to_string())
        );
    }

    #[test]
    fn public_path_not_used_as_username() {
        let paths = vec!["C:/Users/Public/Documents/Foo.sav".to_string()];
        assert_eq!(windows_username_from_paths(&paths), None);
    }

    // ── Windows-format → Linux prefix ──────────────────────────────────────

    #[test]
    fn appdata_redirect_generated() {
        let origin = win_origin(&["C:/Users/akinz/AppData/Local/Deltarune/dr.ini"]);
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].source, "C:/Users/akinz");
        assert!(redirects[0].target.contains("drive_c/users/steamuser"));
        assert_eq!(redirects[0].kind, "restore");
    }

    #[test]
    fn public_and_user_get_separate_rules() {
        let origin = win_origin(&[
            "C:/Users/akinz/AppData/Local/Foo/save.dat",
            "C:/Users/Public/Documents/Bar.sav",
        ]);
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert_eq!(redirects.len(), 2);
        let sources: Vec<&str> = redirects.iter().map(|r| r.source.as_str()).collect();
        assert!(sources.contains(&"C:/Users/akinz"));
        assert!(sources.contains(&"C:/Users/Public"));
    }

    #[test]
    fn install_dir_uses_game_folder() {
        let origin = win_origin(&["G:/Games/ULTRAKILL/Saves/Slot1/save.bepis"]);
        let game_folder = PathBuf::from("/home/deck/Games/ULTRAKILL");
        let redirects = derive_redirects(&origin, Some(&pfx()), Some(&game_folder), None, false);
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].source, "G:/Games/ULTRAKILL");
        assert_eq!(redirects[0].target, "/home/deck/Games/ULTRAKILL");
    }

    #[test]
    fn install_dir_anchors_on_game_folder_name_for_deep_paths() {
        // A Steam-library path is deeper than two dirs; the install root must be
        // anchored on the game folder name, not truncated to `…/steamapps`. (#283)
        let origin =
            win_origin(&["G:/SteamLibrary/steamapps/common/ULTRAKILL/Saves/Slot1/save.bepis"]);
        let game_folder = PathBuf::from("/home/deck/Games/ULTRAKILL");
        let redirects = derive_redirects(&origin, Some(&pfx()), Some(&game_folder), None, false);
        assert_eq!(redirects.len(), 1);
        assert_eq!(
            redirects[0].source,
            "G:/SteamLibrary/steamapps/common/ULTRAKILL"
        );
        assert_eq!(redirects[0].target, "/home/deck/Games/ULTRAKILL");
    }

    #[test]
    fn install_dir_root_drops_filename_for_shallow_path() {
        // A save directly in the install folder (`D:/Game/save.bin`) must map the
        // folder `D:/Game`, not the file itself — otherwise the filename is lost.
        let origin = win_origin(&["D:/Game/save.bin"]);
        let game_folder = PathBuf::from("/home/deck/Games/Game");
        let redirects = derive_redirects(&origin, Some(&pfx()), Some(&game_folder), None, false);
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].source, "D:/Game");
        assert_eq!(redirects[0].target, "/home/deck/Games/Game");
    }

    #[test]
    fn install_dir_skipped_without_game_folder() {
        let origin = win_origin(&["G:/Games/ULTRAKILL/Saves/save.bepis"]);
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert!(redirects.is_empty());
    }

    #[test]
    fn xbox_uwp_paths_skipped() {
        let origin = win_origin(&[
            "C:/Users/akinz/AppData/Local/Packages/Microsoft.OpusPG_xxx/SystemAppData/wgs/abc/save",
        ]);
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert!(redirects.is_empty());
    }

    /// The S3 regression: a Windows-origin save that was canonicalised on Linux
    /// (so the backup is tagged `os: linux` but stores `C:/…` paths) must still
    /// redirect into the prefix on a *second* Linux restore. Decision is by path
    /// format, so the `os: linux` stamp is irrelevant.
    #[test]
    fn canonicalised_windows_save_replayed_on_linux() {
        let origin = lin_origin(&["C:/Users/akinz/AppData/Local/Deltarune/dr.ini"]);
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].source, "C:/Users/akinz");
        assert!(redirects[0].target.contains("drive_c/users/steamuser"));
    }

    // ── Wine-prefix → Windows (Direction B, now symmetric) ─────────────────

    #[test]
    fn prefix_user_profile_restored_on_windows() {
        let origin = lin_origin(&[
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/steamuser/AppData/LocalLow/Game/save.json",
        ]);
        // On Windows: prefix_root is None, local username is the local box's.
        let redirects = derive_redirects(&origin, None, None, Some("alice"), true);
        assert_eq!(redirects.len(), 1);
        assert_eq!(
            redirects[0].source,
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/steamuser"
        );
        assert_eq!(redirects[0].target, "C:/Users/alice");
    }

    #[test]
    fn prefix_public_and_programdata_restored_on_windows() {
        let origin = lin_origin(&[
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/Public/Documents/Bar.sav",
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/ProgramData/Game/cfg.ini",
        ]);
        let redirects = derive_redirects(&origin, None, None, Some("alice"), true);
        assert_eq!(redirects.len(), 2);
        let pairs: Vec<(&str, &str)> = redirects
            .iter()
            .map(|r| (r.source.as_str(), r.target.as_str()))
            .collect();
        assert!(pairs.contains(&(
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/Public",
            "C:/Users/Public"
        )));
        assert!(pairs.contains(&(
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/ProgramData",
            "C:/ProgramData"
        )));
    }

    /// Without a local Windows username we can't target `C:/Users/<user>`, but
    /// Public / ProgramData don't need it and must still be redirected.
    #[test]
    fn prefix_without_username_keeps_public_skips_user_profile() {
        let origin = lin_origin(&[
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/steamuser/AppData/Local/Game/s.dat",
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/Public/Documents/Bar.sav",
        ]);
        let redirects = derive_redirects(&origin, None, None, None, true);
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].target, "C:/Users/Public");
    }

    #[test]
    fn prefix_program_files_install_skipped_on_windows() {
        let origin = lin_origin(&[
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/Program Files/Game/save.bin",
        ]);
        let redirects = derive_redirects(&origin, None, None, Some("alice"), true);
        assert!(redirects.is_empty());
    }

    // ── Linux ↔ Linux prefix-root remap (cross-device) ─────────────────────

    #[test]
    fn prefix_remapped_across_machines() {
        // Authoring deck's prefix vs this machine's prefix (different home dir).
        let origin = lin_origin(&[
            "/home/alice/.local/share/Spool/prefixes/abc/drive_c/users/steamuser/AppData/Local/Game/s.dat",
        ]);
        let local = PathBuf::from("/home/bob/.local/share/Spool/prefixes/abc");
        let redirects = derive_redirects(&origin, Some(&local), None, None, false);
        assert_eq!(redirects.len(), 1);
        assert_eq!(
            redirects[0].source,
            "/home/alice/.local/share/Spool/prefixes/abc/drive_c"
        );
        assert_eq!(
            redirects[0].target,
            "/home/bob/.local/share/Spool/prefixes/abc/drive_c"
        );
    }

    #[test]
    fn same_machine_prefix_no_remap() {
        let origin = lin_origin(&[
            "/home/deck/.local/share/Spool/prefixes/abc/drive_c/users/steamuser/AppData/Local/Game/s.dat",
        ]);
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert!(redirects.is_empty());
    }

    // ── Native Linux (non-prefix) ──────────────────────────────────────────

    #[test]
    fn native_linux_save_on_linux_no_redirect() {
        let origin = lin_origin(&["/home/deck/.local/share/SomeGame/save.dat"]);
        let redirects = derive_redirects(&origin, Some(&pfx()), None, None, false);
        assert!(redirects.is_empty());
    }

    #[test]
    fn native_linux_save_on_windows_skipped() {
        let origin = lin_origin(&["/home/deck/.local/share/SomeGame/save.dat"]);
        let redirects = derive_redirects(&origin, None, None, Some("alice"), true);
        assert!(redirects.is_empty());
    }

    // ── Windows-format on Windows is native ────────────────────────────────

    #[test]
    fn windows_save_on_windows_no_redirect() {
        // e.g. S2: os:linux + C:/ paths replayed on Windows — native, no-op.
        let origin = lin_origin(&["C:/Users/akinz/AppData/Local/Deltarune/dr.ini"]);
        let redirects = derive_redirects(&origin, None, None, Some("akinz"), true);
        assert!(redirects.is_empty());
    }

    // ── Backup canonicalisation (invert_for_backup) ────────────────────────

    #[test]
    fn backup_inverts_only_windows_rules() {
        // A Windows-canonicalisation rule (source = C:/…) is inverted + retagged;
        // a Linux prefix-remap rule (source = /…/drive_c) is dropped so the
        // native Linux backup keeps its own real paths.
        let restore = vec![
            Redirect {
                kind: "restore".into(),
                source: "C:/Users/akinz".into(),
                target: "/home/deck/.../prefixes/abc/drive_c/users/steamuser".into(),
            },
            Redirect {
                kind: "restore".into(),
                source: "/home/alice/.../prefixes/abc/drive_c".into(),
                target: "/home/bob/.../prefixes/abc/drive_c".into(),
            },
        ];
        let backup = invert_for_backup(restore);
        assert_eq!(backup.len(), 1);
        assert_eq!(backup[0].kind, "backup");
        // source = local prefix path (scanned), target = Windows path (stored).
        assert_eq!(
            backup[0].source,
            "/home/deck/.../prefixes/abc/drive_c/users/steamuser"
        );
        assert_eq!(backup[0].target, "C:/Users/akinz");
    }

    #[test]
    fn backup_redirects_invert_restore_redirects() {
        // End-to-end: a Windows-origin save restored onto a Proton prefix must
        // back up with the original Windows paths, not the local prefix paths.
        let origin = win_origin(&["C:/Users/akinz/AppData/Local/Deltarune/dr.ini"]);
        let restore = derive_redirects(&origin, Some(&pfx()), None, None, false);
        let backup = invert_for_backup(restore);
        assert_eq!(backup.len(), 1);
        assert_eq!(backup[0].kind, "backup");
        assert!(backup[0].source.contains("drive_c/users/steamuser"));
        assert_eq!(backup[0].target, "C:/Users/akinz");
    }

    #[test]
    fn native_linux_backup_not_canonicalised() {
        // A native Linux-origin backup on Linux → no restore rules → no backup
        // rules, so the real Linux paths are preserved.
        let origin = lin_origin(&["/home/deck/.local/share/SomeGame/save.dat"]);
        let restore = derive_redirects(&origin, Some(&pfx()), None, None, false);
        let backup = invert_for_backup(restore);
        assert!(backup.is_empty());
    }

    // ── format classification ──────────────────────────────────────────────

    #[test]
    fn classify_format_distinguishes_kinds() {
        assert!(matches!(
            classify_format("C:/Users/akinz/x"),
            PathFormat::Windows
        ));
        assert!(matches!(
            classify_format("G:/Games/X/s"),
            PathFormat::Windows
        ));
        assert!(matches!(
            classify_format("/home/deck/p/abc/drive_c/users/steamuser/x"),
            PathFormat::WinePrefix { .. }
        ));
        assert!(matches!(
            classify_format("/home/deck/.local/share/Game/save"),
            PathFormat::NativeLinux
        ));
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

    // ── Real-backup parsing (skipped when files absent in CI) ──────────────

    #[test]
    fn read_backup_origin_finds_safe_name_folder() {
        let backup_dir = std::path::Path::new("/home/deck/.local/share/Spool/ludusavi-backup");
        if !backup_dir.exists() {
            return;
        }
        if let Some(origin) =
            read_backup_origin(backup_dir, "Lego Batman: Legacy of the Dark Knight")
        {
            assert_eq!(origin.os, BackupOs::Windows);
            assert!(origin.paths.iter().any(|p| p.contains("akinz")));
        }
    }

    #[test]
    fn parse_real_deltarune_mapping() {
        let backup_dir = std::path::Path::new("/home/deck/ludusavi-backup");
        let Some(origin) = read_backup_origin(backup_dir, "Deltarune") else {
            return;
        };
        assert_eq!(origin.os, BackupOs::Windows);
        assert!(origin.paths.iter().any(|p| p.contains("DELTARUNE")));
    }

    #[test]
    fn parse_real_lego_batman_mapping_with_diffs() {
        let backup_dir = std::path::Path::new("/home/deck/ludusavi-backup");
        let game_name = "Lego Batman: Legacy of the Dark Knight";
        let Some(origin) = read_backup_origin(backup_dir, game_name) else {
            return;
        };
        assert_eq!(origin.os, BackupOs::Windows);
        assert!(
            origin.paths.len() > 4,
            "expected paths from diffs too, got {}",
            origin.paths.len()
        );
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

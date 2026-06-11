//! Mounted-drive discovery and free-space queries for the "Library folders"
//! feature. The Settings UI lists detected drives (with free space) so the user
//! can add an install root per drive, and the move-install flow shows live free
//! space for each configured folder.
//!
//! Pure read-only system inspection via [`sysinfo`], plus a small helper to
//! create a chosen library folder on disk. None of this is platform-gated —
//! drives exist on every OS Spool targets.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// One mounted drive/volume, as surfaced to the Settings drive picker.
#[derive(Debug, Clone, Serialize)]
pub struct DriveInfo {
    /// Filesystem mount point — `C:\` on Windows, `/` or `/run/media/...` on
    /// Linux. This is what a library folder is rooted under.
    pub mount_point: String,
    /// OS-level volume name (often the device, e.g. `/dev/nvme0n1p2`). Used only
    /// as a secondary label.
    pub name: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub is_removable: bool,
}

/// Lists mounted drives with their free space. Powers the Settings drive picker.
///
/// Filters out pseudo / zero-size mounts (snap loopbacks, `/dev`, tmpfs with no
/// capacity) that the user can't install games onto, and de-duplicates by mount
/// point (some backends list the same mount twice). Sorted by mount point so the
/// list is stable across calls.
#[tauri::command]
pub async fn list_drives() -> Vec<DriveInfo> {
    // sysinfo stats every mount and can block (sleeping USB drives, network
    // mounts); a sync command would run that on the main thread and freeze the UI.
    tokio::task::spawn_blocking(list_drives_blocking)
        .await
        .unwrap_or_default()
}

fn list_drives_blocking() -> Vec<DriveInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<DriveInfo> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .filter_map(|d| {
            let mount_point = d.mount_point().to_string_lossy().to_string();
            if !seen.insert(mount_point.clone()) {
                return None;
            }
            Some(DriveInfo {
                mount_point,
                name: d.name().to_string_lossy().to_string(),
                total_bytes: d.total_space(),
                available_bytes: d.available_space(),
                is_removable: d.is_removable(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    out
}

/// Available bytes on the filesystem that holds `path`. Matches the drive whose
/// mount point is the longest prefix of `path` (so `/run/media/sd` wins over `/`
/// for a path under the SD card). Returns 0 when no drive matches or the path is
/// empty — the caller treats 0 as "unknown / can't verify".
#[tauri::command]
pub async fn folder_free_space(path: String) -> u64 {
    // Off the main thread — see `list_drives`.
    tokio::task::spawn_blocking(move || free_space_for(Path::new(path.trim())))
        .await
        .unwrap_or(0)
}

/// Core of [`folder_free_space`], callable from backend code that already has
/// a [`Path`] (the move-install free-space gate runs it inside its own
/// `spawn_blocking`).
pub fn free_space_for(path: &Path) -> u64 {
    if path.as_os_str().is_empty() {
        return 0;
    }
    // Resolve as far as possible: a not-yet-created destination (e.g. a brand
    // new `Spool` folder) won't canonicalize, so walk up to the nearest existing
    // ancestor, whose filesystem is the one the new folder will live on.
    let target = nearest_existing_ancestor(path);
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| target.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
        .unwrap_or(0)
}

/// Walks up from `path` to the first ancestor that exists on disk, canonicalising
/// it so prefix-matching against mount points sees real paths (symlinks/`..`
/// resolved). Falls back to the original path when nothing resolves.
fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut cur = Some(path);
    while let Some(p) = cur {
        if let Ok(canon) = std::fs::canonicalize(p) {
            return strip_verbatim(canon);
        }
        cur = p.parent();
    }
    path.to_path_buf()
}

/// Strips Windows' extended-length `\\?\` (and `\\?\UNC\`) verbatim prefix that
/// `std::fs::canonicalize` adds. sysinfo reports mount points in the plain form
/// (`C:\`), so a verbatim `\\?\C:\…` path never `starts_with` them; and the
/// prefix is ugly when shown in the UI / stored in config. No-op off Windows.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(s) = p.to_str() {
            if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
                return PathBuf::from(format!(r"\\{rest}"));
            }
            if let Some(rest) = s.strip_prefix(r"\\?\") {
                return PathBuf::from(rest);
            }
        }
    }
    p
}

/// Ensures the chosen library folder exists on disk and returns its canonical
/// path. Called when the user confirms a new library folder in Settings so we
/// create e.g. `<drive>/Spool/` up front (and store a normalised path). Errors
/// if the directory can't be created.
#[tauri::command]
pub async fn prepare_library_folder(path: String) -> AppResult<String> {
    // Off the main thread — create_dir_all/canonicalize can stall on slow media.
    tokio::task::spawn_blocking(move || prepare_library_folder_blocking(&path))
        .await
        .map_err(|e| AppError::Other(format!("prepare folder task join failed: {e}")))?
}

/// The `<app_data>/lan-games` directory LAN installs fall back to when no
/// library folders are configured. The install-location prompt's "Use Spool's
/// data folder" option registers this path as a library folder, and the
/// frontend can't derive app-data paths itself, so it asks here.
#[tauri::command]
pub fn default_lan_install_dir() -> String {
    crate::paths::app_data_dir()
        .join("lan-games")
        .to_string_lossy()
        .to_string()
}

fn prepare_library_folder_blocking(path: &str) -> AppResult<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Other("Library folder path is empty.".into()));
    }
    let p = PathBuf::from(trimmed);
    std::fs::create_dir_all(&p)
        .map_err(|e| AppError::Other(format!("couldn't create {}: {e}", p.display())))?;
    // Strip the Windows `\\?\` verbatim prefix canonicalize adds, so the stored
    // path is the plain form the UI shows and the modal's path matching expects.
    let canonical = std::fs::canonicalize(&p).map(strip_verbatim).unwrap_or(p);
    Ok(canonical.to_string_lossy().to_string())
}

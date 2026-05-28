//! Centralised filesystem path resolution.
//!
//! Every module that needs to read or write an app file goes through here, so
//! there's one place to change if the layout ever moves. The directory layout
//! mirrors the existing C# Spool app so an existing user's library and config
//! are picked up automatically on first launch.

use std::path::PathBuf;

/// Root data directory: `%LOCALAPPDATA%\Spool` on Windows,
/// `~/.local/share/Spool` on Linux, `~/Library/Application Support/Spool` on macOS.
pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .expect("local data dir must be resolvable")
        .join("Spool")
}

pub fn library_file() -> PathBuf {
    app_data_dir().join("library.json")
}

#[allow(dead_code)]
pub fn config_file() -> PathBuf {
    app_data_dir().join("config.json")
}

#[allow(dead_code)]
pub fn covers_dir() -> PathBuf {
    app_data_dir().join("covers")
}

#[allow(dead_code)]
pub fn launchers_dir() -> PathBuf {
    app_data_dir().join("launchers")
}

/// Persistent log file for `tracing` output. Matches the C# app's path so
/// users (and we) know where to look when something goes wrong. The path
/// is constructed inline in `init_tracing` for the appender's
/// directory-plus-filename API; this helper is kept available for future
/// callers (e.g. a "show debug log" menu item).
#[allow(dead_code)]
pub fn log_file() -> PathBuf {
    app_data_dir().join("debug.log")
}

/// Legacy data root used by the C# `ludusavi-wrap` builds before the
/// Spool rename. Migration looks here to pick up an existing user's
/// config + library on first launch.
fn legacy_ludusavi_wrap_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("ludusavi-wrap"))
}

/// One-shot migration of pre-rename data from `%LOCALAPPDATA%\
/// ludusavi-wrap` into `%LOCALAPPDATA%\Spool`. Runs at startup, before
/// `Config::load` and `Library::load`, so the loaded state already
/// reflects the migrated files. Idempotent via a `.migrated-from-
/// ludusavi-wrap` marker dropped into the new dir on first attempt.
///
/// Rules:
///   * no legacy dir → nothing to do (fresh installs, non-Windows)
///   * marker exists → already ran, never re-migrate
///   * Spool dir already has a `library.json` → user has been running
///     Spool already; refuse to overwrite, just lay down the marker
///     so we stop checking
///   * otherwise → copy every file/folder under the legacy dir into
///     Spool's dir, preserving structure
///
/// Best-effort: a failure here is logged but doesn't fail the app.
/// Worst case the user has both directories and has to point Spool
/// at the right ludusavi.exe manually.
pub fn migrate_from_ludusavi_wrap() {
    let Some(old) = legacy_ludusavi_wrap_dir() else {
        return;
    };
    if !old.is_dir() {
        return;
    }
    let new = app_data_dir();
    let marker = new.join(".migrated-from-ludusavi-wrap");
    if marker.exists() {
        return;
    }

    // Ensure the destination exists for the marker write either way.
    if let Err(e) = std::fs::create_dir_all(&new) {
        tracing::warn!(error = %e, "migration: create Spool dir failed");
        return;
    }

    // If Spool already has a library, don't overwrite — drop a marker
    // so we don't keep checking each launch.
    let has_library = new.join("library.json").is_file();
    if has_library {
        let _ = std::fs::write(&marker, "skipped: Spool dir already populated");
        tracing::info!("migration: Spool dir already has library.json, skipping");
        return;
    }

    match copy_dir_contents(&old, &new) {
        Ok(n) => {
            tracing::info!(files = n, "migration: copied {n} files from ludusavi-wrap");
        }
        Err(e) => {
            tracing::warn!(error = %e, "migration: partial copy from ludusavi-wrap");
        }
    }
    let _ = std::fs::write(&marker, "ok");
}

/// Recursive copy of every file under `src` into `dst`, preserving
/// directory structure. Returns the number of files written. Skips
/// hidden / metadata-only entries that wouldn't help the user
/// (`.bak`, `.tmp`).
fn copy_dir_contents(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<usize> {
    let mut count = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let from = entry.path();
        let to = dst.join(&name);

        if file_type.is_dir() {
            std::fs::create_dir_all(&to)?;
            count += copy_dir_contents(&from, &to)?;
        } else if file_type.is_file() {
            // Skip noisy junk that just inflates the new dir.
            let n = name.to_string_lossy();
            if n.ends_with(".tmp") || n.ends_with(".bak") {
                continue;
            }
            std::fs::copy(&from, &to)?;
            count += 1;
        }
    }
    Ok(count)
}

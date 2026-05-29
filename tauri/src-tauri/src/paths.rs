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

/// Root for per-game Proton/Wine prefixes (Linux). Each game gets a
/// `<id>` subdir used as `WINEPREFIX` and as ludusavi's `--wine-prefix`.
#[allow(dead_code)]
pub fn proton_prefixes_dir() -> PathBuf {
    app_data_dir().join("prefixes")
}

/// Spool-owned ludusavi configuration directory. Passed to every ludusavi
/// invocation via `--config` so Spool controls backup path, cloud remote,
/// and (cross-device) restore redirects without touching the user's own
/// ludusavi config.
#[allow(dead_code)]
pub fn ludusavi_config_dir() -> PathBuf {
    app_data_dir().join("ludusavi")
}

#[allow(dead_code)]
pub fn ludusavi_config_file() -> PathBuf {
    ludusavi_config_dir().join("config.yaml")
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

pub fn resolve_sidecar_path(name: &str) -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let exe_ext = if cfg!(windows) { ".exe" } else { "" };

    // 1. Try with target triple suffix (development / target/debug / unpackaged release)
    let triple = target_triple();
    let name_with_triple = format!("{}-{}{}", name, triple, exe_ext);
    let dev_path = dir.join(name_with_triple);
    if dev_path.is_file() {
        return Some(dev_path);
    }

    // 2. Try without target triple suffix (production / packaged release)
    let prod_name = format!("{}{}", name, exe_ext);
    let prod_path = dir.join(prod_name);
    if prod_path.is_file() {
        return Some(prod_path);
    }

    None
}

pub fn resolve_ludusavi_path(configured_path: &str) -> Option<std::path::PathBuf> {
    if !configured_path.is_empty() && std::path::PathBuf::from(configured_path).is_file() {
        Some(std::path::PathBuf::from(configured_path))
    } else if let Some(bundled) = resolve_sidecar_path("ludusavi") {
        Some(bundled)
    } else {
        find_system_binary("ludusavi")
    }
}

pub fn find_system_binary(name: &str) -> Option<std::path::PathBuf> {
    let exe_name = if cfg!(windows) { format!("{}.exe", name) } else { name.to_string() };

    // Check PATH
    if let Some(path_env) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_env) {
            let candidate = dir.join(&exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn target_triple() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "x86_64-unknown-linux-gnu";
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return "aarch64-unknown-linux-gnu";
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return "x86_64-pc-windows-msvc";
    #[cfg(all(target_os = "windows", target_arch = "x86"))]
    return "i686-pc-windows-msvc";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "x86_64-apple-darwin";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "aarch64-apple-darwin";
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    return "unknown";
}

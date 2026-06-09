//! Centralised filesystem path resolution.
//!
//! Every module that needs to read or write an app file goes through here, so
//! there's one place to change if the layout ever moves. The directory layout
//! mirrors the existing C# Spool app so an existing user's library and config
//! are picked up automatically on first launch.

use std::path::{Path, PathBuf};

/// Root data directory: `%LOCALAPPDATA%\Spool` on Windows,
/// `~/.local/share/Spool` on Linux, `~/Library/Application Support/Spool` on macOS.
pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .expect("local data dir must be resolvable")
        .join("Spool")
}

/// Atomically write `bytes` to `path`: write a sibling temp file, fsync it,
/// optionally copy the existing target to `<path>.bak`, then rename the temp
/// over the target. The rename is atomic on a single filesystem (and
/// replace-over-existing on Windows, via `MoveFileExW`), so a crash or
/// force-kill mid-write leaves either the old file or the complete new one —
/// never a truncated mix. The parent directory is created if missing.
///
/// The temp file is named `<filename>.tmp.<pid>` so two processes writing the
/// same target at once don't share a temp file and clobber each other. When
/// `keep_bak` is set, the previous file is copied to `<filename>.bak` first
/// (best-effort — a failed copy doesn't abort the write) so a corrupted target
/// can be restored manually.
///
/// Returns [`io::Result`](std::io::Result); callers returning
/// [`AppResult`](crate::error::AppResult) get the conversion for free via `?`,
/// since `AppError: From<io::Error>`.
pub fn write_atomic(path: &Path, bytes: &[u8], keep_bak: bool) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name")
    })?;
    let tmp = path.with_file_name(format!("{file_name}.tmp.{}", std::process::id()));

    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        // fsync the data + metadata before the rename so a power loss right
        // after the rename can't surface an empty/short file (the rename could
        // otherwise reach disk before the data does).
        f.sync_all()?;
    }

    if keep_bak && path.is_file() {
        let _ = std::fs::copy(path, path.with_file_name(format!("{file_name}.bak")));
    }

    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn library_file() -> PathBuf {
    app_data_dir().join("library.json")
}

/// SQLite database backing the game library. Lives next to the legacy
/// `library.json`, which is imported once into this DB then renamed to
/// `library.json.migrated` (see `library::Library::open`).
pub fn library_db() -> PathBuf {
    app_data_dir().join("library.db")
}

#[allow(dead_code)]
pub fn config_file() -> PathBuf {
    app_data_dir().join("config.json")
}

/// Record of the in-progress launch session, written by attached `--run` mode
/// so the Decky plugin can decide whether a forced-close fallback backup is
/// needed. Removed/marked done once a backup succeeds.
pub fn active_session_file() -> PathBuf {
    app_data_dir().join("active-session.json")
}

/// File holding the loopback TCP port of the companion Decky plugin server
/// (`spool --headless-server`). The server writes its resolved port here on
/// startup; the Decky plugin (both the Python backend and the React UI, via a
/// `callable`) reads it to build the `http://127.0.0.1:<port>` base URL.
#[cfg(unix)]
pub fn plugin_http_port_path() -> PathBuf {
    app_data_dir().join("plugin-http-port")
}

/// Lock file backing the machine-wide save backup/upload mutex (see
/// `proc_lock.rs`). An empty marker file whose only purpose is to be `flock`ed;
/// it's never read or written. The OS releases the advisory lock when the
/// holding process exits, so a crashed/force-killed holder can't wedge it.
pub fn backup_lock_file() -> PathBuf {
    app_data_dir().join("backup.lock")
}

/// Marker file for the machine-wide lock serialising read-modify-write updates
/// to the Spool-owned ludusavi `config.yaml` across processes. Same lifecycle as
/// [`backup_lock_file`] — only its lock state matters, never its contents.
pub fn ludusavi_config_lock_file() -> PathBuf {
    app_data_dir().join("ludusavi-config.lock")
}

/// Marker file for the machine-wide lock serialising the brief read-modify-write
/// of this device's cross-device blob (`_spool/devices/<id>.json`) across
/// processes. Several Spool processes share one `device_id`, and the blob's
/// `playtime` map is a `+=` accumulator, so concurrent updates must serialise or
/// one is silently lost. Same lifecycle as [`backup_lock_file`] — only its lock
/// state matters, never its contents.
pub fn control_plane_lock_file() -> PathBuf {
    app_data_dir().join("control-plane.lock")
}

/// Directory holding the per-game run-lock marker files (see
/// [`run_lock_file`] / `proc_lock::try_acquire_run`).
pub fn run_locks_dir() -> PathBuf {
    app_data_dir().join("locks")
}

/// Marker file for the machine-wide **per-game run lock** — `flock`ed by the run
/// workflow for a whole play session, and by a disk-wipe (uninstall / delete)
/// for the wipe's duration, so the two can never overlap for one game across any
/// Spool process (tray GUI, attached `spool --run`, Decky headless server).
/// Same lifecycle as [`backup_lock_file`] — only its lock state matters, and the
/// OS frees it when the holder exits. `game_id` is a UUID; sanitised defensively
/// so a stray id can't escape the locks dir.
pub fn run_lock_file(game_id: &str) -> PathBuf {
    let safe: String = game_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    run_locks_dir().join(format!("run-{safe}.lock"))
}

/// Marker file for the machine-wide **per-install LAN lock** — `flock`ed by a
/// peer install for the whole transfer so two Spool processes (the tray GUI and
/// the Decky headless server) can't install the *same* game into the same
/// `<base>.partial` staging directory at once and corrupt it. Keyed by the
/// install's base folder name (see `install_base_name`), which is the contended
/// filesystem resource. Same lifecycle as [`backup_lock_file`] — only its lock
/// state matters, and the OS frees it when the holder exits. `base` is already a
/// safe filename; sanitised again defensively so it can't escape the locks dir.
pub fn lan_install_lock_file(base: &str) -> PathBuf {
    let safe: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    let hash = blake3::hash(base.as_bytes()).to_hex().to_string();
    let hash_prefix = &hash[..16];
    run_locks_dir().join(format!("lan-install-{safe}-{hash_prefix}.lock"))
}

pub fn covers_dir() -> PathBuf {
    app_data_dir().join("covers")
}

pub fn heroes_dir() -> PathBuf {
    app_data_dir().join("heroes")
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

/// Default root for games installed through the guided Windows-installer flow
/// (`guided_install.rs`): `~/.local/share/Spool/games/<name>` on Linux. Each
/// install gets its own subfolder, mounted into the Wine prefix as a drive so
/// the installer's `setup.exe` can write there.
#[allow(dead_code)]
pub fn installed_games_dir() -> PathBuf {
    app_data_dir().join("games")
}

/// Fixed-path wrapper script that external launchers (Steam shortcuts, Armoury
/// Crate stubs) point at when Spool runs as an AppImage. See
/// [`spool_executable`] for why the AppImage path itself can't be used.
pub fn appimage_launcher_script() -> PathBuf {
    app_data_dir().join("spool-launcher.sh")
}

/// If running as an AppImage, (re)write the stable launcher wrapper so it execs
/// the *current* AppImage. Called at startup so the wrapper self-heals when the
/// AppImage path changes (version bump, AppImageLauncher relocation). No-op when
/// not running as an AppImage. Returns the wrapper path when written.
pub fn refresh_appimage_launcher() -> Option<PathBuf> {
    let appimage = std::env::var_os("APPIMAGE")?;
    let appimage = PathBuf::from(appimage);
    if appimage.as_os_str().is_empty() || !appimage.is_file() {
        return None;
    }
    let script = appimage_launcher_script();
    if let Some(parent) = script.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    // Single-quote the path so sh treats it literally — this neutralises `$`,
    // backtick, backslash and spaces alike (a double-quoted string would still
    // interpret `$`/backtick/`\`, so an AppImageLauncher-relocated path with
    // those characters could run a command substitution or expand a variable).
    // A literal `'` is closed, escaped, and reopened (`'\''`). (#279)
    let target = appimage.to_string_lossy().replace('\'', "'\\''");
    let body = format!(
        "#!/bin/sh\n# Auto-generated by Spool — execs the current AppImage.\nexec '{target}' \"$@\"\n"
    );
    std::fs::write(&script, body).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
    }
    Some(script)
}

/// Path to Spool's own executable as it should be referenced from *outside*
/// the running process — i.e. what external launchers (Steam shortcuts,
/// Armoury Crate stubs) should point at.
///
/// When running as an AppImage, `std::env::current_exe()` returns the
/// ephemeral FUSE-mount path (`/tmp/.mount_SpoolXXXX/usr/bin/spool`), and even
/// `$APPIMAGE` is volatile — the filename carries the version and
/// AppImageLauncher relocates it to `~/Applications/` with a content hash, so
/// it changes on every update. So for AppImage installs we return the stable
/// wrapper script ([`refresh_appimage_launcher`]) instead, which forwards to
/// whatever AppImage is current. Native installs (`/usr/bin/spool`) and Windows
/// return `current_exe()` directly — already stable.
pub fn spool_executable() -> Option<PathBuf> {
    if std::env::var_os("APPIMAGE").is_some() {
        let script = appimage_launcher_script();
        if script.is_file() {
            return Some(script);
        }
        if let Some(s) = refresh_appimage_launcher() {
            return Some(s);
        }
    }
    std::env::current_exe().ok()
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

pub fn resolve_ludusavi_path() -> Option<std::path::PathBuf> {
    resolve_sidecar_path("ludusavi")
}

pub fn resolve_rclone_path() -> Option<std::path::PathBuf> {
    resolve_sidecar_path("rclone")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_sidecar_returns_none_for_unknown_binary() {
        // Resolution is relative to the test runner's exe dir; a made-up name
        // must never resolve there.
        assert!(resolve_sidecar_path("definitely-not-a-real-sidecar").is_none());
    }

    #[test]
    fn lan_install_lock_file_avoids_collisions() {
        let path_a = lan_install_lock_file("A B");
        let path_b = lan_install_lock_file("A_B");
        assert_ne!(path_a, path_b, "paths with spaces vs underscores should not collide");

        let file_name_a = path_a.file_name().unwrap().to_str().unwrap();
        assert!(file_name_a.starts_with("lan-install-A_B-"));
        assert!(file_name_a.ends_with(".lock"));
        assert_eq!(file_name_a.len(), 37);
    }
}

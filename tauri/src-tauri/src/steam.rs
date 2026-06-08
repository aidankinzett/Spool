//! Steam integration — non-Steam shortcut creation.
//!
//! Writes an entry to `<steam>/userdata/<uid>/config/shortcuts.vdf` so the
//! game appears in the user's Steam library. The shortcut points at our
//! own spool binary with `--run "Name" "ExePath"` launch options — Steam
//! invokes spool, our single-instance plugin forwards the args to the
//! running tray instance, RunWorkflow kicks off.
//!
//! Heavy lifting comes from two crates:
//!   * `steamlocate` — cross-platform Steam install discovery
//!   * `steam_shortcuts_util` — binary shortcuts.vdf parse + write
//!
//! What we add on top:
//!   * Choosing which Steam user to write to (most-recently-modified)
//!   * Upsert logic (match by app_name; update existing or append)
//!   * Atomic write with `.bak` backup
//!   * Grid art placement under `<userdata>/<uid>/config/grid/<appid>{suffix}.{ext}`
//!     (where `appid` is the CRC32-based id Steam expects, computed by
//!     `steam_shortcuts_util::calculate_app_id`)

use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use steam_shortcuts_util::{
    app_id_generator::calculate_app_id, parse_shortcuts, shortcut::ShortcutOwned,
    shortcuts_to_bytes,
};
use tauri::{AppHandle, Emitter, State};

/// One discovered Steam user — the userdata subfolder + path to their
/// existing shortcuts.vdf (which may not exist yet).
#[derive(Debug, Clone)]
pub struct SteamUser {
    pub user_id: String,
    pub shortcuts_path: PathBuf,
    pub grid_dir: PathBuf,
    pub last_modified: SystemTime,
}

/// Resolves the Steam install directory. On Windows, `steamlocate` reads
/// `HKLM\SOFTWARE\WOW6432Node\Valve\Steam\InstallPath` first, which can be
/// corrupted by games that write their own path there. When steamlocate fails
/// we fall back to `HKCU\Software\Valve\Steam\SteamPath`, which Steam itself
/// maintains and is almost always correct.
#[cfg(windows)]
fn find_steam_dir() -> AppResult<PathBuf> {
    if let Ok(d) = steamlocate::SteamDir::locate() {
        let p = d.path().to_path_buf();
        if p.is_dir() {
            return Ok(p);
        }
    }
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Valve\Steam")
        .map_err(|e| AppError::Other(format!("Steam registry key not found: {e}")))?;
    let path_str: String = key
        .get_value("SteamPath")
        .map_err(|e| AppError::Other(format!("SteamPath registry value not found: {e}")))?;
    let path = PathBuf::from(&path_str);
    if !path.is_dir() {
        return Err(AppError::Other(format!(
            "Steam directory from registry ({}) does not exist",
            path.display()
        )));
    }
    Ok(path)
}

/// Path to the Steam client executable, used to issue `steam.exe -shutdown` and
/// to relaunch Steam after writing a shortcut (see [`crate::steam_process`]).
#[cfg(windows)]
pub(crate) fn steam_executable() -> AppResult<PathBuf> {
    Ok(find_steam_dir()?.join("steam.exe"))
}

#[cfg(not(windows))]
fn find_steam_dir() -> AppResult<PathBuf> {
    steamlocate::SteamDir::locate()
        .map(|d| d.path().to_path_buf())
        .map_err(|e| AppError::Other(format!("Steam not found: {e}")))
}

/// Finds Steam's install dir and enumerates users with at least one
/// shortcuts.vdf present (or a config/ folder ready for us to create
/// one in). Returns users sorted newest-first by last-modified.
pub fn locate_steam_users() -> AppResult<Vec<SteamUser>> {
    let userdata = find_steam_dir()?.join("userdata");
    if !userdata.is_dir() {
        return Err(AppError::Other(format!(
            "Steam userdata folder not found at {}",
            userdata.display()
        )));
    }

    let mut users: Vec<SteamUser> = Vec::new();
    for entry in std::fs::read_dir(&userdata)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let user_id = entry.file_name().to_string_lossy().to_string();
        // Steam uses "0" as a dummy account for offline mode; skip it.
        if user_id == "0" || user_id == "ac" {
            continue;
        }
        let config_dir = entry.path().join("config");
        let shortcuts_path = config_dir.join("shortcuts.vdf");
        let grid_dir = config_dir.join("grid");
        let last_modified = shortcuts_path
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        users.push(SteamUser {
            user_id,
            shortcuts_path,
            grid_dir,
            last_modified,
        });
    }
    users.sort_by_key(|u| std::cmp::Reverse(u.last_modified));
    Ok(users)
}

/// Reads existing shortcuts. Returns an empty Vec when the file doesn't
/// exist yet — Steam happily creates one the first time it loads.
pub fn read_shortcuts(path: &Path) -> AppResult<Vec<ShortcutOwned>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    let parsed = parse_shortcuts(&bytes)
        .map_err(|e| AppError::Other(format!("failed to parse {}: {e}", path.display())))?;
    // Convert to owned so we can hold + mutate without lifetime grief.
    Ok(parsed.iter().map(|s| s.to_owned()).collect())
}

/// Quotes a path the way Steam stores exe paths in shortcuts.vdf:
/// wrap in double-quotes and escape any embedded double-quotes.
fn quote_exe(path: &str) -> String {
    format!("\"{}\"", path.replace('"', "\\\""))
}

/// Adds (or updates by `app_name`) a Spool-managed entry. Returns the
/// computed Steam appid so callers can place grid art with the right
/// filename prefix.
pub fn upsert_spool_shortcut(
    shortcuts: &mut Vec<ShortcutOwned>,
    app_name: &str,
    spool_exe: &str,
    spool_start_dir: &str,
    launch_options: &str,
) -> u32 {
    // Steam stores exe / start_dir with their own quoting.
    let quoted_exe = quote_exe(spool_exe);
    let quoted_start = format!("\"{}\"", spool_start_dir.replace('"', "\\\""));
    let app_id = calculate_app_id(&quoted_exe, app_name);

    if let Some(existing) = shortcuts.iter_mut().find(|s| s.app_name == app_name) {
        existing.app_id = app_id;
        existing.exe = quoted_exe;
        existing.start_dir = quoted_start;
        existing.icon = spool_exe.to_string();
        existing.launch_options = launch_options.to_string();
        return app_id;
    }

    let mut entry = ShortcutOwned {
        order: shortcuts.len().to_string(),
        app_id,
        app_name: app_name.to_string(),
        exe: quoted_exe,
        start_dir: quoted_start,
        icon: spool_exe.to_string(),
        shortcut_path: String::new(),
        launch_options: launch_options.to_string(),
        is_hidden: false,
        allow_desktop_config: true,
        allow_overlay: true,
        open_vr: 0,
        dev_kit: 0,
        dev_kit_game_id: String::new(),
        dev_kit_overrite_app_id: 0,
        last_play_time: 0,
        tags: vec!["Spool".to_string()],
    };
    // Re-stamp the order field in case anyone deleted entries in Steam:
    entry.order = shortcuts.len().to_string();
    shortcuts.push(entry);
    app_id
}

/// Computes the Steam non-Steam shortcut appid for a Spool-managed game —
/// the same CRC32-based value that `upsert_spool_shortcut` stamps into
/// shortcuts.vdf. Used by the plugin server to expose the appid in
/// `/library` so the Decky UI can match game-detail pages without needing
/// the localStorage inverse map.
#[cfg(unix)]
pub fn compute_shortcut_app_id(game_name: &str, spool_exe: &str) -> u32 {
    calculate_app_id(&quote_exe(spool_exe), game_name)
}

/// Serialises + writes atomically (write `.tmp`, rename). Keeps a `.bak`
/// of the previous file so a corrupted Steam can be restored manually.
pub fn write_shortcuts(path: &Path, shortcuts: &[ShortcutOwned]) -> AppResult<()> {
    // Re-stamp the order field consecutively from 0 — Steam can choke
    // on gaps after a delete.
    let mut owned: Vec<ShortcutOwned> = shortcuts.to_vec();
    for (i, s) in owned.iter_mut().enumerate() {
        s.order = i.to_string();
    }
    let borrowed: Vec<_> = owned.iter().map(|s| s.borrow()).collect();
    let bytes = shortcuts_to_bytes(&borrowed);

    crate::paths::write_atomic(path, &bytes, true)?;
    Ok(())
}

/// Spool's own branded Big Picture artwork, generated from the cassette brand
/// by `scripts/gen-steam-art.mjs` and embedded at compile time. The Spool
/// shortcut has no SteamGridDB entry to pull from, so we ship these and drop
/// them into Steam's grid dir directly. Suffixes match Steam's grid naming
/// (`p` portrait, `` wide, `_hero`, `_logo`).
const SPOOL_ART_PORTRAIT: &[u8] = include_bytes!("../assets/steam/portrait.png");
const SPOOL_ART_WIDE: &[u8] = include_bytes!("../assets/steam/wide.png");
const SPOOL_ART_HERO: &[u8] = include_bytes!("../assets/steam/hero.png");
const SPOOL_ART_LOGO: &[u8] = include_bytes!("../assets/steam/logo.png");

/// Steam resolves grid art by globbing `<app_id><suffix>.*` (any extension), so
/// writing a new asset under a different extension than a previous run leaves
/// both files behind and Steam renders an arbitrary one. Remove any existing
/// file whose stem is *exactly* `<app_id><suffix>` (any extension) before each
/// write, so exactly one file per slot remains. The exact-stem match is what
/// keeps the empty "wide grid" suffix (`<id>.*`) from also matching `<id>p.*` /
/// `<id>_hero.*`. Best-effort — a failed removal never blocks the write. (#284)
pub(crate) fn remove_stale_grid_art(grid_dir: &Path, app_id: u32, suffix: &str) {
    let stem = format!("{app_id}{suffix}");
    let Ok(entries) = std::fs::read_dir(grid_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_stem().and_then(|s| s.to_str()) == Some(stem.as_str()) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Writes embedded image bytes into Steam's grid dir as
/// `<grid_dir>/<app_id><suffix>.png`. Used to place Spool's own branded
/// artwork (see [`SPOOL_ART_PORTRAIT`] et al.). Creates the grid dir if needed.
fn write_grid_art_bytes(
    grid_dir: &Path,
    app_id: u32,
    suffix: &str,
    bytes: &[u8],
) -> AppResult<PathBuf> {
    std::fs::create_dir_all(grid_dir)?;
    remove_stale_grid_art(grid_dir, app_id, suffix);
    let dest = grid_dir.join(format!("{app_id}{suffix}.png"));
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

/// Copies a source image file to Steam's grid dir under
/// `<grid_dir>/<app_id><suffix>.<ext>`, where `suffix` differentiates the
/// art kind ("p" for portrait cover, "" for wide grid, "_hero" for hero,
/// "_logo" for logo). Pass `None` to skip.
pub fn place_grid_art(
    grid_dir: &Path,
    app_id: u32,
    suffix: &str,
    source: Option<&Path>,
) -> AppResult<Option<PathBuf>> {
    let Some(source) = source else {
        return Ok(None);
    };
    if !source.is_file() {
        return Ok(None);
    }
    std::fs::create_dir_all(grid_dir)?;
    remove_stale_grid_art(grid_dir, app_id, suffix);
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("png");
    let dest = grid_dir.join(format!("{app_id}{suffix}.{ext}"));
    std::fs::copy(source, &dest)?;
    Ok(Some(dest))
}

/// Build the `--run "<name>" "<exe>" --attached` launch-options string. Steam
/// stores the value verbatim and splits args by shell rules at launch time, so
/// each token gets its own quoted block. Interior `"` are escaped as `\"`
/// so names/paths containing quotes don't corrupt the field.
///
/// The trailing `--attached` forces attached-launch mode: the Steam-launched
/// `spool.exe` skips the single-instance plugin (runs as its own process rather
/// than forwarding argv to the tray instance and exiting), shows the splash,
/// runs the workflow, and exits when the game closes. That lets Steam track the
/// session by the launched process tree — without it, the forwarded launch exits
/// immediately while the game runs under the tray instance, so Steam shows the
/// game as still running after it closes and "Stop" kills the whole tray app.
pub fn build_launch_options(game_name: &str, exe_path: &str) -> String {
    let name = game_name.replace('"', "\\\"");
    let exe = exe_path.replace('"', "\\\"");
    format!("--run \"{name}\" \"{exe}\" --attached")
}

// ── Tauri commands ──────────────────────────────────────────────────────────

/// Surfaced when Steam was running but didn't shut down in time, so we abort
/// rather than write a shortcut Steam would clobber on its next exit.
const STEAM_SHUTDOWN_FAILED_MSG: &str =
    "Couldn't close Steam to add the shortcut. Close Steam manually, then try again.";

/// Adds Spool itself as a non-Steam shortcut so the user can launch the
/// library from Steam's Gaming Mode on SteamOS / Steam Deck.
#[tauri::command]
pub async fn add_spool_to_steam() -> AppResult<AddToSteamResult> {
    let spool_exe = crate::paths::spool_executable()
        .ok_or_else(|| AppError::Other("can't resolve own exe path".to_string()))?;
    let spool_exe_str = spool_exe.to_string_lossy().to_string();
    let spool_start_dir = spool_exe
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let users = locate_steam_users()?;
    let user = users
        .first()
        .cloned()
        .ok_or_else(|| AppError::Other("No Steam user accounts found".into()))?;

    // Steam rewrites shortcuts.vdf from memory on exit, clobbering our write, and
    // only reads it at startup. Shut Steam down before writing, then relaunch
    // below so it loads the new shortcut. No-op when Steam isn't running.
    let restart = crate::steam_process::shut_down_for_write().await;
    // Steam was running but wouldn't close — bail before writing: Steam would
    // rewrite shortcuts.vdf from memory on its eventual exit and drop our entry.
    if restart == crate::steam_process::SteamRestart::ShutdownFailed {
        return Err(AppError::Other(STEAM_SHUTDOWN_FAILED_MSG.into()));
    }

    let mut shortcuts = read_shortcuts(&user.shortcuts_path)?;
    // No --run args — this shortcut opens the Spool library itself.
    let app_id = upsert_spool_shortcut(
        &mut shortcuts,
        "Spool",
        &spool_exe_str,
        &spool_start_dir,
        "",
    );
    write_shortcuts(&user.shortcuts_path, &shortcuts)?;

    // Place Spool's branded artwork so the shortcut presents with brand art
    // instead of a blank tile / default Steam background. Best-effort — an art
    // write failing never aborts the shortcut itself.
    let arts: [(&str, &str, &[u8]); 4] = [
        ("p", "portrait", SPOOL_ART_PORTRAIT),
        ("", "wide", SPOOL_ART_WIDE),
        ("_hero", "hero", SPOOL_ART_HERO),
        ("_logo", "logo", SPOOL_ART_LOGO),
    ];
    let mut portrait_placed = false;
    let mut extras_placed = Vec::new();
    for (suffix, label, bytes) in arts {
        match write_grid_art_bytes(&user.grid_dir, app_id, suffix, bytes) {
            Ok(_) if suffix == "p" => portrait_placed = true,
            Ok(_) => extras_placed.push(label.to_string()),
            Err(e) => tracing::warn!(%e, label, "add_spool_to_steam: failed to place branded art"),
        }
    }

    // Relaunch Steam if we shut it down, so it reloads the new shortcut.
    let steam_restarted = restart == crate::steam_process::SteamRestart::ShutDown;
    if steam_restarted {
        crate::steam_process::relaunch().await;
    }

    Ok(AddToSteamResult {
        steam_user_id: user.user_id,
        app_id,
        shortcuts_path: user.shortcuts_path.to_string_lossy().to_string(),
        portrait_placed,
        extras_placed,
        steam_restarted,
    })
}

#[tauri::command]
pub async fn add_to_steam(
    app: AppHandle,
    library: State<'_, SharedLibrary>,
    game_id: String,
) -> AppResult<AddToSteamResult> {
    // 1. Snapshot the data we need from the library (drop lock fast).
    let (app_name, exe_path, save_path_str, cover_image_path, steam_id) = {
        let entry = library
            .find(&game_id)
            .await?
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.game_name.clone(),
            entry.exe_path.clone(),
            entry.save_paths.first().cloned().unwrap_or_default(),
            entry.cover_image_path.clone(),
            entry.steam_id,
        )
    };
    drop(save_path_str); // not used yet — placeholder for future per-game start dir

    // 2. Spool binary path. `cli` mode parses --run from forwarded launches.
    //    Use the AppImage-aware resolver: when running as an AppImage,
    //    current_exe() is an ephemeral /tmp mount that only exists while Spool
    //    is running (so the shortcut would only work with Spool already open,
    //    and shows a garbage path in Steam properties). spool_executable()
    //    returns the stable .AppImage path via $APPIMAGE.
    let spool_exe = crate::paths::spool_executable()
        .ok_or_else(|| AppError::Other("can't resolve own exe path".to_string()))?;
    let spool_exe_str = spool_exe.to_string_lossy().to_string();
    let spool_start_dir = spool_exe
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    // 3. Pick the most-recently-modified Steam user. (Multi-user picker
    //    is a polish follow-up.)
    let users = locate_steam_users()?;
    let user = users
        .first()
        .cloned()
        .ok_or_else(|| AppError::Other("No Steam user accounts found".into()))?;

    // 3b. Steam rewrites shortcuts.vdf from memory on exit (clobbering our write)
    //     and only reads it at startup. Shut Steam down before writing, then
    //     relaunch in step 9 so it picks up the new shortcut. No-op when Steam
    //     isn't running or in Game Mode (Spool is a Steam child there).
    let restart = crate::steam_process::shut_down_for_write().await;
    // Steam was running but wouldn't close — bail before writing: Steam would
    // rewrite shortcuts.vdf from memory on its eventual exit and drop our entry.
    if restart == crate::steam_process::SteamRestart::ShutdownFailed {
        return Err(AppError::Other(STEAM_SHUTDOWN_FAILED_MSG.into()));
    }

    // 4. Read + upsert + write.
    let mut shortcuts = read_shortcuts(&user.shortcuts_path)?;
    let launch_options = build_launch_options(&app_name, &exe_path);
    let app_id = upsert_spool_shortcut(
        &mut shortcuts,
        &app_name,
        &spool_exe_str,
        &spool_start_dir,
        &launch_options,
    );
    write_shortcuts(&user.shortcuts_path, &shortcuts)?;

    tracing::debug!(
        grid_dir = %user.grid_dir.display(),
        app_id,
        "add_to_steam: placing artwork"
    );

    // 5. Place the portrait capsule (`<appid>p`) — the main library tile.
    //    Two-step fallback: existing library cover → fresh SteamGridDB fetch.
    //    The previous implementation only fell back to SteamGridDB when
    //    cover_image_path was None, not when the path existed but the file had
    //    been deleted or moved. Both branches are best-effort — art failures
    //    never abort the shortcut write.
    let placed_portrait = {
        // Step 1: try the library cover already on disk.
        let from_lib = match cover_image_path.as_deref() {
            Some(cover) => {
                match place_grid_art(&user.grid_dir, app_id, "p", Some(Path::new(cover))) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(cover, %e, "add_to_steam: portrait copy from library cover failed");
                        None
                    }
                }
            }
            None => None,
        };

        // Step 2: if we still don't have a portrait, fetch one from SteamGridDB.
        if from_lib.is_some() {
            from_lib
        } else {
            match crate::steamgriddb::fetch_and_save_cover(&app, &game_id).await {
                Ok(Some(fetched)) => {
                    match place_grid_art(&user.grid_dir, app_id, "p", Some(Path::new(&fetched))) {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::warn!(%e, "add_to_steam: portrait copy after SteamGridDB fetch failed");
                            None
                        }
                    }
                }
                Ok(None) => {
                    tracing::debug!("add_to_steam: no portrait from SteamGridDB (not configured or no results)");
                    None
                }
                Err(e) => {
                    tracing::warn!(%e, "add_to_steam: SteamGridDB portrait fetch failed");
                    None
                }
            }
        }
    };

    if placed_portrait.is_none() {
        tracing::warn!(app_id, "add_to_steam: portrait not placed — Steam tile will be blank");
    } else {
        tracing::debug!(app_id, "add_to_steam: portrait placed");
    }

    // 6. Fetch hero + wide grid + logo + icon (official Steam CDN first, then
    //    SteamGridDB). Resolve the SteamGridDB id once here and hand it to the
    //    bundle so it isn't looked up again. Best-effort.
    let sgdb = crate::steamgriddb::resolve_sgdb_id(&app, &app_name, steam_id)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(%e, "add_to_steam: SteamGridDB id resolve failed");
            None
        });
    let extra_arts = match crate::steamgriddb::fetch_steam_grid_bundle(
        &app,
        steam_id,
        sgdb,
        &user.grid_dir,
        app_id,
    )
    .await
    {
        Ok(arts) => arts,
        Err(e) => {
            tracing::warn!(%e, "add_to_steam: SteamGridDB bundle fetch failed");
            Vec::new()
        }
    };

    // 7. Keep the "Spool" Steam collection in sync with the managed shortcuts.
    //    Pass the in-memory shortcuts we just wrote (authoritative — avoids a
    //    re-read that could diverge). Best-effort: a collection write failing
    //    never aborts the shortcut (the game is already in the library; the
    //    collection is a convenience).
    if let Err(e) = crate::steam_collections::sync_spool_collection(&user, &shortcuts) {
        tracing::warn!(%e, "add_to_steam: failed to sync Spool collection");
    }

    // 8. Notify the library so the UI can react if any state changed.
    let _ = app.emit("library:changed", &game_id);

    // 9. Relaunch Steam if we shut it down, so it reloads the new shortcut.
    let steam_restarted = restart == crate::steam_process::SteamRestart::ShutDown;
    if steam_restarted {
        crate::steam_process::relaunch().await;
    }

    Ok(AddToSteamResult {
        steam_user_id: user.user_id,
        app_id,
        shortcuts_path: user.shortcuts_path.to_string_lossy().to_string(),
        portrait_placed: placed_portrait.is_some(),
        extras_placed: extra_arts,
        steam_restarted,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct AddToSteamResult {
    pub steam_user_id: String,
    pub app_id: u32,
    pub shortcuts_path: String,
    pub portrait_placed: bool,
    pub extras_placed: Vec<String>,
    /// True when Spool shut Steam down and relaunched it so the new shortcut is
    /// loaded immediately. False when Steam wasn't running (or couldn't be
    /// restarted), in which case the user restarts Steam to see the shortcut.
    pub steam_restarted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_options_quote_both_args() {
        assert_eq!(
            build_launch_options("Hades II", "C:\\Games\\Hades II\\Hades II.exe"),
            "--run \"Hades II\" \"C:\\Games\\Hades II\\Hades II.exe\" --attached"
        );
    }

    #[test]
    fn upsert_appends_new_entry() {
        let mut shortcuts: Vec<ShortcutOwned> = Vec::new();
        let _id = upsert_spool_shortcut(
            &mut shortcuts,
            "Hades",
            "C:/Tools/spool.exe",
            "C:/Tools",
            "--run \"Hades\" \"C:/Games/Hades/Hades.exe\"",
        );
        assert_eq!(shortcuts.len(), 1);
        assert_eq!(shortcuts[0].app_name, "Hades");
        assert!(shortcuts[0].launch_options.contains("--run"));
    }

    #[test]
    fn upsert_updates_existing_by_name() {
        let mut shortcuts: Vec<ShortcutOwned> = Vec::new();
        let _ = upsert_spool_shortcut(
            &mut shortcuts,
            "Hades",
            "C:/Tools/spool.exe",
            "C:/Tools",
            "--run \"Hades\" \"old\"",
        );
        let _ = upsert_spool_shortcut(
            &mut shortcuts,
            "Hades",
            "C:/Tools/spool.exe",
            "C:/Tools",
            "--run \"Hades\" \"new\"",
        );
        assert_eq!(shortcuts.len(), 1, "should update in-place, not duplicate");
        assert!(shortcuts[0].launch_options.contains("new"));
    }

    #[test]
    fn appid_is_stable_for_same_inputs() {
        let mut a: Vec<ShortcutOwned> = Vec::new();
        let mut b: Vec<ShortcutOwned> = Vec::new();
        let id_a = upsert_spool_shortcut(&mut a, "Hades", "spool.exe", "/", "");
        let id_b = upsert_spool_shortcut(&mut b, "Hades", "spool.exe", "/", "");
        assert_eq!(id_a, id_b);
        // High bit set per Steam's appid convention.
        assert!(id_a & 0x80000000 != 0);
    }

    #[test]
    fn remove_stale_grid_art_matches_exact_stem_only() {
        let dir = tempfile::tempdir().unwrap();
        let g = dir.path();
        for f in [
            "12345.png",      // wide-grid slot (suffix "")
            "12345.jpg",      // stale wide-grid, different extension
            "12345p.png",     // portrait slot
            "12345_hero.jpg", // hero slot
            "99999p.png",     // a different app — must never be touched
        ] {
            std::fs::write(g.join(f), b"x").unwrap();
        }

        // Cleaning the wide-grid slot drops only `12345.*`, not `12345p.*` etc.
        remove_stale_grid_art(g, 12345, "");
        assert!(!g.join("12345.png").exists());
        assert!(!g.join("12345.jpg").exists());
        assert!(g.join("12345p.png").exists(), "portrait slot untouched");
        assert!(g.join("12345_hero.jpg").exists(), "hero slot untouched");
        assert!(g.join("99999p.png").exists(), "other app untouched");

        // Cleaning the portrait slot drops only `12345p.*`.
        remove_stale_grid_art(g, 12345, "p");
        assert!(!g.join("12345p.png").exists());
        assert!(g.join("99999p.png").exists(), "other app still untouched");
    }
}

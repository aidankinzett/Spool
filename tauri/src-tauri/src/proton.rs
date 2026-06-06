//! Proton / umu-launcher integration (Linux).
//!
//! Spool runs Windows `.exe` games on Linux by handing them to
//! [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher)
//! (`umu-run`), which sets up the Steam Linux Runtime container, picks a
//! Proton build, and manages a per-game Wine prefix. umu-run is env-driven:
//!
//!   GAMEID=umu-<id>  WINEPREFIX=<prefix root>  PROTONPATH=<proton dir>  umu-run <exe> [args]
//!
//! The launch env also caps umu's per-launch network calls (`UMU_HTTP_TIMEOUT`
//! / `UMU_HTTP_RETRIES`) so an offline launch fails fast and falls back to the
//! cached runtime instead of stalling — see [`build_umu_launch`].
//!
//! Each game gets its own prefix under `paths::proton_prefixes_dir()/<id>`
//! (mirrors Steam's compatdata-per-appid model) so saves and config stay
//! isolated and ludusavi can target a single prefix per game.
//!
//! Everything here is Linux-only in practice; the discovery functions return
//! empty / error on Windows where native launch is used instead.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use crate::paths;
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

/// Overall ceiling on a `winetricks` install. Downloads can legitimately take
/// many minutes, but a wedged umu-run / wineserver or a stdin prompt that never
/// comes would otherwise hang the Tauri command future forever — so bound it.
const WINETRICKS_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// A discovered Proton install. Mirrored to `types.ts` as `ProtonVersion`.
#[derive(Debug, Clone, Serialize)]
pub struct ProtonVersion {
    /// Display name, e.g. `"Proton - Experimental"` or `"GE-Proton9-20"`.
    pub name: String,
    /// Absolute path to the Proton directory (the one containing `proton`).
    pub path: String,
    /// Where it was found: `"steam"` (steamapps/common) or `"compat"`
    /// (compatibilitytools.d).
    pub source: String,
}

/// True iff `dir` looks like a usable Proton install: it contains both the
/// `proton` launcher script and a `toolmanifest.vdf`.
pub fn is_valid_proton_dir(dir: &Path) -> bool {
    dir.join("proton").is_file() && dir.join("toolmanifest.vdf").is_file()
}

/// Candidate Steam roots. Both `~/.steam/steam` and `~/.local/share/Steam`
/// are checked since distros differ on which is the real dir vs a symlink.
fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".local/share/Steam"));
    }
    roots
}

/// Scan the usual locations for Proton builds and return the valid ones,
/// de-duplicated by canonical path and sorted newest-first. Empty on Windows.
pub fn installed_proton_versions() -> Vec<ProtonVersion> {
    if cfg!(windows) {
        return Vec::new();
    }

    let mut found: Vec<ProtonVersion> = Vec::new();

    let mut push_dir = |dir: &Path, source: &str| {
        if !is_valid_proton_dir(dir) {
            return;
        }
        let Some(name) = dir.file_name().map(|n| n.to_string_lossy().to_string()) else {
            return;
        };
        let canon = dir
            .canonicalize()
            .unwrap_or_else(|_| dir.to_path_buf())
            .to_string_lossy()
            .to_string();
        if found.iter().any(|p| {
            Path::new(&p.path)
                .canonicalize()
                .map(|c| c.to_string_lossy() == canon)
                .unwrap_or(false)
        }) {
            return;
        }
        found.push(ProtonVersion {
            name,
            path: dir.to_string_lossy().to_string(),
            source: source.to_string(),
        });
    };

    // Steam-bundled Proton builds under steamapps/common, plus user/system
    // compatibilitytools.d (GE-Proton, UMU-Proton, etc.).
    for root in steam_roots() {
        if let Ok(entries) = std::fs::read_dir(root.join("steamapps/common")) {
            for entry in entries.flatten() {
                let p = entry.path();
                // Match any dir whose name mentions "proton" (covers
                // "Proton - Experimental", "GE-Proton…", "UMU-Proton…");
                // is_valid_proton_dir filters out any actual game that
                // happens to contain the word.
                if p.is_dir()
                    && p.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains("proton"))
                        .unwrap_or(false)
                {
                    push_dir(&p, "steam");
                }
            }
        }
        scan_compat_dir(&root.join("compatibilitytools.d"), &mut push_dir);
    }
    scan_compat_dir(
        Path::new("/usr/share/steam/compatibilitytools.d"),
        &mut push_dir,
    );

    found.sort_by(|a, b| proton_rank(&b.name).cmp(&proton_rank(&a.name)).then(b.name.cmp(&a.name)));
    found
}

fn scan_compat_dir(dir: &Path, push_dir: &mut impl FnMut(&Path, &str)) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                push_dir(&p, "compat");
            }
        }
    }
}

/// Rough ordering score so the "best default" floats to the top of the
/// auto-pick. Community builds are preferred for non-Steam launching:
/// UMU-Proton is umu-run's native runtime (consistent prefixes, protonfixes,
/// and `winetricks` support), then GE-Proton, then stock Steam Proton
/// (Experimental, then highest numbered). Only drives the default — the user
/// can always override per-game.
fn proton_rank(name: &str) -> u32 {
    let lower = name.to_lowercase();
    if lower.contains("umu-proton") {
        return 3_000_000 + version_score(name);
    }
    if lower.contains("ge-proton") {
        return 2_000_000 + version_score(name);
    }
    if lower.contains("experimental") {
        return 1_000_000;
    }
    version_score(name) * 10
}

/// Coarse version signal: the first run of digits in the name (e.g. `10` from
/// `"Proton 10.0"` or `"UMU-Proton-10.0-4"`), clamped.
fn version_score(name: &str) -> u32 {
    let digits: String = name
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u32>().unwrap_or(0).min(9_999)
}

/// Pick the Proton dir to force via `PROTONPATH`, if the user has chosen one.
///
/// Returns `Some` only for an explicit per-game override or a config default
/// that points at a valid Proton dir. When neither is set, returns `None`:
/// Spool leaves `PROTONPATH` unset and lets `umu-run` pick its own default
/// (its bundled UMU-Proton).
///
/// We deliberately do *not* fall back to a "newest installed" guess here.
/// Forcing such a guess broke launches when it didn't match the Proton that
/// built the game's prefix — the game would exit instantly (issue: auto-Proton
/// regression after #80). Letting umu-run choose keeps prefix and runtime in
/// sync, matching what a bare `umu-run <exe>` does.
pub fn resolve_proton_path(override_path: Option<&str>, default_path: Option<&str>) -> Option<PathBuf> {
    for candidate in [override_path, default_path].into_iter().flatten() {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let p = PathBuf::from(trimmed);
        if is_valid_proton_dir(&p) {
            return Some(p);
        }
    }
    None
}

fn name_is_umu_or_ge(name: &str) -> bool {
    let l = name.to_lowercase();
    l.contains("umu-proton") || l.contains("ge-proton")
}

/// Resolve a Proton build capable of driving umu's `winetricks` (UMU-Proton or
/// GE-Proton — stock Steam Proton can't). Prefers the game's explicit override
/// if it qualifies, then the config default, then any installed UMU/GE build.
fn resolve_winetricks_proton(
    override_path: Option<&str>,
    default_path: Option<&str>,
) -> AppResult<PathBuf> {
    for cand in [override_path, default_path].into_iter().flatten() {
        let trimmed = cand.trim();
        if trimmed.is_empty() {
            continue;
        }
        let p = PathBuf::from(trimmed);
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if is_valid_proton_dir(&p) && name_is_umu_or_ge(&name) {
            return Ok(p);
        }
    }
    for v in installed_proton_versions() {
        if name_is_umu_or_ge(&v.name) {
            return Ok(PathBuf::from(v.path));
        }
    }
    Err(AppError::Other(
        "Installing dependencies needs UMU-Proton or GE-Proton. Install one (e.g. via Steam's compatibilitytools.d) and set it as the game's Proton version.".into(),
    ))
}

/// Locate the `umu-run` launcher: config override → `/usr/bin/umu-run` → PATH.
pub fn resolve_umu_run(override_path: Option<&str>) -> AppResult<PathBuf> {
    if let Some(o) = override_path {
        let trimmed = o.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    let usr = PathBuf::from("/usr/bin/umu-run");
    if usr.is_file() {
        return Ok(usr);
    }
    if let Some(path_env) = env::var_os("PATH") {
        for dir in env::split_paths(&path_env) {
            let candidate = dir.join("umu-run");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(AppError::Other(
        "umu-run not found. Install umu-launcher or set its path in Settings → Compatibility.".into(),
    ))
}

/// Per-game prefix ROOT: `<prefixes_dir>/<game_id>`. Used as `WINEPREFIX`
/// and (Phase 2) as ludusavi's `--wine-prefix` value (the ROOT, not `drive_c`).
pub fn game_prefix_path(game_id: &str) -> PathBuf {
    paths::proton_prefixes_dir().join(game_id)
}

/// The user-profile subpath inside a Proton/Wine prefix: `<prefix>/drive_c` +
/// this is where AppData / Documents / Saved Games live. umu-launcher uses the
/// fixed Steam user name `steamuser`. One constant so the runner, redirects, and
/// the custom-saves picker don't each spell it out.
pub const WINE_STEAMUSER_PROFILE: &str = "drive_c/users/steamuser";

/// The Proton/Wine prefix ROOT for a game (the dir containing `drive_c`), or
/// `None` when it doesn't launch through Proton. Single source of truth for the
/// `wine_prefix_path`-override-or-default-`game_prefix_path(id)` resolution,
/// shared by the run workflow, the backup/restore paths, and the custom-saves
/// folder picker (which previously each re-implemented it).
pub fn resolve_prefix_root(
    uses_proton: bool,
    wine_prefix_override: Option<&str>,
    game_id: &str,
) -> Option<PathBuf> {
    if !uses_proton {
        return None;
    }
    Some(
        wine_prefix_override
            .filter(|p| !p.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| game_prefix_path(game_id)),
    )
}

/// Whether a game's executable should launch through Proton.
///
/// On Windows, games always run natively, so this is always `false`. On Linux,
/// Proton is used *automatically* for any Windows `.exe` target — there is no
/// user-facing on/off toggle (issue #80); only the Proton *version* is
/// selectable. Native Linux executables (anything not ending in `.exe`) run
/// directly. This is the single source of truth for the Proton launch decision.
pub fn exe_needs_proton(exe_path: &str) -> bool {
    cfg!(not(windows)) && exe_path.trim().to_ascii_lowercase().ends_with(".exe")
}

/// A fully-resolved umu-run invocation: program, args, and environment.
pub struct UmuLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Build the umu-run command + environment for a Windows exe. The caller
/// sets the working directory (to the exe's parent) and spawns/waits.
///
/// `proton_path` is `None` when the user hasn't pinned a Proton version — in
/// that case `PROTONPATH` is left unset so umu-run picks its own default (its
/// bundled UMU-Proton). See [`resolve_proton_path`].
pub fn build_umu_launch(
    umu_run: &Path,
    exe_path: &Path,
    extra_args: &[String],
    prefix_root: &Path,
    proton_path: Option<&Path>,
    game_id: &str,
) -> UmuLaunch {
    let mut args = Vec::with_capacity(1 + extra_args.len());
    args.push(exe_path.to_string_lossy().to_string());
    args.extend(extra_args.iter().cloned());

    let mut env = vec![
        ("GAMEID".to_string(), format!("umu-{game_id}")),
        ("WINEPREFIX".to_string(), prefix_root.to_string_lossy().to_string()),
    ];
    if let Some(proton_path) = proton_path {
        env.push(("PROTONPATH".to_string(), proton_path.to_string_lossy().to_string()));
    }

    // Bound umu-run's network touches so an offline launch fails fast instead
    // of stalling a Game-Mode boot. umu contacts the network on each launch to
    // check for a Steam Runtime update and to look up the GAMEID in the umu
    // database (protonfixes); with its defaults (5 s timeout × 3 retries) every
    // one of those waits the full budget when there's no connectivity. We leave
    // the update check itself *enabled* (UMU_RUNTIME_UPDATE) so a cached runtime
    // still updates when online — only the per-request timeout and retry count
    // are capped, so an offline launch loses a few seconds and then umu falls
    // back to the already-downloaded runtime on its own. (The very first run on
    // a machine still needs the network to download the runtime — bounding
    // can't conjure a runtime that was never fetched.) Mirrors the rclone
    // fail-fast timeouts in `ludusavi_config::ensure_rclone_timeouts`. Both keys
    // defer to an explicit user override if one is already set in the env.
    if std::env::var_os("UMU_HTTP_TIMEOUT").is_none() {
        env.push(("UMU_HTTP_TIMEOUT".to_string(), "4".to_string()));
    }
    if std::env::var_os("UMU_HTTP_RETRIES").is_none() {
        env.push(("UMU_HTTP_RETRIES".to_string(), "1".to_string()));
    }

    UmuLaunch {
        program: umu_run.to_path_buf(),
        args,
        env,
    }
}

// ── Tauri commands ──────────────────────────────────────────────────────────

/// Lists discovered Proton versions for the per-game launch settings dropdown.
#[tauri::command]
pub fn list_proton_versions() -> Vec<ProtonVersion> {
    installed_proton_versions()
}

/// Installs Windows runtime dependencies into a game's Proton prefix via
/// `umu-run winetricks -q <verbs>` (e.g. `vcrun2022`, `dotnet48`). Requires a
/// UMU/GE Proton. Long-running — downloads + installs into the prefix.
/// Returns a short success message; on failure, the tail of the output.
#[tauri::command]
pub async fn install_proton_deps(
    app: AppHandle,
    game_id: String,
    verbs: String,
) -> AppResult<String> {
    // Snapshot from state, then drop guards before the long await.
    let (prefix_override, proton_override) = {
        let entry = app
            .state::<SharedLibrary>()
            .find(&game_id)
            .await?
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.wine_prefix_path.clone(),
            entry.proton_version_path.clone(),
        )
    };
    let (umu_run_path, default_proton_path) = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        (
            cfg.data.launch.umu_run_path.clone(),
            cfg.data.launch.default_proton_path.clone(),
        )
    };

    install_proton_deps_core(
        &game_id,
        &verbs,
        prefix_override.as_deref(),
        proton_override.as_deref(),
        &umu_run_path,
        &default_proton_path,
    )
    .await
}

/// State-free core of [`install_proton_deps`]. Takes the resolved per-game and
/// config values directly (no Tauri `State` injection), so it can be driven
/// from both the Tauri command and the Decky plugin's loopback HTTP server
/// (`plugin_server.rs`), which loads the library/config from disk instead.
pub async fn install_proton_deps_core(
    game_id: &str,
    verbs: &str,
    prefix_override: Option<&str>,
    proton_override: Option<&str>,
    umu_run_path: &str,
    default_proton_path: &str,
) -> AppResult<String> {
    let verb_list: Vec<String> = verbs.split_whitespace().map(String::from).collect();
    if verb_list.is_empty() {
        return Err(AppError::Other(
            "No winetricks verbs given (e.g. vcrun2022).".into(),
        ));
    }

    let umu_run = resolve_umu_run(Some(umu_run_path))?;
    let proton_path = resolve_winetricks_proton(proton_override, Some(default_proton_path))?;
    let prefix_root = prefix_override
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| game_prefix_path(game_id));
    std::fs::create_dir_all(&prefix_root)
        .map_err(|e| AppError::Other(format!("failed to create prefix dir: {e}")))?;

    let mut args = vec!["winetricks".to_string(), "-q".to_string()];
    args.extend(verb_list);

    tracing::info!(game_id, verbs, ?proton_path, "winetricks install starting");

    let mut cmd = Command::new(&umu_run);
    cmd.args(&args)
        .env("GAMEID", format!("umu-{game_id}"))
        .env("WINEPREFIX", &prefix_root)
        .env("PROTONPATH", &proton_path);
    // Strip the AppImage's environment pollution (PYTHONHOME, LD_LIBRARY_PATH,
    // GTK/GDK vars, …) so umu-run's Python sees the host environment — the same
    // treatment the launch path applies in process.rs. Without it, when Spool
    // runs as an AppImage umu-run's Python aborts instantly with "failed to
    // import encodings module" and the install never starts.
    crate::process::strip_appimage_env(&mut cmd);
    // Bound umu's per-request HTTP like the launch path (build_umu_launch) so a
    // network stall mid-download fails fast instead of hanging the whole install.
    // Defer to an explicit user override if one is already set. (#281)
    if std::env::var_os("UMU_HTTP_TIMEOUT").is_none() {
        cmd.env("UMU_HTTP_TIMEOUT", "4");
    }
    if std::env::var_os("UMU_HTTP_RETRIES").is_none() {
        cmd.env("UMU_HTTP_RETRIES", "1");
    }
    // kill_on_drop so the child is reaped when the timeout below drops its future,
    // rather than being left running detached. (#281)
    cmd.kill_on_drop(true);
    let child = cmd
        .spawn()
        .map_err(|e| AppError::Other(format!("failed to run umu-run winetricks: {e}")))?;
    let output = match tokio::time::timeout(WINETRICKS_TIMEOUT, child.wait_with_output()).await {
        Ok(res) => {
            res.map_err(|e| AppError::Other(format!("failed to run umu-run winetricks: {e}")))?
        }
        Err(_) => {
            tracing::warn!(game_id, verbs, "winetricks install timed out");
            return Err(AppError::Other(format!(
                "winetricks install timed out after {} minutes — the download may have stalled or a prompt is waiting for input. Check your network and try again.",
                WINETRICKS_TIMEOUT.as_secs() / 60
            )));
        }
    };

    if output.status.success() {
        tracing::info!(game_id, verbs, "winetricks install succeeded");
        Ok("Dependencies installed.".to_string())
    } else {
        // Surface the most useful tail (stderr, falling back to stdout).
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let src = if stderr.trim().is_empty() { stdout } else { stderr };
        let tail = src
            .lines()
            .rev()
            .take(15)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        // Log the failure too — the helper otherwise only returns this tail to
        // the UI toast, leaving nothing in debug.log to diagnose after the fact.
        tracing::error!(game_id, verbs, status = ?output.status.code(), %tail, "winetricks install failed");
        Err(AppError::Other(format!("winetricks failed:\n{tail}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn umu_launch_sets_env_and_args() {
        let l = build_umu_launch(
            Path::new("/usr/bin/umu-run"),
            Path::new("/games/Hades/Hades.exe"),
            &["--skip-intro".to_string()],
            Path::new("/prefixes/abc"),
            Some(Path::new("/proton/Experimental")),
            "abc",
        );
        assert_eq!(l.program, PathBuf::from("/usr/bin/umu-run"));
        assert_eq!(l.args, vec!["/games/Hades/Hades.exe".to_string(), "--skip-intro".to_string()]);
        let get = |k: &str| l.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("GAMEID"), Some("umu-abc".to_string()));
        assert_eq!(get("WINEPREFIX"), Some("/prefixes/abc".to_string()));
        assert_eq!(get("PROTONPATH"), Some("/proton/Experimental".to_string()));
        // Network-bounding defaults are injected (unless the user overrode them
        // in the env — not set in the test environment).
        if std::env::var_os("UMU_HTTP_TIMEOUT").is_none() {
            assert_eq!(get("UMU_HTTP_TIMEOUT"), Some("4".to_string()));
        }
        if std::env::var_os("UMU_HTTP_RETRIES").is_none() {
            assert_eq!(get("UMU_HTTP_RETRIES"), Some("1".to_string()));
        }
    }

    #[test]
    fn umu_launch_omits_protonpath_when_unpinned() {
        // No pinned Proton → PROTONPATH unset so umu-run uses its own default.
        let l = build_umu_launch(
            Path::new("/usr/bin/umu-run"),
            Path::new("/games/Hades/Hades.exe"),
            &[],
            Path::new("/prefixes/abc"),
            None,
            "abc",
        );
        assert!(l.env.iter().all(|(n, _)| n != "PROTONPATH"));
    }

    #[test]
    fn resolve_proton_path_none_when_unconfigured() {
        // Neither override nor default set → None (let umu-run choose).
        assert_eq!(resolve_proton_path(None, Some("")), None);
        assert_eq!(resolve_proton_path(Some("   "), None), None);
    }

    #[test]
    fn prefix_path_is_under_prefixes_dir() {
        let p = game_prefix_path("xyz");
        assert!(p.ends_with("prefixes/xyz"));
    }

    #[test]
    fn rank_prefers_umu_then_ge_then_stock() {
        // Community builds outrank stock; UMU above GE above Experimental.
        assert!(proton_rank("UMU-Proton-10.0-4") > proton_rank("GE-Proton9-20"));
        assert!(proton_rank("GE-Proton9-20") > proton_rank("Proton - Experimental"));
        assert!(proton_rank("Proton - Experimental") > proton_rank("Proton 11.0"));
        assert!(proton_rank("Proton 11.0") > proton_rank("Proton 10.0"));
    }
}

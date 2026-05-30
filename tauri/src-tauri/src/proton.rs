//! Proton / umu-launcher integration (Linux).
//!
//! Spool runs Windows `.exe` games on Linux by handing them to
//! [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher)
//! (`umu-run`), which sets up the Steam Linux Runtime container, picks a
//! Proton build, and manages a per-game Wine prefix. umu-run is env-driven:
//!
//!   GAMEID=umu-<id>  WINEPREFIX=<prefix root>  PROTONPATH=<proton dir>  umu-run <exe> [args]
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
use tauri::{AppHandle, Manager};
use tokio::process::Command;

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

/// Pick the Proton dir to launch with: explicit per-game override → config
/// default → newest discovered. Errors if none can be resolved.
pub fn resolve_proton_path(override_path: Option<&str>, default_path: Option<&str>) -> AppResult<PathBuf> {
    for candidate in [override_path, default_path].into_iter().flatten() {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let p = PathBuf::from(trimmed);
        if is_valid_proton_dir(&p) {
            return Ok(p);
        }
    }
    if let Some(newest) = installed_proton_versions().into_iter().next() {
        return Ok(PathBuf::from(newest.path));
    }
    Err(AppError::Other(
        "No Proton version found. Install Proton via Steam (or a GE-Proton build).".into(),
    ))
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
pub fn build_umu_launch(
    umu_run: &Path,
    exe_path: &Path,
    extra_args: &[String],
    prefix_root: &Path,
    proton_path: &Path,
    game_id: &str,
) -> UmuLaunch {
    let mut args = Vec::with_capacity(1 + extra_args.len());
    args.push(exe_path.to_string_lossy().to_string());
    args.extend(extra_args.iter().cloned());

    let env = vec![
        ("GAMEID".to_string(), format!("umu-{game_id}")),
        ("WINEPREFIX".to_string(), prefix_root.to_string_lossy().to_string()),
        ("PROTONPATH".to_string(), proton_path.to_string_lossy().to_string()),
    ];

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
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(&game_id)
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
            cfg.data.umu_run_path.clone(),
            cfg.data.default_proton_path.clone(),
        )
    };

    let verb_list: Vec<String> = verbs.split_whitespace().map(String::from).collect();
    if verb_list.is_empty() {
        return Err(AppError::Other(
            "No winetricks verbs given (e.g. vcrun2022).".into(),
        ));
    }

    let umu_run = resolve_umu_run(Some(&umu_run_path))?;
    let proton_path =
        resolve_winetricks_proton(proton_override.as_deref(), Some(&default_proton_path))?;
    let prefix_root = prefix_override
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| game_prefix_path(&game_id));
    std::fs::create_dir_all(&prefix_root)
        .map_err(|e| AppError::Other(format!("failed to create prefix dir: {e}")))?;

    let mut args = vec!["winetricks".to_string(), "-q".to_string()];
    args.extend(verb_list);

    let output = Command::new(&umu_run)
        .args(&args)
        .env("GAMEID", format!("umu-{game_id}"))
        .env("WINEPREFIX", &prefix_root)
        .env("PROTONPATH", &proton_path)
        .output()
        .await
        .map_err(|e| AppError::Other(format!("failed to run umu-run winetricks: {e}")))?;

    if output.status.success() {
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
            Path::new("/proton/Experimental"),
            "abc",
        );
        assert_eq!(l.program, PathBuf::from("/usr/bin/umu-run"));
        assert_eq!(l.args, vec!["/games/Hades/Hades.exe".to_string(), "--skip-intro".to_string()]);
        let get = |k: &str| l.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("GAMEID"), Some("umu-abc".to_string()));
        assert_eq!(get("WINEPREFIX"), Some("/prefixes/abc".to_string()));
        assert_eq!(get("PROTONPATH"), Some("/proton/Experimental".to_string()));
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

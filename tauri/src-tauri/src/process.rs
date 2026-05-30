//! Game process spawn + wait.
//!
//! Two launch paths, picked based on the effective Run-As-Admin flag:
//!
//!   - Normal: `tokio::process::Command` — runs as the current user,
//!     async wait, child inherits stdio, cwd set to the exe's dir.
//!   - Elevated (Windows only): `runas` crate → `ShellExecuteExW`
//!     with the `runas` verb. Triggers UAC. Sync wait wrapped in
//!     `tokio::task::spawn_blocking` so we don't block the runtime.
//!
//! Per `m07-concurrency` rule "Blocking code → spawn_blocking": the
//! elevated path holds a blocking thread for the entire game session
//! (potentially hours). That's a tokio blocking-pool thread, which
//! defaults to 512 capacity — one stuck thread is fine. We could
//! switch to direct `ShellExecuteExW` + tokio handle waiting later
//! if the trade-off ever matters.
//!
//! Caveat: the `runas` crate's API doesn't expose `lpDirectory`, so
//! elevated games don't get their exe's directory as cwd. Most games
//! work fine without it (they resolve resources relative to their
//! own exe path). For the small set that need a specific cwd, the
//! workaround is to disable Run-As-Admin and rely on the game's own
//! UAC manifest if any.

use crate::error::{AppError, AppResult};
use crate::proton;
use std::path::Path;
use tokio::process::Command;

/// Strip AppImage-injected environment pollution from a child command.
///
/// When Spool runs as an AppImage, the linuxdeploy AppRun + GTK hook rewrite
/// the environment to point at the AppImage's bundled runtime:
///   - `PYTHONHOME=$APPDIR/usr` / `PYTHONPATH` — **fatal to umu-run** (a Python
///     app): the interpreter aborts with "Failed to import encodings module".
///   - `LD_LIBRARY_PATH` prepended with `$APPDIR/...` — breaks the Steam Linux
///     Runtime container's dynamic linking.
///   - `PATH`, `XDG_DATA_DIRS`, `QT_PLUGIN_PATH`, `GST_PLUGIN_SYSTEM_PATH*`,
///     `PERLLIB`, `GSETTINGS_SCHEMA_DIR`, and `GDK_*`/`GTK_*`/`GIO_*` — all
///     pointed at the AppImage, wrong for any host tool we spawn.
///
/// We only sanitise the *child* environment; Spool keeps its own. The child
/// (umu-run → Proton → the Steam runtime → the game) brings its own complete
/// runtime and must see the host environment, not Spool's bundle.
///
/// No-op when not running from an AppImage (`APPDIR` unset) — so native
/// installs (AUR, deb/rpm) and Windows are unaffected.
fn strip_appimage_env(cmd: &mut Command) {
    let Some(appdir) = std::env::var_os("APPDIR") else {
        return;
    };
    let appdir = appdir.to_string_lossy().to_string();
    if appdir.is_empty() {
        return;
    }

    // Vars the AppImage sets wholesale (no host original preserved) → drop.
    for var in [
        "PYTHONHOME",
        "PYTHONPATH",
        "PYTHONDONTWRITEBYTECODE",
        "GDK_BACKEND",
        "GTK_THEME",
        "GTK_DATA_PREFIX",
        "GTK_PATH",
        "GTK_IM_MODULE_FILE",
        "GTK_EXE_PREFIX",
        "GDK_PIXBUF_MODULE_FILE",
        "GIO_EXTRA_MODULES",
    ] {
        cmd.env_remove(var);
    }

    // Colon-separated path vars: the AppImage prepends `$APPDIR/...` entries and
    // keeps the host original after them. Drop only the `$APPDIR` entries so the
    // child still sees the host paths.
    for var in [
        "PATH",
        "LD_LIBRARY_PATH",
        "XDG_DATA_DIRS",
        "PERLLIB",
        "QT_PLUGIN_PATH",
        "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0",
        "GSETTINGS_SCHEMA_DIR",
    ] {
        if let Some(val) = std::env::var_os(var) {
            let val = val.to_string_lossy();
            let cleaned: Vec<&str> = val
                .split(':')
                .filter(|p| !p.is_empty() && !p.starts_with(&appdir))
                .collect();
            if cleaned.is_empty() {
                cmd.env_remove(var);
            } else {
                cmd.env(var, cleaned.join(":"));
            }
        }
    }
}

/// How to launch a game. The `Native` path is unchanged from before; the
/// `Proton` path wraps the exe in umu-run with a per-game Wine prefix.
pub enum LaunchSpec<'a> {
    Native {
        run_as_admin: bool,
    },
    Proton {
        umu_run: &'a Path,
        prefix_root: &'a Path,
        /// `None` leaves `PROTONPATH` unset so umu-run picks its own default.
        proton_path: Option<&'a Path>,
        game_id: &'a str,
        extra_args: &'a [String],
    },
}

/// Spawns the game and waits for it to exit. Returns the exit code (or -1
/// if the process was killed by a signal / didn't yield a code).
pub async fn run_game(exe_path: &Path, spec: LaunchSpec<'_>) -> AppResult<i32> {
    let cwd = exe_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    match spec {
        LaunchSpec::Native { run_as_admin } => {
            if cfg!(windows) && run_as_admin {
                return run_elevated(exe_path).await;
            }

            let mut cmd = Command::new(exe_path);
            cmd.current_dir(cwd);
            strip_appimage_env(&mut cmd);
            let mut child = cmd
                .spawn()
                .map_err(|e| AppError::Other(format!("failed to start game: {e}")))?;

            let status = child
                .wait()
                .await
                .map_err(|e| AppError::Other(format!("failed waiting on game: {e}")))?;

            Ok(status.code().unwrap_or(-1))
        }
        LaunchSpec::Proton {
            umu_run,
            prefix_root,
            proton_path,
            game_id,
            extra_args,
        } => {
            let launch = proton::build_umu_launch(
                umu_run,
                exe_path,
                extra_args,
                prefix_root,
                proton_path,
                game_id,
            );

            // Block until the game exits — the run workflow's backup phase
            // depends on the real exit. (Notably NOT the detach/quick-exit
            // heuristic some launchers use.)
            //
            // strip_appimage_env removes the AppImage's environment pollution
            // (PYTHONHOME, LD_LIBRARY_PATH, GTK/GDK vars, …) so umu-run and the
            // Steam runtime container see the host environment. Without it,
            // umu-run's Python aborts instantly and the game "exits" in ~10ms.
            let mut cmd = Command::new(&launch.program);
            cmd.args(&launch.args).envs(launch.env).current_dir(cwd);
            strip_appimage_env(&mut cmd);
            let mut child = cmd
                .spawn()
                .map_err(|e| AppError::Other(format!("failed to start game via Proton: {e}")))?;

            let status = child
                .wait()
                .await
                .map_err(|e| AppError::Other(format!("failed waiting on Proton game: {e}")))?;

            Ok(status.code().unwrap_or(-1))
        }
    }
}

/// Spawns the game via ShellExecuteExW with the `runas` verb. Triggers
/// the UAC prompt; blocks the calling thread until the elevated
/// process exits. Wrapped in `spawn_blocking` by the caller so the
/// async runtime keeps moving.
#[cfg(windows)]
async fn run_elevated(exe_path: &Path) -> AppResult<i32> {
    let exe = exe_path.to_path_buf();
    let code = tokio::task::spawn_blocking(move || -> std::io::Result<i32> {
        let status = runas::Command::new(&exe).gui(true).status()?;
        Ok(status.code().unwrap_or(-1))
    })
    .await
    .map_err(|e| AppError::Other(format!("elevated spawn join: {e}")))?
    .map_err(|e| AppError::Other(format!("elevated spawn: {e}")))?;
    Ok(code)
}

#[cfg(not(windows))]
async fn run_elevated(_exe_path: &Path) -> AppResult<i32> {
    Err(AppError::Other(
        "Run-as-administrator is only supported on Windows".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_mods(cmd: &Command) -> HashMap<String, Option<String>> {
        cmd.as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_string(),
                    v.map(|s| s.to_string_lossy().to_string()),
                )
            })
            .collect()
    }

    // Single test (not two) because both phases mutate the process-global
    // APPDIR var; splitting them would race under parallel test execution.
    #[test]
    fn strip_appimage_env_behaviour() {
        // ── Phase 1: no APPDIR → no-op ──
        std::env::remove_var("APPDIR");
        let mut cmd = Command::new("true");
        strip_appimage_env(&mut cmd);
        assert_eq!(cmd.as_std().get_envs().count(), 0, "should be a no-op outside an AppImage");

        // ── Phase 2: AppImage env → sanitised ──
        std::env::set_var("APPDIR", "/tmp/.mount_SpoolXYZ");
        std::env::set_var("PYTHONHOME", "/tmp/.mount_SpoolXYZ/usr");
        std::env::set_var(
            "LD_LIBRARY_PATH",
            "/tmp/.mount_SpoolXYZ/usr/lib:/usr/lib:/usr/lib32",
        );

        let mut cmd = Command::new("true");
        strip_appimage_env(&mut cmd);
        let mods = env_mods(&cmd);

        // PYTHONHOME removed entirely (would otherwise crash umu-run's Python).
        assert_eq!(mods.get("PYTHONHOME"), Some(&None));
        // LD_LIBRARY_PATH keeps host entries, drops the $APPDIR one.
        assert_eq!(
            mods.get("LD_LIBRARY_PATH"),
            Some(&Some("/usr/lib:/usr/lib32".to_string()))
        );

        std::env::remove_var("APPDIR");
        std::env::remove_var("PYTHONHOME");
        std::env::remove_var("LD_LIBRARY_PATH");
    }
}

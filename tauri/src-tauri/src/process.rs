//! Game process spawn + wait.
//!
//! Two launch paths, picked based on the effective Run-As-Admin flag:
//!
//!   - Normal: `tokio::process::Command` — runs as the current user,
//!     async wait, child inherits stdio, cwd set to the exe's dir.
//!   - Elevated (Windows only): direct `ShellExecuteExW` with the
//!     `runas` verb. Triggers UAC. Sync wait wrapped in
//!     `tokio::task::spawn_blocking` so we don't block the runtime.
//!
//! Per `m07-concurrency` rule "Blocking code → spawn_blocking": the
//! elevated path holds a blocking thread for the entire game session
//! (potentially hours). That's a tokio blocking-pool thread, which
//! defaults to 512 capacity — one stuck thread is fine.
//!
//! We call `ShellExecuteExW` ourselves (rather than via the `runas`
//! crate) specifically so we can set `lpDirectory` to the exe's own
//! folder. Without it the elevated game inherits Spool's working
//! directory and games that resolve assets relative to cwd start but
//! never open a window — the process shows in Task Manager and then
//! dies. Matching the non-elevated path's cwd keeps the two launch
//! routes behaving identically.

use crate::error::{AppError, AppResult};
use crate::proton;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::process::Command;

/// Result of spawning and waiting for a game process.
pub struct GameExitResult {
    /// Raw exit code from the process, or -1 if the process was signalled.
    pub code: i32,
    /// Tail of umu-run's stderr, populated when the process exits in under 5
    /// seconds with a non-zero code — a reliable signal that Wine/Proton
    /// crashed before the game window opened (missing DLL, bad prefix, etc.).
    /// `None` for normal-length sessions or clean exits.
    pub crash_hint: Option<String>,
}

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
///
/// Shared with `system_open.rs`, which spawns the host file manager via
/// `xdg-open` and must hand it the host environment for the same reason
/// game launches do.
pub(crate) fn strip_appimage_env(cmd: &mut Command) {
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
        /// Additional env vars applied after the standard umu env.
        extra_env: &'a [(&'a str, &'a str)],
    },
}

/// Spawns the game and waits for it to exit.
pub async fn run_game(exe_path: &Path, spec: LaunchSpec<'_>) -> AppResult<GameExitResult> {
    let cwd = exe_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    match spec {
        LaunchSpec::Native { run_as_admin } => {
            if cfg!(windows) && run_as_admin {
                let code = run_elevated(exe_path).await?;
                return Ok(GameExitResult { code, crash_hint: None });
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

            Ok(GameExitResult { code: status.code().unwrap_or(-1), crash_hint: None })
        }
        LaunchSpec::Proton {
            umu_run,
            prefix_root,
            proton_path,
            game_id,
            extra_args,
            extra_env,
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
            tracing::info!(
                program = %launch.program.display(),
                args = ?launch.args,
                cwd = %cwd.display(),
                "spawning via umu-run"
            );

            let start = tokio::time::Instant::now();

            let mut cmd = Command::new(&launch.program);
            cmd.args(&launch.args).envs(launch.env).current_dir(cwd);
            cmd.envs(extra_env.iter().copied());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            strip_appimage_env(&mut cmd);
            let mut child = cmd
                .spawn()
                .map_err(|e| AppError::Other(format!("failed to start game via Proton: {e}")))?;

            let stdout_handle = child.stdout.take().map(|s| {
                tokio::spawn(async move {
                    use tokio::io::{AsyncBufReadExt, BufReader};
                    let mut lines = BufReader::new(s).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        tracing::info!(target: "umu", "{}", line);
                    }
                })
            });

            // Collect stderr into a buffer (for crash diagnosis) while still
            // piping every line to debug.log via tracing.
            let stderr_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
            let stderr_buf_clone = Arc::clone(&stderr_buf);
            let stderr_handle = child.stderr.take().map(|s| {
                tokio::spawn(async move {
                    use tokio::io::{AsyncBufReadExt, BufReader};
                    let mut lines = BufReader::new(s).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        tracing::warn!(target: "umu", "{}", line);
                        if let Ok(mut buf) = stderr_buf_clone.lock() {
                            buf.push(line);
                        }
                    }
                })
            });

            let status = child
                .wait()
                .await
                .map_err(|e| AppError::Other(format!("failed waiting on Proton game: {e}")))?;
            let elapsed = start.elapsed();

            if let Some(h) = stdout_handle { let _ = h.await; }
            if let Some(h) = stderr_handle { let _ = h.await; }

            let code = status.code().unwrap_or(-1);
            tracing::info!(exit_code = code, elapsed_secs = elapsed.as_secs(), "umu-run process exited");

            // A non-zero exit in under 5 seconds means the game almost certainly
            // never opened a window — Wine/Proton printed the reason to stderr
            // (missing DLL, broken prefix, etc.). Surface the tail so callers can
            // include it in the error message without the user needing debug.log.
            let crash_hint = if code != 0 && elapsed.as_secs() < 5 {
                let buf = stderr_buf.lock().unwrap_or_else(|e| e.into_inner());
                let tail = buf
                    .iter()
                    .rev()
                    .take(15)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                if tail.is_empty() { None } else { Some(tail) }
            } else {
                None
            };

            Ok(GameExitResult { code, crash_hint })
        }
    }
}

/// Spawns the game via `ShellExecuteExW` with the `runas` verb. Triggers
/// the UAC prompt; blocks the calling thread until the elevated process
/// exits. Wrapped in `spawn_blocking` by the caller so the async runtime
/// keeps moving.
///
/// Sets `lpDirectory` to the exe's own folder so the elevated game gets
/// the same working directory the non-elevated path passes via
/// `current_dir` — see the module-level note on why this matters.
#[cfg(windows)]
async fn run_elevated(exe_path: &Path) -> AppResult<i32> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = exe_path.to_path_buf();
    let dir = exe
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf());

    let to_wide = |s: &OsStr| -> Vec<u16> { s.encode_wide().chain(once(0)).collect() };

    let code = tokio::task::spawn_blocking(move || -> std::io::Result<i32> {
        // These wide buffers must outlive the ShellExecuteExW call — keep them
        // in locals the struct only borrows via raw pointers.
        let verb = to_wide(OsStr::new("runas"));
        let file = to_wide(exe.as_os_str());
        let dir_w = dir.as_ref().map(|p| to_wide(p.as_os_str()));

        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpDirectory = dir_w.as_ref().map_or(std::ptr::null(), |d| d.as_ptr());
        info.nShow = SW_SHOWNORMAL;

        // SAFETY: every pointer field references a buffer alive for this scope,
        // and the struct is fully zero-initialised before the fields we set.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_CANCELLED {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "UAC elevation was declined",
                ));
            }
            return Err(std::io::Error::from_raw_os_error(err as i32));
        }

        // SEE_MASK_NOCLOSEPROCESS gives us a process handle to wait on. If the
        // request was handed to an already-running instance there's no handle —
        // treat that as a clean launch (exit code 0).
        if info.hProcess.is_null() {
            return Ok(0);
        }

        // SAFETY: hProcess is a valid handle owned by us until CloseHandle.
        let exit_code = unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            let mut exit_code: u32 = 0;
            GetExitCodeProcess(info.hProcess, &mut exit_code);
            CloseHandle(info.hProcess);
            exit_code
        };
        Ok(exit_code as i32)
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

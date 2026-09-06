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
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::process::Command;

/// How many lines of a Proton child's output reach `debug.log` per launch.
///
/// A healthy umu launch prints on the order of a hundred lines, so this keeps
/// the diagnostics while capping a child that never stops talking: umu's
/// accessibility layer can throw in a loop, printing a .NET stack trace each
/// time, which is how one session produced ~1.2M lines and a 220 MB log (#512).
/// Past the cap the lines are counted, and the count is logged when the stream
/// ends.
const UMU_RELAY_LINE_CAP: usize = 500;

/// Trailing stderr lines kept in memory for the crash hint. The hint itself
/// shows the last 15; the rest are headroom for reading a little further back
/// without the buffer tracking a whole session's output.
const UMU_STDERR_TAIL_LINES: usize = 50;

/// Bytes of a single umu output line buffered before its newline. Past this the
/// rest of the line is dropped rather than stored, so a child that emits a long
/// run of bytes with no newline can't grow memory or the log without bound —
/// `tokio`'s `Lines` / `read_until` buffer a whole record before any cap can
/// see it. Set far above any real umu line, which run well under 1 KiB.
const UMU_RELAY_LINE_MAX_BYTES: usize = 64 * 1024;

/// Append to the crash-hint tail, dropping the oldest line once it's full.
fn push_tail(buf: &mut VecDeque<String>, line: String) {
    if buf.len() == UMU_STDERR_TAIL_LINES {
        buf.pop_front();
    }
    buf.push_back(line);
}

/// Relay one of a Proton child's output streams to `debug.log`, bounded three
/// ways so a child that never stops talking can't blow up memory or the log
/// (#512):
///
///   * at most [`UMU_RELAY_LINE_CAP`] lines reach tracing; the rest are counted;
///   * a line is buffered only up to [`UMU_RELAY_LINE_MAX_BYTES`]; once it hits
///     that, the rest of the line is drained without being stored and the whole
///     line is dropped, so a newline-free blob costs a fixed amount of memory
///     (`tokio`'s `Lines` / `read_until` buffer a whole record before any cap
///     can see it);
///   * `tail`, when given (stderr only), keeps just the last
///     [`UMU_STDERR_TAIL_LINES`] lines for the crash hint.
///
/// After an oversized line the relay resynchronises at the next newline and
/// carries on. `emit` writes one kept line to tracing at that stream's own level
/// — stdout at INFO, stderr at WARN.
async fn relay_umu_stream<R>(
    mut reader: R,
    stream: &'static str,
    tail: Option<Arc<Mutex<VecDeque<String>>>>,
    emit: impl Fn(&str) + Send,
) where
    R: tokio::io::AsyncRead + Unpin + Send,
{
    use tokio::io::AsyncReadExt as _;

    let mut relayed = 0usize;
    let mut suppressed = 0usize;
    let mut oversized = 0usize;

    let mut deliver = |bytes: &[u8]| {
        let text = String::from_utf8_lossy(bytes);
        if relayed < UMU_RELAY_LINE_CAP {
            emit(&text);
            relayed += 1;
        } else {
            suppressed += 1;
        }
        if let Some(tail) = tail.as_ref() {
            if let Ok(mut buf) = tail.lock() {
                push_tail(&mut buf, text.into_owned());
            }
        }
    };

    let mut chunk = [0u8; 8 * 1024];
    let mut line: Vec<u8> = Vec::new();
    // The current line has passed the byte cap: swallow the rest of it.
    let mut dropping = false;

    loop {
        let n = match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        for &byte in &chunk[..n] {
            if byte == b'\n' {
                if dropping {
                    oversized += 1;
                } else {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    deliver(&line);
                }
                line.clear();
                dropping = false;
            } else if !dropping {
                if line.len() >= UMU_RELAY_LINE_MAX_BYTES {
                    line.clear();
                    dropping = true;
                } else {
                    line.push(byte);
                }
            }
        }
    }
    // A trailing line with no newline still counts.
    if dropping {
        oversized += 1;
    } else if !line.is_empty() {
        deliver(&line);
    }

    if suppressed > 0 || oversized > 0 {
        tracing::warn!(
            target: "umu",
            stream,
            suppressed,
            oversized,
            line_cap = UMU_RELAY_LINE_CAP,
            max_line_bytes = UMU_RELAY_LINE_MAX_BYTES,
            "umu output truncated to keep debug.log bounded"
        );
    }
}

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
///     Runtime container's dynamic linking (and leaks the AppImage's bundled
///     libs, e.g. an old libzstd, into the child's host Python). Entries from
///     *any* AppImage mount are dropped, not just the current `$APPDIR` — see
///     the path-var loop for why stale mounts pile up.
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
/// Two variants cover the two command types used in the codebase:
/// - [`strip_appimage_env`] for `tokio::process::Command` (game launch, `hidden_command`)
/// - [`strip_appimage_env_blocking`] for `std::process::Command` (`blocking_command`, `load_manifest`)
///
/// Both delegate to [`appimage_env_ops`] so the stripping logic lives in one place.
///
/// Shared with `system_open.rs`, which spawns the host file manager via
/// `xdg-open` and must hand it the host environment for the same reason
/// game launches do.
enum EnvOp {
    Remove(&'static str),
    Set(&'static str, String),
}

/// Computes the set of environment mutations needed to strip AppImage pollution.
/// Returns an empty Vec when not running inside an AppImage (`APPDIR` unset).
fn appimage_env_ops() -> Vec<EnvOp> {
    let Some(appdir) = std::env::var_os("APPDIR") else {
        return vec![];
    };
    let appdir = appdir.to_string_lossy().to_string();
    if appdir.is_empty() {
        return vec![];
    }

    let mut ops = Vec::new();

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
        ops.push(EnvOp::Remove(var));
    }

    // Colon-separated path vars: the AppImage prepends `$APPDIR/...` entries and
    // keeps the host original after them. Drop the `$APPDIR` entries so the child
    // still sees the host paths.
    //
    // We also drop entries from *other* AppImage mounts (`/.mount_<App><rand>`).
    // When one Spool AppImage process relaunches/spawns another (single-instance
    // forwarding, the updater's `current_app.AppImage` copy-relaunch, the headless
    // server), AppRun prepends the new mount's lib dirs to the inherited
    // `LD_LIBRARY_PATH` but only overwrites `$APPDIR` with its own mount — so the
    // var accumulates lib dirs from several mounts while `$APPDIR` names just the
    // newest. A `starts_with($APPDIR)`-only filter would leave the stale mounts'
    // `usr/lib` on the path, and the child (e.g. umu-run's host Python) would load
    // the bundled libs from there instead of the host's — the libzstd 1.4.8 the
    // AppImage ships lacks `ZSTD_defaultCLevel`, which crashed umu-run's `_zstd`
    // import. Matching the AppImage mount-dir convention strips every generation.
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
                .filter(|p| {
                    !p.is_empty() && !p.starts_with(&appdir) && !p.contains("/.mount_")
                })
                .collect();
            if cleaned.is_empty() {
                ops.push(EnvOp::Remove(var));
            } else {
                ops.push(EnvOp::Set(var, cleaned.join(":")));
            }
        }
    }

    ops
}

/// Strips AppImage env pollution from a `tokio::process::Command`.
pub(crate) fn strip_appimage_env(cmd: &mut Command) {
    for op in appimage_env_ops() {
        match op {
            EnvOp::Remove(v) => {
                cmd.env_remove(v);
            }
            EnvOp::Set(v, s) => {
                cmd.env(v, s);
            }
        }
    }
}

/// Strips AppImage env pollution from a `std::process::Command`.
pub(crate) fn strip_appimage_env_blocking(cmd: &mut std::process::Command) {
    for op in appimage_env_ops() {
        match op {
            EnvOp::Remove(v) => {
                cmd.env_remove(v);
            }
            EnvOp::Set(v, s) => {
                cmd.env(v, s);
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
                return Ok(GameExitResult {
                    code,
                    crash_hint: None,
                });
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

            Ok(GameExitResult {
                code: status.code().unwrap_or(-1),
                crash_hint: None,
            })
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
                crate::config::offline_mode_enabled(),
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
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            strip_appimage_env(&mut cmd);
            cmd.envs(extra_env.iter().copied());
            let mut child = cmd
                .spawn()
                .map_err(|e| AppError::Other(format!("failed to start game via Proton: {e}")))?;

            let stdout_handle = child.stdout.take().map(|s| {
                tokio::spawn(relay_umu_stream(s, "stdout", None, |line| {
                    tracing::info!(target: "umu", "{}", line)
                }))
            });

            // Relay stderr to debug.log and, unlike stdout, keep the tail for
            // crash diagnosis — `stderr_buf` is the ring the crash hint reads its
            // last 15 lines from. Both the relay and the ring are bounded; see
            // `relay_umu_stream`. umu's accessibility layer throwing in a loop is
            // what wrote ~1.2M lines and a 220 MB debug.log in #512.
            let stderr_buf: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
            let stderr_handle = child.stderr.take().map({
                let tail = Arc::clone(&stderr_buf);
                move |s| {
                    tokio::spawn(relay_umu_stream(s, "stderr", Some(tail), |line| {
                        tracing::warn!(target: "umu", "{}", line)
                    }))
                }
            });

            let status = child
                .wait()
                .await
                .map_err(|e| AppError::Other(format!("failed waiting on Proton game: {e}")))?;
            let elapsed = start.elapsed();

            if let Some(h) = stdout_handle {
                let _ = h.await;
            }
            if let Some(h) = stderr_handle {
                let _ = h.await;
            }

            let code = status.code().unwrap_or(-1);
            tracing::info!(
                exit_code = code,
                elapsed_secs = elapsed.as_secs(),
                "umu-run process exited"
            );

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
                if tail.is_empty() {
                    None
                } else {
                    Some(tail)
                }
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

    #[test]
    fn push_tail_keeps_the_newest_lines_and_stays_bounded() {
        // The crash hint reads the end of the stream, so a child that never
        // stops printing must cost a fixed amount of memory, not a growing one
        // (#512).
        let mut buf: VecDeque<String> = VecDeque::new();
        for i in 0..(UMU_STDERR_TAIL_LINES * 40) {
            push_tail(&mut buf, format!("line {i}"));
        }
        assert_eq!(buf.len(), UMU_STDERR_TAIL_LINES);
        assert_eq!(
            buf.back().map(String::as_str),
            Some(format!("line {}", UMU_STDERR_TAIL_LINES * 40 - 1).as_str())
        );
        assert_eq!(
            buf.front().map(String::as_str),
            Some(format!("line {}", UMU_STDERR_TAIL_LINES * 39).as_str())
        );
    }

    #[test]
    fn push_tail_below_the_cap_keeps_everything() {
        let mut buf: VecDeque<String> = VecDeque::new();
        push_tail(&mut buf, "a".to_string());
        push_tail(&mut buf, "b".to_string());
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.front().map(String::as_str), Some("a"));
    }

    #[tokio::test]
    async fn relay_drops_a_newline_free_blob_instead_of_buffering_it() {
        // `Lines::next_line` would allocate the whole blob before any cap could
        // see it; the byte cap drains it instead, so neither the log nor the
        // crash-hint tail grows without bound (#512).
        let blob = vec![b'x'; UMU_RELAY_LINE_MAX_BYTES * 4];
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let relayed = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&relayed);
        relay_umu_stream(blob.as_slice(), "stderr", Some(Arc::clone(&tail)), move |line| {
            sink.lock().unwrap().push(line.to_string());
        })
        .await;

        assert!(relayed.lock().unwrap().is_empty(), "oversized line never relayed");
        assert!(tail.lock().unwrap().is_empty(), "oversized line never buffered");
    }

    #[tokio::test]
    async fn relay_resyncs_to_normal_lines_after_an_oversized_one() {
        let mut input = b"before\n".to_vec();
        input.resize(input.len() + UMU_RELAY_LINE_MAX_BYTES * 2, b'x');
        input.push(b'\n');
        input.extend_from_slice(b"after\n");

        let relayed = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&relayed);
        relay_umu_stream(input.as_slice(), "stdout", None, move |line| {
            sink.lock().unwrap().push(line.to_string());
        })
        .await;

        assert_eq!(*relayed.lock().unwrap(), ["before", "after"]);
    }

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
        assert_eq!(
            cmd.as_std().get_envs().count(),
            0,
            "should be a no-op outside an AppImage"
        );

        // ── Phase 2: AppImage env → sanitised ──
        std::env::set_var("APPDIR", "/tmp/.mount_SpoolXYZ");
        std::env::set_var("PYTHONHOME", "/tmp/.mount_SpoolXYZ/usr");
        // A stale mount from an earlier AppImage generation (.mount_SpoolOLD)
        // is left on the path: `$APPDIR` only names the newest mount, but the
        // var accumulates lib dirs from every generation. Both must be dropped.
        std::env::set_var(
            "LD_LIBRARY_PATH",
            "/tmp/.mount_SpoolXYZ/usr/lib:/tmp/.mount_SpoolOLD/usr/lib:/usr/lib:/usr/lib32",
        );

        let mut cmd = Command::new("true");
        strip_appimage_env(&mut cmd);
        let mods = env_mods(&cmd);

        // PYTHONHOME removed entirely (would otherwise crash umu-run's Python).
        assert_eq!(mods.get("PYTHONHOME"), Some(&None));
        // LD_LIBRARY_PATH keeps host entries, drops the current $APPDIR mount
        // AND any stale AppImage mount (else the child loads the AppImage's
        // bundled libs, e.g. the old libzstd that crashes umu-run's _zstd).
        assert_eq!(
            mods.get("LD_LIBRARY_PATH"),
            Some(&Some("/usr/lib:/usr/lib32".to_string()))
        );

        std::env::remove_var("APPDIR");
        std::env::remove_var("PYTHONHOME");
        std::env::remove_var("LD_LIBRARY_PATH");
    }
}

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
//!
//! Both native paths then wait out a launcher handoff ([`wait_for_handoff`]):
//! some games' exe starts the real game as a separate process and exits
//! (e.g. an Epic Online Services bootstrap), so the session continues while
//! any process running from the install folder is alive.
//!
//! A stop request ([`StopCtl`], from the `stop_game` command) ends the session
//! early: [`stop_processes`] kills the spawned process and its descendants, any
//! process the handoff wait would count, and for Proton every process running
//! in the game's Wine prefix. `run_game` then returns as for a normal exit.
//! The elevated path can't be stopped, since an unelevated Spool can't
//! terminate an elevated process.

use crate::error::{AppError, AppResult};
use crate::launch_options::LaunchOptions;
use crate::proton;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

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
///
/// `observe` sees every piece of output regardless of the log cap, so status
/// tracking keeps working after the cap is hit. Pieces end at `\n` or `\r`:
/// a progress bar redraws one line with `\r`, and waiting for the newline
/// would only report it once the download had finished.
async fn relay_umu_stream<R>(
    mut reader: R,
    stream: &'static str,
    tail: Option<Arc<Mutex<VecDeque<String>>>>,
    emit: impl Fn(&str) + Send,
    observe: impl Fn(&str) + Send,
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
    // Start of the piece of `line` since the last `\r`, not yet observed.
    let mut piece_start = 0usize;

    loop {
        let n = match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        for &byte in &chunk[..n] {
            if byte == b'\r' && !dropping {
                observe(&String::from_utf8_lossy(
                    &line[piece_start.min(line.len())..],
                ));
                line.push(byte);
                piece_start = line.len();
            } else if byte == b'\n' {
                if dropping {
                    oversized += 1;
                } else {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    observe(&String::from_utf8_lossy(
                        &line[piece_start.min(line.len())..],
                    ));
                    deliver(&line);
                }
                line.clear();
                piece_start = 0;
                dropping = false;
            } else if !dropping {
                if line.len() >= UMU_RELAY_LINE_MAX_BYTES {
                    line.clear();
                    piece_start = 0;
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
        observe(&String::from_utf8_lossy(
            &line[piece_start.min(line.len())..],
        ));
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
    /// True when the session ended because of a stop request.
    pub stopped: bool,
}

/// Lets the run workflow end a running game early. `token` is cancelled by the
/// `stop_game` command. `armed` is set by the workflow just before the game is
/// spawned (never for an elevated launch); the command refuses a stop until
/// then. A stop that arrives before the spawn completes takes effect as soon as
/// the process exists.
#[derive(Clone, Default)]
pub struct StopCtl {
    pub token: CancellationToken,
    pub armed: Arc<AtomicBool>,
}

impl StopCtl {
    pub fn is_armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }
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
                .filter(|p| !p.is_empty() && !p.starts_with(&appdir) && !p.contains("/.mount_"))
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

/// How to launch a game. The `Native` path runs the exe directly; the
/// `Proton` path wraps the exe in umu-run with a per-game Wine prefix.
///
/// Both carry the game's [`LaunchOptions`] (`launch_options.rs`): a non-empty
/// `wrapper` is spawned instead, with the game command (the exe, or the whole
/// umu-run invocation) appended to its arguments.
pub enum LaunchSpec<'a> {
    Native {
        run_as_admin: bool,
        options: &'a LaunchOptions,
        /// The game's install folder. After the spawned process exits, the
        /// launch waits for any process running from this folder (or the exe's
        /// own folder when the exe isn't inside it); see [`handoff_watch_dir`].
        install_dir: Option<&'a Path>,
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
        /// Wrapper program + args placed in front of the umu-run command.
        wrapper: &'a [String],
        /// Told when umu-run is downloading or unpacking its runtime or Proton
        /// (`Some(message)`), and when that finishes (`None`).
        on_setup: Option<SetupCallback>,
    },
}

/// Callback for umu-run's setup status; see [`SetupTracker`].
pub type SetupCallback = Arc<dyn Fn(Option<&str>) + Send + Sync>;

/// Classifies a piece of umu-run output as runtime/Proton setup work. umu logs
/// these as `INFO: Downloading UMU-Proton-10.0-4.tar.gz...`, `INFO: Downloading
/// steamrt3 (…), please wait...` and `INFO: Extracting …` on stderr. Only lines
/// that *begin* with the verb (after umu's `INFO: ` prefix) count, so a Wine or
/// game line that merely mentions a download isn't mistaken for umu setup.
fn umu_setup_status(line: &str) -> Option<&'static str> {
    let l = line.trim_start();
    let l = l.strip_prefix("INFO: ").unwrap_or(l).to_ascii_lowercase();
    let runtime = l.contains("steamrt");
    if l.starts_with("downloading") {
        Some(if runtime {
            "Downloading Steam Linux Runtime…"
        } else {
            "Downloading Proton…"
        })
    } else if l.starts_with("extracting") || l.starts_with("unpacking") {
        Some(if runtime {
            "Unpacking Steam Linux Runtime…"
        } else {
            "Unpacking Proton…"
        })
    } else if l.starts_with("verifying") {
        Some("Verifying Steam Linux Runtime…")
    } else {
        None
    }
}

/// How long umu must stay free of setup lines before setup counts as finished.
/// Steps are separated by short non-setup lines (`SHA512 is OK`, `Using …`), so
/// finishing on the first of those would flip the phase to `playing` between
/// every download and unpack.
const SETUP_SETTLE: Duration = Duration::from_secs(2);

#[derive(Default)]
struct SetupState {
    /// A setup message is currently showing.
    active: bool,
    /// A settle timer is running for the showing message.
    settling: bool,
    /// Bumped by every new setup line and by `finish`, which cancels any
    /// settle timer started under an older value.
    generation: u64,
    last: Option<&'static str>,
    /// When umu last printed anything; the settle time counts from here.
    last_output: Option<tokio::time::Instant>,
}

/// Turns umu-run's output into setup start/finish callbacks. Both output
/// streams feed one tracker; the callback runs under its lock so a "finished"
/// can't be reordered ahead of the "started" it follows. "Finished" is reported
/// once umu has been quiet for [`SETUP_SETTLE`], so consecutive steps read as
/// one setup.
struct SetupTracker {
    cb: Option<SetupCallback>,
    settle: Duration,
    state: Arc<Mutex<SetupState>>,
}

impl SetupTracker {
    fn new(cb: Option<SetupCallback>) -> Self {
        Self::with_settle(cb, SETUP_SETTLE)
    }

    fn with_settle(cb: Option<SetupCallback>, settle: Duration) -> Self {
        Self {
            cb,
            settle,
            state: Arc::new(Mutex::new(SetupState::default())),
        }
    }

    /// Reports `Some(message)` when a setup step starts, and `None` once setup
    /// has been quiet for the settle time. Must run inside a tokio runtime.
    fn observe(&self, line: &str) {
        let Some(cb) = self.cb.as_ref() else { return };
        if line.trim().is_empty() {
            return;
        }
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.last_output = Some(tokio::time::Instant::now());
        match umu_setup_status(line) {
            Some(msg) => {
                st.generation += 1;
                st.settling = false;
                // Repeated steps with the same message (a checksum download
                // then the archive) are one message to the user.
                if !(st.active && st.last == Some(msg)) {
                    cb(Some(msg));
                }
                st.active = true;
                st.last = Some(msg);
            }
            None if st.active && !st.settling => {
                st.settling = true;
                let generation = st.generation;
                let state = Arc::clone(&self.state);
                let cb = Arc::clone(cb);
                let settle = self.settle;
                tokio::spawn(async move {
                    // Wait until umu has been silent for `settle`: output that
                    // arrives while waiting pushes the deadline back.
                    loop {
                        let wait = {
                            let st = state.lock().unwrap_or_else(|e| e.into_inner());
                            if st.generation != generation || !st.active {
                                return;
                            }
                            let quiet_since =
                                st.last_output.unwrap_or_else(tokio::time::Instant::now);
                            (quiet_since + settle)
                                .saturating_duration_since(tokio::time::Instant::now())
                        };
                        if wait.is_zero() {
                            break;
                        }
                        tokio::time::sleep(wait).await;
                    }
                    let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                    if st.generation == generation && st.active {
                        st.active = false;
                        st.settling = false;
                        st.last = None;
                        cb(None);
                    }
                });
            }
            None => {}
        }
    }

    /// Ends any setup still showing, e.g. when umu exits mid-download, and
    /// cancels a pending settle timer so it can't fire after the caller moves
    /// on to the next phase.
    fn finish(&self) {
        let Some(cb) = self.cb.as_ref() else { return };
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.generation += 1;
        st.settling = false;
        st.last = None;
        if st.active {
            st.active = false;
            cb(None);
        }
    }
}

/// `program args…`, or `wrapper[0] wrapper[1..]… program args…` when a
/// wrapper is set.
fn wrapped_command<S: AsRef<std::ffi::OsStr>>(
    wrapper: &[String],
    program: &Path,
    args: &[S],
) -> Command {
    let mut cmd = match wrapper.split_first() {
        Some((wrap_program, wrap_args)) => {
            let mut cmd = Command::new(wrap_program);
            cmd.args(wrap_args).arg(program);
            cmd
        }
        None => Command::new(program),
    };
    cmd.args(args);
    cmd
}

/// Spawns the game and waits for it to exit.
pub async fn run_game(exe_path: &Path, spec: LaunchSpec<'_>) -> AppResult<GameExitResult> {
    run_game_stoppable(exe_path, spec, &StopCtl::default()).await
}

/// [`run_game`], ending the session early when `stop.token` is cancelled.
pub async fn run_game_stoppable(
    exe_path: &Path,
    spec: LaunchSpec<'_>,
    stop: &StopCtl,
) -> AppResult<GameExitResult> {
    let cwd = exe_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    match spec {
        LaunchSpec::Native {
            run_as_admin,
            options,
            install_dir,
        } => {
            let start = tokio::time::Instant::now();
            let started_at = unix_secs_now();
            let watch_dir = handoff_watch_dir(exe_path, install_dir).map(resolve_watch_dir);

            if cfg!(windows) && run_as_admin {
                if !options.is_empty() {
                    tracing::warn!("launch options are not applied to an elevated launch");
                }
                let code = run_elevated(exe_path).await?;
                if let Some(dir) = &watch_dir {
                    // Not stoppable: the workflow never arms an elevated
                    // launch, so `stop_game` refuses it.
                    wait_for_handoff(dir, started_at, start.elapsed(), &stop.token).await;
                }
                return Ok(GameExitResult {
                    code,
                    crash_hint: None,
                    stopped: false,
                });
            }

            let mut cmd = wrapped_command(&options.wrapper, exe_path, &options.args);
            cmd.current_dir(cwd);
            strip_appimage_env(&mut cmd);
            cmd.envs(options.env.iter().map(|(k, v)| (k, v)));
            let mut child = cmd.spawn().map_err(|e| match options.wrapper.first() {
                Some(w) => AppError::Other(format!("failed to start launch wrapper {w}: {e}")),
                None => AppError::Other(format!("failed to start game: {e}")),
            })?;

            let target = StopTarget {
                root_pid: child.id(),
                watch_dir: watch_dir.clone(),
                started_at,
                wine_prefix: None,
            };
            let (status, mut stopped) = wait_or_stop(&mut child, &stop.token, &target)
                .await
                .map_err(|e| AppError::Other(format!("failed waiting on game: {e}")))?;

            if !stopped {
                if let Some(dir) = &watch_dir {
                    wait_for_handoff(dir, started_at, start.elapsed(), &stop.token).await;
                    if stop.token.is_cancelled() {
                        // The spawned process has exited and been reaped, so
                        // its PID may now belong to something else.
                        stop_processes(StopTarget {
                            root_pid: None,
                            ..target
                        })
                        .await;
                        stopped = true;
                    }
                }
            }

            Ok(GameExitResult {
                code: status.code().unwrap_or(-1),
                crash_hint: None,
                stopped,
            })
        }
        LaunchSpec::Proton {
            umu_run,
            prefix_root,
            proton_path,
            game_id,
            extra_args,
            extra_env,
            wrapper,
            on_setup,
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
                wrapper = ?wrapper,
                program = %launch.program.display(),
                args = ?launch.args,
                cwd = %cwd.display(),
                "spawning via umu-run"
            );

            let start = tokio::time::Instant::now();
            let started_at = unix_secs_now();

            let mut cmd = wrapped_command(wrapper, &launch.program, &launch.args);
            cmd.envs(launch.env).current_dir(cwd);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            strip_appimage_env(&mut cmd);
            cmd.envs(extra_env.iter().copied());
            let mut child = cmd.spawn().map_err(|e| match wrapper.first() {
                Some(w) => AppError::Other(format!("failed to start launch wrapper {w}: {e}")),
                None => AppError::Other(format!("failed to start game via Proton: {e}")),
            })?;

            let setup = Arc::new(SetupTracker::new(on_setup));

            let stdout_handle = child.stdout.take().map(|s| {
                let setup = Arc::clone(&setup);
                tokio::spawn(relay_umu_stream(
                    s,
                    "stdout",
                    None,
                    |line| tracing::info!(target: "umu", "{}", line),
                    move |piece| setup.observe(piece),
                ))
            });

            // Relay stderr to debug.log and, unlike stdout, keep the tail for
            // crash diagnosis — `stderr_buf` is the ring the crash hint reads its
            // last 15 lines from. Both the relay and the ring are bounded; see
            // `relay_umu_stream`. umu's accessibility layer throwing in a loop is
            // what wrote ~1.2M lines and a 220 MB debug.log in #512.
            let stderr_buf: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
            let stderr_handle = child.stderr.take().map({
                let tail = Arc::clone(&stderr_buf);
                let setup = Arc::clone(&setup);
                move |s| {
                    tokio::spawn(relay_umu_stream(
                        s,
                        "stderr",
                        Some(tail),
                        |line| tracing::warn!(target: "umu", "{}", line),
                        move |piece| setup.observe(piece),
                    ))
                }
            });

            let target = StopTarget {
                root_pid: child.id(),
                watch_dir: None,
                started_at,
                wine_prefix: Some(prefix_root.to_path_buf()),
            };
            let (status, mut stopped) = wait_or_stop(&mut child, &stop.token, &target)
                .await
                .map_err(|e| AppError::Other(format!("failed waiting on Proton game: {e}")))?;
            let elapsed = start.elapsed();

            // The spawned process has been reaped, so its PID may now belong to
            // something else; the prefix match still finds the Wine processes.
            let leftover = StopTarget {
                root_pid: None,
                ..target
            };
            let relays: Vec<_> = [stdout_handle, stderr_handle]
                .into_iter()
                .flatten()
                .collect();
            let aborts: Vec<_> = relays.iter().map(|h| h.abort_handle()).collect();
            let mut joined = std::pin::pin!(async {
                for h in relays {
                    let _ = h.await;
                }
            });
            if !stopped {
                // A launch-option wrapper can exit while the game it started
                // keeps the output pipes open, so the session lasts until the
                // relays end. A stop in that window still has to work.
                tokio::select! {
                    () = &mut joined => {}
                    () = stop.token.cancelled() => {
                        stop_processes(leftover.clone()).await;
                        stopped = true;
                    }
                }
            }
            if stopped {
                // A process the stop didn't reach may still hold the pipes.
                if tokio::time::timeout(STOP_RELAY_WAIT, &mut joined)
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        "umu-run output still open after the stop; no longer relaying it"
                    );
                    for abort in &aborts {
                        abort.abort();
                    }
                }
            } else if stop.token.is_cancelled() {
                // The stop landed as the game was exiting: clear anything left
                // in the prefix and report the session as stopped.
                stop_processes(leftover).await;
                stopped = true;
            }
            // Both relays are done, so no further setup callbacks can race
            // whatever phase the caller emits next.
            setup.finish();

            let code = status.code().unwrap_or(-1);
            tracing::info!(
                exit_code = code,
                elapsed_secs = elapsed.as_secs(),
                stopped,
                "umu-run process exited"
            );

            // A non-zero exit in under 5 seconds means the game almost certainly
            // never opened a window — Wine/Proton printed the reason to stderr
            // (missing DLL, broken prefix, etc.). Surface the tail so callers can
            // include it in the error message without the user needing debug.log.
            let crash_hint = if !stopped && code != 0 && elapsed.as_secs() < 5 {
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

            Ok(GameExitResult {
                code,
                crash_hint,
                stopped,
            })
        }
    }
}

/// A spawned exe that exits within this long may be a launcher handing off to
/// the real game, so [`wait_for_handoff`] keeps looking for the game's process
/// for [`HANDOFF_GRACE`] before ending the session.
const HANDOFF_QUICK_EXIT: Duration = Duration::from_secs(60);
/// How long to look for a handed-off game process after a quick exit. An
/// Epic Online Services bootstrap can take a few seconds to start the game.
const HANDOFF_GRACE: Duration = Duration::from_secs(15);
/// Process-list poll interval while waiting on a handoff.
const HANDOFF_POLL: Duration = Duration::from_secs(2);
/// Slack on the "started after the launch" check. Process start times are
/// whole seconds, and on Linux are derived from boot time plus clock ticks.
const HANDOFF_START_SLACK_SECS: u64 = 2;

fn unix_secs_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The folder whose processes count as "the game" once the spawned exe exits:
/// the install folder when the exe is inside it (a launcher in the root often
/// starts `Binaries/Win64/Game.exe`), else the exe's own folder. A filesystem
/// root is never used, since it would match every process on the drive.
pub(crate) fn handoff_watch_dir(exe: &Path, install_dir: Option<&Path>) -> Option<PathBuf> {
    let not_root = |d: &&Path| !d.as_os_str().is_empty() && d.parent().is_some();
    let dir = install_dir
        .filter(not_root)
        .filter(|d| exe_in_dir(exe, d))
        .or_else(|| exe.parent().filter(not_root))?;
    Some(dir.to_path_buf())
}

/// On Linux, `/proc/<pid>/exe` is the path with symlinks resolved, so the
/// watch folder is resolved the same way (e.g. a `~/Games` symlink or a Deck
/// SD-card mount path). Left as-is on Windows, where the process path keeps
/// the drive letter a mapped drive was launched from and `canonicalize`
/// would turn it into a UNC path.
fn resolve_watch_dir(dir: PathBuf) -> PathBuf {
    #[cfg(unix)]
    {
        std::fs::canonicalize(&dir).unwrap_or(dir)
    }
    #[cfg(not(unix))]
    {
        dir
    }
}

/// Whether `exe` lives under `dir`. Compared per path component, and
/// case-insensitively on Windows.
fn exe_in_dir(exe: &Path, dir: &Path) -> bool {
    #[cfg(windows)]
    {
        let lower = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
        lower(exe).starts_with(lower(dir))
    }
    #[cfg(not(windows))]
    {
        exe.starts_with(dir)
    }
}

/// Refreshes the process list and returns the exe of a process (other than
/// Spool) that started at or after `started_at` (unix seconds) and runs from
/// `dir`, if there is one. Processes that were already running before the
/// launch don't count, so a resident process in a shared folder, or an
/// earlier instance of the game, can't hold the session open.
fn find_process_in_dir(sys: &mut System, dir: &Path, started_at: u64) -> Option<PathBuf> {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, stop_refresh_kind(false));
    let own_pid = std::process::id();
    let since = started_at.saturating_sub(HANDOFF_START_SLACK_SECS);
    sys.processes()
        .iter()
        .filter(|(pid, p)| pid.as_u32() != own_pid && p.start_time() >= since)
        .find_map(|(pid, p)| process_exe(*pid, p).filter(|exe| exe_in_dir(exe, dir)))
}

/// A process's exe path, falling back to [`image_path_limited`] when sysinfo
/// couldn't read it.
fn process_exe(pid: Pid, p: &sysinfo::Process) -> Option<PathBuf> {
    match p.exe().filter(|e| !e.as_os_str().is_empty()) {
        Some(e) => Some(e.to_path_buf()),
        None => image_path_limited(pid.as_u32()),
    }
}

/// A process's exe path read with a `PROCESS_QUERY_LIMITED_INFORMATION`
/// handle. sysinfo reads the path through the process's module list, which
/// needs `PROCESS_VM_READ`; that isn't granted for an elevated process when
/// Spool isn't elevated (a Run-As-Admin launch) or for one that restricts
/// handle access, and sysinfo then reports an empty path.
#[cfg(windows)]
fn image_path_limited(pid: u32) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: the handle is checked for null and closed before returning; the
    // buffer outlives the call and `len` holds its capacity in u16s.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 32_768];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        (ok != 0 && len > 0)
            .then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..len as usize])))
    }
}

#[cfg(not(windows))]
fn image_path_limited(_pid: u32) -> Option<PathBuf> {
    None
}

/// Called after the spawned game process exits. Returns once no process that
/// started since the launch (`started_at`, unix seconds) is running from
/// `dir`. When the spawned process ran for less than [`HANDOFF_QUICK_EXIT`],
/// it first gives the game [`HANDOFF_GRACE`] to show up, since a launcher may
/// exit before the process it started appears.
/// Also returns early when `stop` is cancelled.
async fn wait_for_handoff(
    dir: &Path,
    started_at: u64,
    ran_for: Duration,
    stop: &CancellationToken,
) {
    let grace_until =
        (ran_for < HANDOFF_QUICK_EXIT).then(|| tokio::time::Instant::now() + HANDOFF_GRACE);
    let mut sys = Some(System::new());
    let mut waiting = false;
    loop {
        let dir_owned = dir.to_path_buf();
        let mut s = sys.take().unwrap_or_default();
        let Ok((s, found)) = tokio::task::spawn_blocking(move || {
            let found = find_process_in_dir(&mut s, &dir_owned, started_at);
            (s, found)
        })
        .await
        else {
            return;
        };
        sys = Some(s);

        match found {
            Some(exe) => {
                if !waiting {
                    tracing::info!(
                        process = %exe.display(),
                        "launcher exited but the game is still running; waiting for it"
                    );
                    waiting = true;
                }
            }
            None if waiting => {
                tracing::info!("handed-off game process exited");
                return;
            }
            None => {
                if !grace_until.is_some_and(|t| tokio::time::Instant::now() < t) {
                    return;
                }
            }
        }
        tokio::select! {
            () = tokio::time::sleep(HANDOFF_POLL) => {}
            () = stop.cancelled() => return,
        }
    }
}

/// How long processes get to exit after `SIGTERM` before they're sent
/// `SIGKILL` (Unix). Windows has no polite equivalent for a process with no
/// window, so it terminates them straight away.
#[cfg(unix)]
const STOP_GRACE: Duration = Duration::from_secs(5);
/// How long a stop keeps looking for and killing the game's processes before
/// it gives up and lets the workflow continue.
const STOP_TIMEOUT: Duration = Duration::from_secs(15);
/// Process-list poll interval during a stop.
const STOP_POLL: Duration = Duration::from_millis(250);
/// After a Proton stop, how long to wait for the umu-run output relays to
/// finish. A process the stop didn't reach can hold the pipes open forever.
const STOP_RELAY_WAIT: Duration = Duration::from_secs(5);

/// Store clients a game may start itself (Steam through
/// `SteamAPI_RestartAppIfNecessary`, the EA or Ubisoft app) and that stay
/// running for the user afterwards. A stop never kills these or anything they
/// started. Matched against the process name, lowercased, without `.exe`.
const STORE_CLIENTS: &[&str] = &[
    "steam",
    "steamwebhelper",
    "steamservice",
    "epicgameslauncher",
    "epicwebhelper",
    "eadesktop",
    "eabackgroundservice",
    "origin",
    "originwebhelperservice",
    "upc",
    "ubisoftconnect",
    "uplaywebcore",
    "galaxyclient",
    "galaxyclienthelper",
    "battle.net",
];

/// What a stop request kills. See [`collect_stop_pids`].
#[derive(Clone)]
struct StopTarget {
    /// The spawned process (the game, a launch-option wrapper, or umu-run).
    /// Only set while it hasn't been reaped, so its PID can't have been reused.
    root_pid: Option<u32>,
    /// The handoff watch folder, for processes the game started that aren't
    /// in the spawned process's tree.
    watch_dir: Option<PathBuf>,
    /// Unix seconds at the launch. Processes that started earlier are never
    /// killed, which also guards against a reused PID.
    started_at: u64,
    /// The Proton prefix root. Every process whose `WINEPREFIX` (or umu's
    /// `STEAM_COMPAT_DATA_PATH`) is this folder belongs to the game's Wine
    /// session, wineserver included.
    wine_prefix: Option<PathBuf>,
}

/// Waits for `child` to exit, or kills the game when `stop` is cancelled first.
/// Returns the exit status and whether the game was stopped.
async fn wait_or_stop(
    child: &mut tokio::process::Child,
    stop: &CancellationToken,
    target: &StopTarget,
) -> std::io::Result<(std::process::ExitStatus, bool)> {
    tokio::select! {
        status = child.wait() => Ok((status?, false)),
        () = stop.cancelled() => {
            stop_processes(target.clone()).await;
            // Normally already dead from the above; this covers a process
            // sysinfo couldn't see.
            let _ = child.start_kill();
            Ok((child.wait().await?, true))
        }
    }
}

/// Kills the game's processes; see [`stop_processes_blocking`].
async fn stop_processes(target: StopTarget) {
    let _ = tokio::task::spawn_blocking(move || stop_processes_blocking(&target)).await;
}

/// Kills every process [`collect_stop_pids`] finds for `target` and returns
/// once none are left (or after [`STOP_TIMEOUT`]). The process list is re-read
/// every round, so a process the game starts during the stop (a launcher
/// handing off, a crash reporter) is killed too. A PID is only acted on while
/// it has the start time it had when first seen, so one the OS hands to a new
/// process during the stop is left alone.
fn stop_processes_blocking(target: &StopTarget) {
    let start = std::time::Instant::now();
    let mut sys = System::new();
    // PID → start time of every process signalled so far.
    let mut tracked: HashMap<Pid, u64> = HashMap::new();
    #[cfg(unix)]
    let mut forced = false;
    #[cfg(not(unix))]
    let forced = true;
    loop {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            stop_refresh_kind(target.wine_prefix.is_some()),
        );
        let alive = collect_stop_pids(&sys, target, &tracked);
        if alive.is_empty() {
            tracing::info!(count = tracked.len(), "game processes stopped");
            return;
        }
        for (pid, started) in &alive {
            if tracked.insert(*pid, *started).is_none() {
                if let Some(p) = sys.process(*pid) {
                    signal_stop(p, forced);
                }
            }
        }
        let elapsed = start.elapsed();
        #[cfg(unix)]
        if !forced && elapsed >= STOP_GRACE {
            forced = true;
            tracing::info!(
                count = alive.len(),
                "game processes ignored SIGTERM; killing"
            );
            for (pid, _) in &alive {
                if let Some(p) = sys.process(*pid) {
                    signal_stop(p, true);
                }
            }
        }
        if elapsed >= STOP_TIMEOUT {
            tracing::warn!(remaining = alive.len(), "gave up stopping game processes");
            return;
        }
        std::thread::sleep(STOP_POLL);
    }
}

/// `SIGTERM`, or `SIGKILL` when `force` (Unix). Windows always terminates.
fn signal_stop(p: &sysinfo::Process, force: bool) {
    #[cfg(unix)]
    {
        if force {
            p.kill();
        } else {
            p.kill_with(sysinfo::Signal::Term);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = force;
        if !p.kill() {
            tracing::warn!(pid = p.pid().as_u32(), "failed to terminate game process");
        }
    }
}

fn stop_refresh_kind(with_environ: bool) -> ProcessRefreshKind {
    let kind = ProcessRefreshKind::nothing()
        .without_tasks()
        .with_exe(UpdateKind::OnlyIfNotSet);
    if with_environ {
        kind.with_environ(UpdateKind::OnlyIfNotSet)
    } else {
        kind
    }
}

/// The live processes a stop kills, with their start times: the spawned
/// process, any process running from the watch folder or in the Wine prefix,
/// the processes `tracked` by earlier rounds, and everything below those. Only
/// processes started since the launch count. Never Spool itself, a store client
/// or anything below one, or a tracked PID whose start time changed (reused).
fn collect_stop_pids(
    sys: &System,
    target: &StopTarget,
    tracked: &HashMap<Pid, u64>,
) -> Vec<(Pid, u64)> {
    let since = target.started_at.saturating_sub(HANDOFF_START_SLACK_SECS);
    let own_pid = Pid::from_u32(std::process::id());
    let eligible = |pid: Pid| {
        sys.process(pid).is_some_and(|p| {
            pid != own_pid
                && p.start_time() >= since
                && tracked.get(&pid).is_none_or(|t| *t == p.start_time())
                && !is_store_client(&p.name().to_string_lossy())
        })
    };

    let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
    let mut roots: Vec<Pid> = tracked.keys().copied().collect();
    roots.extend(target.root_pid.map(Pid::from_u32));
    for (pid, p) in sys.processes() {
        if let Some(parent) = p.parent() {
            children.entry(parent).or_default().push(*pid);
        }
        let in_dir = target
            .watch_dir
            .as_deref()
            .is_some_and(|dir| process_exe(*pid, p).is_some_and(|exe| exe_in_dir(&exe, dir)));
        let in_prefix = target
            .wine_prefix
            .as_deref()
            .is_some_and(|prefix| runs_in_prefix(p.environ(), prefix));
        if in_dir || in_prefix {
            roots.push(*pid);
        }
    }

    descendants_and_self(roots, &children, eligible)
        .into_iter()
        .filter_map(|pid| {
            let p = sys.process(pid)?;
            // A zombie has exited and is only waiting to be reaped.
            #[cfg(unix)]
            if p.status() == sysinfo::ProcessStatus::Zombie {
                return None;
            }
            Some((pid, p.start_time()))
        })
        .collect()
}

/// The `roots` that pass `include`, plus every process below them in the
/// `parent → children` map, without descending into one that fails `include`.
fn descendants_and_self(
    roots: impl IntoIterator<Item = Pid>,
    children: &HashMap<Pid, Vec<Pid>>,
    include: impl Fn(Pid) -> bool,
) -> HashSet<Pid> {
    let mut found = HashSet::new();
    let mut queue: Vec<Pid> = roots.into_iter().collect();
    while let Some(pid) = queue.pop() {
        if found.contains(&pid) || !include(pid) {
            continue;
        }
        found.insert(pid);
        queue.extend(children.get(&pid).into_iter().flatten().copied());
    }
    found
}

/// Whether a process name is one of the [`STORE_CLIENTS`].
fn is_store_client(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    STORE_CLIENTS.contains(&name)
}

/// Whether an environment sets `WINEPREFIX` or `STEAM_COMPAT_DATA_PATH` to
/// `prefix`.
fn runs_in_prefix(environ: &[std::ffi::OsString], prefix: &Path) -> bool {
    let want = prefix.to_string_lossy();
    let want = want.trim_end_matches('/');
    environ.iter().any(|var| {
        let var = var.to_string_lossy();
        ["WINEPREFIX=", "STEAM_COMPAT_DATA_PATH="]
            .iter()
            .any(|key| {
                var.strip_prefix(key)
                    .is_some_and(|v| v.trim_end_matches('/') == want)
            })
    })
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

    #[cfg(not(windows))]
    #[test]
    fn handoff_watch_dir_prefers_the_install_folder_containing_the_exe() {
        let exe = Path::new("/games/Foo/Binaries/Win64/Foo.exe");
        assert_eq!(
            handoff_watch_dir(exe, Some(Path::new("/games/Foo"))),
            Some(PathBuf::from("/games/Foo"))
        );
        // Install folder that doesn't contain the exe → the exe's own folder.
        assert_eq!(
            handoff_watch_dir(exe, Some(Path::new("/games/Bar"))),
            Some(PathBuf::from("/games/Foo/Binaries/Win64"))
        );
        // A sibling whose name shares a prefix isn't "inside".
        assert_eq!(
            handoff_watch_dir(exe, Some(Path::new("/games/Fo"))),
            Some(PathBuf::from("/games/Foo/Binaries/Win64"))
        );
        assert_eq!(
            handoff_watch_dir(exe, None),
            Some(PathBuf::from("/games/Foo/Binaries/Win64"))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn handoff_watch_dir_never_watches_a_filesystem_root() {
        assert_eq!(handoff_watch_dir(Path::new("/game"), None), None);
        assert_eq!(
            handoff_watch_dir(Path::new("/Foo/game"), Some(Path::new("/"))),
            Some(PathBuf::from("/Foo"))
        );
        assert_eq!(handoff_watch_dir(Path::new("game"), None), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn find_process_in_dir_only_counts_processes_started_since_the_launch() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let exe = std::fs::read_link(format!("/proc/{}/exe", child.id())).unwrap();
        let dir = exe.parent().unwrap().to_path_buf();
        let launched = unix_secs_now();

        let mut sys = System::new();
        let found = find_process_in_dir(&mut sys, &dir, launched);
        // A launch in the future: everything running now predates it.
        let later = find_process_in_dir(&mut sys, &dir, launched + 3600);
        child.kill().unwrap();
        let _ = child.wait();

        assert!(found.is_some_and(|p| p.starts_with(&dir)));
        assert_eq!(later, None);
    }

    #[test]
    fn descendants_and_self_walks_the_whole_tree() {
        let pid = Pid::from_u32;
        // 1 → 2 → 3, 1 → 4; 5 is unrelated, and 6 is its child.
        let children = HashMap::from([
            (pid(1), vec![pid(2), pid(4)]),
            (pid(2), vec![pid(3)]),
            (pid(5), vec![pid(6)]),
        ]);
        let all = |_| true;
        assert_eq!(
            descendants_and_self([pid(1)], &children, all),
            HashSet::from([pid(1), pid(2), pid(3), pid(4)])
        );
        assert_eq!(
            descendants_and_self([pid(3)], &children, all),
            HashSet::from([pid(3)])
        );
        // Several roots, overlapping.
        assert_eq!(
            descendants_and_self([pid(2), pid(3), pid(5)], &children, all),
            HashSet::from([pid(2), pid(3), pid(5), pid(6)])
        );
    }

    #[test]
    fn descendants_and_self_skips_excluded_subtrees() {
        let pid = Pid::from_u32;
        // The game (1) started Steam (2), which started its web helper (3).
        let children = HashMap::from([(pid(1), vec![pid(2), pid(4)]), (pid(2), vec![pid(3)])]);
        assert_eq!(
            descendants_and_self([pid(1)], &children, |p| p != pid(2)),
            HashSet::from([pid(1), pid(4)])
        );
    }

    #[test]
    fn is_store_client_matches_names_with_or_without_exe() {
        assert!(is_store_client("steam.exe"));
        assert!(is_store_client("Steam.exe"));
        assert!(is_store_client("steamwebhelper"));
        assert!(is_store_client("EADesktop.exe"));
        assert!(is_store_client("Battle.net.exe"));
        assert!(!is_store_client("AlanWake.exe"));
        assert!(!is_store_client("steam_api64.exe"));
    }

    #[test]
    fn runs_in_prefix_matches_either_variable() {
        let env =
            |vars: &[&str]| -> Vec<std::ffi::OsString> { vars.iter().map(Into::into).collect() };
        let prefix = Path::new("/home/u/.local/share/Spool/prefixes/abc");
        assert!(runs_in_prefix(
            &env(&[
                "HOME=/home/u",
                "WINEPREFIX=/home/u/.local/share/Spool/prefixes/abc"
            ]),
            prefix
        ));
        assert!(runs_in_prefix(
            &env(&["STEAM_COMPAT_DATA_PATH=/home/u/.local/share/Spool/prefixes/abc/"]),
            prefix
        ));
        assert!(!runs_in_prefix(
            &env(&["WINEPREFIX=/home/u/.local/share/Spool/prefixes/abcd"]),
            prefix
        ));
        assert!(!runs_in_prefix(
            &env(&["OTHER=/home/u/.local/share/Spool/prefixes/abc"]),
            prefix
        ));
    }

    /// Launches a script that runs `prelude`, starts `sleep 60` in the
    /// background and waits, then stops it once the `sleep` is up. Asserts
    /// the launch returns as stopped and the `sleep` is gone.
    #[cfg(target_os = "linux")]
    async fn assert_stop_kills_tree(prelude: &str) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("child.pid");
        let script = dir.path().join("game.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n{prelude}\nsleep 60 &\necho $! > '{}'\nwait\n",
                pid_file.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let stop = StopCtl::default();
        let options = LaunchOptions::default();
        let run = run_game_stoppable(
            &script,
            LaunchSpec::Native {
                run_as_admin: false,
                options: &options,
                install_dir: Some(dir.path()),
            },
            &stop,
        );
        let trigger = async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            let child_pid = loop {
                if let Some(pid) = std::fs::read_to_string(&pid_file)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                {
                    break pid;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "script never started"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            };
            stop.token.cancel();
            child_pid
        };
        let (result, child_pid) =
            tokio::join!(tokio::time::timeout(Duration::from_secs(30), run), trigger);
        let result = result.expect("stop timed out").unwrap();
        assert!(result.stopped);
        assert_pid_gone(child_pid);
    }

    /// Waits up to 5 s for `pid` to exit. An orphaned process is reaped by
    /// init once it dies, so a zombie counts as gone.
    #[cfg(target_os = "linux")]
    fn assert_pid_gone(pid: u32) {
        let gone = || {
            std::fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |s| {
                s.rsplit(')')
                    .next()
                    .is_some_and(|r| r.trim_start().starts_with('Z'))
            })
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !gone() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(gone(), "process {pid} survived the stop");
    }

    /// A Proton launch whose spawned process exits while a process it started
    /// keeps the output pipes open (a launch-option wrapper, here a fake
    /// umu-run) can still be stopped: the process is found by its
    /// `WINEPREFIX` and killed, and the launch returns as stopped.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn stop_works_while_waiting_on_proton_output() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().join("prefix");
        let pid_file = dir.path().join("child.pid");
        let umu_run = dir.path().join("umu-run");
        std::fs::write(
            &umu_run,
            format!(
                "#!/bin/sh\nsleep 60 &\necho $! > '{}'\n",
                pid_file.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&umu_run, std::fs::Permissions::from_mode(0o755)).unwrap();
        let game_exe = dir.path().join("Game.exe");

        let stop = StopCtl::default();
        let run = run_game_stoppable(
            &game_exe,
            LaunchSpec::Proton {
                umu_run: &umu_run,
                prefix_root: &prefix,
                proton_path: None,
                game_id: "stop-test",
                extra_args: &[],
                extra_env: &[],
                wrapper: &[],
                on_setup: None,
            },
            &stop,
        );
        let trigger = async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            let child_pid = loop {
                if let Some(pid) = std::fs::read_to_string(&pid_file)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                {
                    break pid;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "script never started"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            };
            // Let the fake umu-run exit, so the launch is waiting on the
            // output relays alone.
            tokio::time::sleep(Duration::from_millis(500)).await;
            stop.token.cancel();
            child_pid
        };
        let (result, child_pid) =
            tokio::join!(tokio::time::timeout(Duration::from_secs(30), run), trigger);
        let result = result.expect("stop timed out").unwrap();
        assert!(result.stopped);
        assert_pid_gone(child_pid);
    }

    /// Stopping a native launch kills the spawned process and the child it
    /// started.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn stop_kills_the_spawned_process_tree() {
        assert_stop_kills_tree("").await;
    }

    /// Processes that ignore SIGTERM are killed after the grace period. An
    /// ignored signal stays ignored across exec, so the `sleep` ignores it too.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn stop_kills_processes_that_ignore_sigterm() {
        assert_stop_kills_tree("trap '' TERM").await;
    }

    #[cfg(windows)]
    #[test]
    fn exe_in_dir_ignores_case_and_separator_style_on_windows() {
        assert!(exe_in_dir(
            Path::new(r"E:\Spool\Alan Wake Remastered\Game_f_x64_EOS.exe"),
            Path::new("e:/spool/alan wake remastered/"),
        ));
        assert!(!exe_in_dir(
            Path::new(r"E:\Spool\Alan Wake Remastered 2\Game.exe"),
            Path::new(r"E:\Spool\Alan Wake Remastered"),
        ));
    }

    #[test]
    fn umu_setup_status_matches_real_umu_output() {
        // Lines captured from umu-run 1.4.4 on a fresh install.
        let status = |l| umu_setup_status(l);
        assert_eq!(
            status("INFO: Downloading UMU-Proton-10.0-4.tar.gz..."),
            Some("Downloading Proton…")
        );
        assert_eq!(
            status("INFO: Downloading steamrt3 (3.0.20261021.266025), please wait..."),
            Some("Downloading Steam Linux Runtime…")
        );
        assert_eq!(
            status("INFO: Extracting UMU-Proton-10.0-4.tar.gz..."),
            Some("Unpacking Proton…")
        );
        assert_eq!(
            status("INFO: Verifying integrity of sniper_platform_3.0.20261021.266025..."),
            Some("Verifying Steam Linux Runtime…")
        );
        assert_eq!(status("INFO: UMU-Proton-10.0-4.tar.gz: SHA512 is OK"), None);
        assert_eq!(status("INFO: Using UMU-Proton-10.0-4"), None);
        // A game or Wine line that only mentions a download isn't umu setup.
        assert_eq!(status("fixme: downloading patch list"), None);
    }

    type Events = Arc<Mutex<Vec<Option<String>>>>;

    fn recording_tracker(settle: Duration) -> (SetupTracker, Events) {
        let events: Events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let cb: SetupCallback = Arc::new(move |m| sink.lock().unwrap().push(m.map(String::from)));
        (SetupTracker::with_settle(Some(cb), settle), events)
    }

    const TEST_SETTLE: Duration = Duration::from_millis(40);

    async fn settled() {
        tokio::time::sleep(TEST_SETTLE * 4).await;
    }

    fn snapshot(events: &Events) -> Vec<Option<String>> {
        events.lock().unwrap().clone()
    }

    #[tokio::test(start_paused = true)]
    async fn setup_tracker_reports_consecutive_steps_as_one_setup() {
        // Lines captured from umu-run 1.4.4 on a fresh install.
        let (t, events) = recording_tracker(TEST_SETTLE);
        for line in [
            "INFO: umu-launcher version 1.4.4",
            "INFO: Downloading UMU-Proton-10.0-4.sha512sum...",
            "INFO: Downloading UMU-Proton-10.0-4.tar.gz...",
            "INFO: UMU-Proton-10.0-4.tar.gz: SHA512 is OK",
            "INFO: Extracting UMU-Proton-10.0-4.tar.gz...",
            "INFO: Using UMU-Proton-10.0-4",
            "INFO: Downloading steamrt3 (3.0.20261021.266025), please wait...",
            "INFO: Running 'UMU-Proton-10.0-4' using runtime 'sniper'",
        ] {
            t.observe(line);
        }
        settled().await;
        // One message per distinct step, and a single finish: no `playing`
        // between steps, and the checksum + archive downloads read as one.
        assert_eq!(
            snapshot(&events),
            [
                Some("Downloading Proton…".to_string()),
                Some("Unpacking Proton…".to_string()),
                Some("Downloading Steam Linux Runtime…".to_string()),
                None,
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn setup_tracker_stays_quiet_when_umu_has_nothing_to_set_up() {
        let (t, events) = recording_tracker(TEST_SETTLE);
        t.observe("INFO: umu-launcher version 1.4.4");
        t.observe("INFO: Using UMU-Proton-10.0-4");
        settled().await;
        t.finish();
        assert!(snapshot(&events).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn setup_tracker_finish_clears_a_setup_cut_off_by_exit() {
        let (t, events) = recording_tracker(TEST_SETTLE);
        t.observe("INFO: Downloading UMU-Proton-10.0-4.tar.gz...");
        t.finish();
        t.finish();
        assert_eq!(
            snapshot(&events),
            [Some("Downloading Proton…".to_string()), None]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn setup_tracker_waits_for_umu_to_go_quiet() {
        let (t, events) = recording_tracker(TEST_SETTLE);
        t.observe("INFO: Downloading UMU-Proton-10.0-4.tar.gz...");
        t.observe("INFO: some other line"); // starts the settle timer
                                            // Output keeps arriving, each line inside the settle window, so the
                                            // deadline keeps moving and setup never reads as finished.
        for _ in 0..5 {
            tokio::time::sleep(TEST_SETTLE / 2).await;
            t.observe("progress");
        }
        assert_eq!(snapshot(&events), [Some("Downloading Proton…".to_string())]);
        settled().await;
        assert_eq!(
            snapshot(&events),
            [Some("Downloading Proton…".to_string()), None]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn setup_tracker_finish_cancels_a_pending_settle() {
        let (t, events) = recording_tracker(TEST_SETTLE);
        t.observe("INFO: Downloading UMU-Proton-10.0-4.tar.gz...");
        t.observe("INFO: Using UMU-Proton-10.0-4"); // starts the settle timer
        t.finish();
        settled().await;
        assert_eq!(
            snapshot(&events),
            [Some("Downloading Proton…".to_string()), None],
            "the timer must not report a second finish after finish()"
        );
    }

    #[tokio::test]
    async fn relay_observes_carriage_return_progress_and_lines_past_the_log_cap() {
        let mut input = b"Downloading Proton\r 10%\r 50%\rdone\n".to_vec();
        for _ in 0..UMU_RELAY_LINE_CAP + 5 {
            input.extend_from_slice(b"noise\n");
        }
        input.extend_from_slice(b"Extracting late\n");

        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&seen);
        relay_umu_stream(
            input.as_slice(),
            "stdout",
            None,
            |_| {},
            move |p| {
                sink.lock().unwrap().push(p.to_string());
            },
        )
        .await;

        let seen = seen.lock().unwrap();
        // Pieces end at `\r` as well as `\n`: the first arrives before any newline.
        assert_eq!(seen[..4], ["Downloading Proton", " 10%", " 50%", "done"]);
        assert_eq!(seen.last().map(String::as_str), Some("Extracting late"));
    }

    fn argv(cmd: &Command) -> Vec<String> {
        let std = cmd.as_std();
        std::iter::once(std.get_program())
            .chain(std.get_args())
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn wrapped_command_puts_the_wrapper_in_front() {
        let wrapper = vec!["/bin/wrap".to_string(), "--flag".to_string()];
        let cmd = wrapped_command(&wrapper, Path::new("/g/game"), &["-dx11"]);
        assert_eq!(argv(&cmd), ["/bin/wrap", "--flag", "/g/game", "-dx11"]);

        let cmd = wrapped_command(&[], Path::new("/g/game"), &["-dx11"]);
        assert_eq!(argv(&cmd), ["/g/game", "-dx11"]);
    }

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
        relay_umu_stream(
            blob.as_slice(),
            "stderr",
            Some(Arc::clone(&tail)),
            move |line| {
                sink.lock().unwrap().push(line.to_string());
            },
            |_| {},
        )
        .await;

        assert!(
            relayed.lock().unwrap().is_empty(),
            "oversized line never relayed"
        );
        assert!(
            tail.lock().unwrap().is_empty(),
            "oversized line never buffered"
        );
    }

    #[tokio::test]
    async fn relay_resyncs_to_normal_lines_after_an_oversized_one() {
        let mut input = b"before\n".to_vec();
        input.resize(input.len() + UMU_RELAY_LINE_MAX_BYTES * 2, b'x');
        input.push(b'\n');
        input.extend_from_slice(b"after\n");

        let relayed = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&relayed);
        relay_umu_stream(
            input.as_slice(),
            "stdout",
            None,
            move |line| {
                sink.lock().unwrap().push(line.to_string());
            },
            |_| {},
        )
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

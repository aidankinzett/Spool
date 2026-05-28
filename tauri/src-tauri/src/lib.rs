//! Crate root for the Spool Tauri backend.
//!
//! ## Tray-resident lifecycle
//!
//! Spool runs as a single long-lived process (the cassette deck stays in
//! the dock). The library window is one *view* on that process; closing
//! it hides to the tray rather than quitting the app. Secondary `spool`
//! invocations from Steam shortcuts / Armoury Crate launchers are
//! intercepted by `tauri-plugin-single-instance` and forwarded as argv
//! to the running primary — no cold-start cost on game launch.
//!
//! Quit is **only** via the tray menu's "Quit Spool" item, which calls
//! `app.exit(0)`. Window close + `RunEvent::ExitRequested` are both
//! prevented otherwise.
//!
//! ## Modules
//!   - [`error`]       — unified error type used across the backend
//!   - [`paths`]       — single source of truth for filesystem locations
//!   - [`config`]      — app settings: persistence, identity, ludusavi auto-detect
//!   - [`library`]     — the game library: data model, persistence, commands
//!   - [`ludusavi`]    — CLI subprocess + manifest cache + search/enrich
//!   - [`steamgriddb`] — cover art fetch
//!   - [`process`]     — game process spawn
//!   - [`runner`]      — run workflow state machine
//!   - [`cli`]         — argv parsing for `--run` mode
//!   - [`steamgriddb`] — cover art fetch

mod cli;
mod config;
mod error;
mod lan;
mod library;
mod ludusavi;
mod paths;
mod process;
mod runner;
mod steam;
mod steamgriddb;

use cli::CliMode;
use config::{Config, SharedConfig};
use lan::{LanDownloadState, LanState};
use library::{Library, SharedLibrary};
use ludusavi::LudusaviClient;
use runner::RunState;
use std::sync::Mutex;
use steamgriddb::SteamGridDbClient;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent, State, WindowEvent,
};

/// Holds a game id queued for launch at startup. The cold-start path
/// for `spool --run "Name" "Exe"` writes here; the frontend's library
/// page reads + clears it via `take_pending_run` after its event
/// listeners are wired up.
#[derive(Default)]
pub struct PendingRun {
    inner: Mutex<Option<String>>,
}

impl PendingRun {
    fn set(&self, id: String) {
        if let Ok(mut g) = self.inner.lock() {
            *g = Some(id);
        }
    }
    fn take(&self) -> Option<String> {
        self.inner.lock().ok().and_then(|mut g| g.take())
    }
}

#[tauri::command]
fn take_pending_run(state: State<'_, PendingRun>) -> Option<String> {
    state.take()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing first — everything below logs through it. The
    // worker guard is bound to the function frame so background log
    // writes flush before the process exits.
    let _log_guard = init_tracing();
    tracing::info!("spool starting up");

    // Load persistent state synchronously — both files are small.
    let library = Library::load().unwrap_or_else(|err| {
        tracing::warn!(error = %err, "failed to load library, starting empty");
        Library::default()
    });
    let config = Config::load().unwrap_or_else(|err| {
        tracing::warn!(error = %err, "failed to load config, starting with defaults");
        Config::default()
    });

    // Pre-resolve the startup CLI mode so the single-instance plugin
    // can install before we touch state.
    let initial_args: Vec<String> = std::env::args().collect();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // Single-instance: secondary `spool` invocations land here. We
        // dispatch on argv to either focus the library or kick off a
        // game launch. Must come early — adds the IPC channel.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_forwarded_launch(app, &argv);
        }))
        .manage::<SharedLibrary>(Mutex::new(library))
        .manage::<SharedConfig>(Mutex::new(config))
        .manage::<LudusaviClient>(LudusaviClient::new())
        .manage::<SteamGridDbClient>(SteamGridDbClient::new())
        .manage::<RunState>(RunState::default())
        .manage::<PendingRun>(PendingRun::default())
        .manage::<LanState>(LanState::new())
        .manage::<LanDownloadState>(LanDownloadState::default())
        .invoke_handler(tauri::generate_handler![
            take_pending_run,
            // library
            library::list_games,
            library::add_game,
            library::update_game,
            library::remove_game,
            // config
            config::get_config,
            config::update_config,
            config::detect_ludusavi,
            // ludusavi
            ludusavi::search_games,
            ludusavi::search_by_exe,
            ludusavi::open_ludusavi_gui,
            // steamgriddb
            steamgriddb::fetch_cover,
            // steam shortcut
            steam::add_to_steam,
            // lan discovery
            lan::list_lan_peers,
            lan::fetch_peer_games,
            lan::start_peer_install,
            lan::current_peer_download,
            // runner
            runner::launch_game,
        ])
        .setup(move |app| {
            // Mount tray icon + menu.
            mount_tray(app.handle())?;

            // Intercept the main window's close button → hide instead.
            // First time the user does this, emit `tray:first-hide` so the
            // library page can surface a sticky info toast explaining where
            // Spool went. The toast survives the hide (it's in component
            // state) so the user sees it next time they re-open the window.
            if let Some(main) = app.get_webview_window("main") {
                let win = main.clone();
                let app_handle = app.handle().clone();
                main.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                        emit_tray_intro_once(&app_handle);
                    }
                });
            }

            // Kick off LAN peer discovery in the background. Logs and
            // skips if the socket can't bind (port in use, firewall, etc.)
            // — peer count stays at 0 in that case but everything else
            // keeps working.
            lan::spawn_discovery(app.handle().clone());

            // Startup --run dispatch: queue the game id so the frontend
            // can pick it up once its listeners are ready.
            if let CliMode::Run { ref game_name, .. } = cli::parse_args(&initial_args) {
                let library = app.state::<SharedLibrary>();
                let pending = app.state::<PendingRun>();
                if let Some(id) = find_game_id_by_name(&library, game_name) {
                    pending.set(id);
                } else {
                    tracing::warn!(name = %game_name, "startup --run: no library entry matches");
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Run with an exit-event interceptor so the app stays alive when
    // the last window closes (it's a tray app — only "Quit" exits).
    app.run(|_app_handle, event| {
        if let RunEvent::ExitRequested { api, code, .. } = &event {
            // code is `Some(_)` when we explicitly called `app.exit()`
            // — let that through. Otherwise (last-window-closed), block.
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}

/// Builds the tray icon + context menu and registers click handlers.
fn mount_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, "tray:show", "Show Spool", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "tray:quit", "Quit Spool", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    let _tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Spool")
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or("missing default window icon")?,
        )
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray:show" => show_library(app),
            "tray:quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left-click = toggle library; right-click is reserved for
            // the OS-rendered context menu.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_library(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_library(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn toggle_library(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        match win.is_visible() {
            Ok(true) => {
                let _ = win.hide();
            }
            _ => {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }
    }
}

/// Looks up a game by exact `game_name` match. Returns the entry id.
fn find_game_id_by_name(library: &SharedLibrary, name: &str) -> Option<String> {
    let lib = library.lock().ok()?;
    lib.entries
        .iter()
        .find(|e| e.game_name == name)
        .map(|e| e.id.clone())
}

/// Initialises the global `tracing` subscriber. Two layers:
///
///   * **stderr** — what `cargo run` / `tauri dev` show in the terminal
///   * **file**   — appended to `%LOCALAPPDATA%\Spool\debug.log`, the same
///                  path the C# app used, so existing support workflows
///                  ("send me your debug.log") still work.
///
/// Default verbosity: `info`, with the noisy crates (tauri / hyper /
/// reqwest / h2) clamped to `warn`. Override with `SPOOL_LOG=debug` (or
/// any standard `EnvFilter` spec) to widen.
///
/// Returns the non-blocking worker guard; binding it to the call frame
/// keeps the writer alive and flushes buffered lines at shutdown.
fn init_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let log_dir = paths::app_data_dir();
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::never(&log_dir, "debug.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_env("SPOOL_LOG").unwrap_or_else(|_| {
        EnvFilter::new("info,tauri=warn,h2=warn,hyper=warn,hyper_util=warn,reqwest=warn,rustls=warn")
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(false).with_writer(std::io::stderr))
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_target(true)
                .with_writer(file_writer),
        )
        .init();

    guard
}

/// Fires `tray:first-hide` the first time the user hides Spool to the
/// tray, then marks the flag in Config so it never fires again. No-op on
/// subsequent hides. All-or-nothing — if either the flag read or the save
/// fails we just skip the event (the worst case is the user never sees
/// the intro, which is a minor regression, not a crash).
fn emit_tray_intro_once(app: &AppHandle) {
    let config = app.state::<SharedConfig>();
    let needs_intro = match config.lock() {
        Ok(cfg) => !cfg.data.tray_intro_seen,
        Err(_) => false,
    };
    if !needs_intro {
        return;
    }
    if let Ok(mut cfg) = config.lock() {
        cfg.data.tray_intro_seen = true;
        if cfg.save().is_err() {
            // Save failed — bail without emitting so we'll try again next
            // close (rather than emitting now and never marking seen).
            return;
        }
    }
    if let Err(e) = app.emit("tray:first-hide", &()) {
        tracing::warn!(error = %e, "failed to emit tray:first-hide");
    }
}

/// Dispatches a forwarded secondary-launch's argv. Either focuses the
/// library (no args) or queues a game launch (`--run "Name" "Exe"`).
fn handle_forwarded_launch(app: &AppHandle, argv: &[String]) {
    match cli::parse_args(argv) {
        CliMode::Run { game_name, .. } => {
            show_library(app); // bring the window up so the user sees the workflow run
            let Some(id) = find_game_id_by_name(&app.state::<SharedLibrary>(), &game_name) else {
                tracing::warn!(name = %game_name, "forwarded --run: no library entry matches");
                return;
            };
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = runner::launch_game_inner(&app_clone, &id).await {
                    tracing::error!(error = %e, "forwarded --run workflow failed");
                }
            });
        }
        CliMode::Normal => show_library(app),
    }
}


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

mod accent_backfill;
mod browse_download;
mod cli;
mod decky_install;
mod diagnostics;
mod config;
mod error;
mod gamemode;
mod hydra;
mod lan;
mod launcher;
mod library;
mod ludusavi;
mod ludusavi_config;
mod paths;
mod process;
mod proton;
mod redirects;
mod registry;
mod runner;
mod session;
mod size_backfill;
mod steam;
mod steamgriddb;
mod sync;
mod torbox;

use cli::CliMode;
use config::{Config, SharedConfig};
use lan::{LanDownloadState, LanServerShutdown, LanState, LanUploadsState};
use sync::SyncStatusState;
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

/// Readiness gate for the attached Game-Mode splash. The attached `--run`
/// workflow emits `run:phase` events the moment it starts, so it must not
/// begin until the splash window has wired up its `run:phase` listener —
/// otherwise the early phases (restoring → launching → playing) fire into
/// the void and the splash stays stuck on its default "Restoring saves…"
/// label for the whole session. The splash calls `notify_splash_ready`
/// once its listener is registered; the workflow task waits on this
/// (with a timeout fallback so a webview that never loads can't hang the
/// launch). Mirrors the `PendingRun` handshake the main library window
/// uses for the desktop `--run` path.
#[derive(Default)]
pub struct SplashReady {
    notify: tokio::sync::Notify,
}

#[tauri::command]
fn notify_splash_ready(state: State<'_, SplashReady>) {
    state.notify.notify_one();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── Linux WebKitGTK rendering workaround ──────────────────────────────
    // WebKitGTK's GPU-accelerated compositing + DMA-BUF renderer fail to
    // initialise on many Linux GPU/compositor combos (AMD/radeonsi + Mesa on
    // Wayland here; also common on NVIDIA), leaving a black window with no
    // error. The ecosystem-standard fix is to disable both paths *before*
    // the webview initialises. Set in-process (not via the launch env) so it
    // works from the desktop entry, a terminal, and the AppImage alike.
    //
    // Only set when the user hasn't already chosen a value, so power users can
    // still opt back into the GPU path. These are WebKit-specific and harmless
    // to the umu-run/Proton children (which also strip GDK_* in process.rs).
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
    }

    // Initialize tracing first — everything below logs through it. The
    // worker guard is bound to the function frame so background log
    // writes flush before the process exits.
    let _log_guard = init_tracing();
    tracing::info!("spool starting up");

    // Headless one-shot backup (Decky plugin forced-close fallback). No GUI.
    let initial_args: Vec<String> = std::env::args().collect();
    if let CliMode::Backup { ref game_name } = cli::parse_args(&initial_args) {
        std::process::exit(run_backup_headless(game_name));
    }

    // One-shot migration: pull `%LOCALAPPDATA%\ludusavi-wrap\` data
    // into the new Spool dir on first run. No-op if already migrated,
    // if there's no legacy dir, or if Spool already has a library.
    paths::migrate_from_ludusavi_wrap();

    // When running as an AppImage, refresh the stable launcher wrapper so
    // Steam shortcuts / Armoury stubs (which point at the wrapper, not the
    // version-stamped .AppImage) exec the current AppImage. Self-heals after
    // an update relocates the file. No-op on native installs.
    let _ = paths::refresh_appimage_launcher();

    // Load persistent state synchronously — both files are small.
    let library = Library::load().unwrap_or_else(|err| {
        tracing::warn!(error = %err, "failed to load library, starting empty");
        Library::default()
    });
    let config = Config::load().unwrap_or_else(|err| {
        tracing::warn!(error = %err, "failed to load config, starting with defaults");
        Config::default()
    });

    // Decide whether this is an attached Game-Mode launch: `spool --run`
    // inside a SteamOS gamescope session. If so, we skip the tray,
    // single-instance plugin, and background pollers, run the game, then
    // exit — so Steam sees the game stop when Spool does.
    let cli_mode = cli::parse_args(&initial_args);
    let attached = matches!(cli_mode, CliMode::Run { .. }) && gamemode::is_steam_game_mode();
    if attached {
        tracing::info!("attached launch mode (SteamOS Game Mode) — no tray, exit on game close");
    }

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // Native OS toast notifications. Used by the run workflow to
        // tell the user "Saves backed up" / "Save restore failed"
        // while Spool itself is hidden in the tray during gameplay.
        .plugin(tauri_plugin_notification::init())
        // Auto-update via Tauri's updater. Polls a signed JSON
        // manifest (URL configured in tauri.conf.json), verifies the
        // ed25519 signature, then runs the NSIS installer silently.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Process control — used by the updater UI to relaunch Spool
        // after an update installs. On Windows the NSIS installer
        // relaunches us itself, but on the Linux AppImage the updater
        // only swaps the file in place, so we must restart explicitly.
        .plugin(tauri_plugin_process::init());
    if !attached {
        // Single-instance: secondary `spool` invocations land here. We
        // dispatch on argv to either focus the library or kick off a
        // game launch. Must come early — adds the IPC channel.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_forwarded_launch(app, &argv);
        }));
    }
    let app = builder
        .manage::<SharedLibrary>(Mutex::new(library))
        .manage::<SharedConfig>(Mutex::new(config))
        .manage::<LudusaviClient>(LudusaviClient::new())
        .manage::<SteamGridDbClient>(SteamGridDbClient::new())
        .manage::<RunState>(RunState::default())
        .manage::<PendingRun>(PendingRun::default())
        .manage::<SplashReady>(SplashReady::default())
        .manage::<LanState>(LanState::new())
        // Single reqwest::Client shared across LAN code — reqwest is
        // designed for reuse (connection pooling, DNS cache). Per
        // `domain-web` best practice + `m07-concurrency`'s "avoid
        // per-call allocations in hot paths", every LAN call gets
        // this client via `app.state::<reqwest::Client>()` and uses
        // RequestBuilder::timeout for per-request limits.
        .manage::<reqwest::Client>(
            reqwest::Client::builder()
                .build()
                .expect("reqwest client build"),
        )
        .manage::<LanDownloadState>(LanDownloadState::default())
        .manage::<LanUploadsState>(LanUploadsState::default())
        .manage::<LanServerShutdown>(LanServerShutdown::default())
        .manage::<SyncStatusState>(SyncStatusState::default())
        .manage::<browse_download::BrowseDownloadState>(
            browse_download::BrowseDownloadState::default(),
        )
        .invoke_handler(tauri::generate_handler![
            take_pending_run,
            notify_splash_ready,
            // library
            library::list_games,
            library::add_game,
            library::update_game,
            library::remove_game,
            // config
            config::get_config,
            config::update_config,
            config::detect_ludusavi,
            config::detect_umu_run,
            config::app_platform,
            diagnostics::check_dependencies,
            // proton / linux launch
            proton::list_proton_versions,
            proton::install_proton_deps,
            // ludusavi
            ludusavi::search_games,
            ludusavi::search_by_exe,
            ludusavi::open_ludusavi_gui,
            ludusavi::set_cloud_webdav,
            // steamgriddb
            steamgriddb::fetch_cover,
            // steam shortcut
            steam::add_to_steam,
            // armoury crate launcher
            launcher::generate_armoury_launcher,
            // registry compat-flag probe
            registry::get_run_as_admin_in_registry,
            // sync server
            sync::current_sync_status,
            sync::refresh_sync_status,
            sync::sync_register_account,
            sync::use_server_save_storage,
            // torbox
            torbox::torbox_add_magnet,
            torbox::torbox_torrent_info,
            torbox::torbox_request_download_link,
            torbox::torbox_ping,
            // hydra feeds (Browse Games)
            hydra::hydra_fetch_all,
            hydra::hydra_add_source,
            hydra::hydra_remove_source,
            // browse-games download orchestrator
            browse_download::start_browse_download,
            browse_download::cancel_browse_download,
            browse_download::current_browse_download,
            // lan discovery
            lan::discovery::list_lan_peers,
            lan::install::fetch_peer_games,
            lan::install::start_peer_install,
            lan::install::current_peer_download,
            lan::install::cancel_peer_install,
            lan::server::list_active_uploads,
            lan::server::cancel_upload,
            // runner
            runner::launch_game,
            runner::manual_backup,
            runner::manual_restore,
            // decky plugin installer (Linux / SteamOS)
            decky_install::decky_plugin_status,
            decky_install::install_decky_plugin,
        ])
        .setup(move |app| {
            if attached {
                // ── Attached Game-Mode launch ────────────────────────────
                // No tray, no pollers, no library window. Show a splash,
                // launch the game from Rust, exit when the workflow ends.
                let CliMode::Run { game_name, .. } = cli::parse_args(&initial_args) else {
                    app.handle().exit(1);
                    return Ok(());
                };
                let Some(id) = find_game_id_by_name(&app.state::<SharedLibrary>(), &game_name)
                else {
                    tracing::error!(name = %game_name, "attached --run: no library entry matches");
                    app.handle().exit(1);
                    return Ok(());
                };

                // Write the session record (appid matches the Steam shortcut).
                if let Some(exe) = paths::spool_executable() {
                    let appid =
                        session::compute_steam_appid(&exe.to_string_lossy(), &game_name);
                    if let Err(e) = session::write_start(&game_name, appid) {
                        tracing::warn!(error = %e, "failed to write active-session record");
                    }
                }

                // Make sure ludusavi config exists before the workflow runs.
                if let Err(e) = ludusavi_config::ensure_config() {
                    tracing::warn!(error = %e, "failed to initialise ludusavi config dir");
                }

                // Splash window (the `main` window stays hidden / unused).
                if let Err(e) = tauri::WebviewWindowBuilder::new(
                    app,
                    "splash",
                    tauri::WebviewUrl::App("splash".into()),
                )
                .title("Spool")
                .decorations(false)
                .inner_size(520.0, 260.0)
                .center()
                .resizable(false)
                .build()
                {
                    tracing::warn!(error = %e, "failed to create splash window");
                }

                // Launch + exit when done. app.exit(0) lets Steam see the
                // game stop (RunEvent::ExitRequested only blocks code.is_none()).
                //
                // Wait for the splash to wire its `run:phase` listener before
                // starting the workflow — otherwise the restoring/launching/
                // playing phases fire before the webview is listening and the
                // splash sits on its default "Restoring saves…" label for the
                // whole session. A timeout fallback keeps a webview that never
                // loads from blocking the launch entirely.
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    {
                        let ready = app_handle.state::<SplashReady>();
                        let _ = tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            ready.notify.notified(),
                        )
                        .await;
                    }
                    if let Err(e) = runner::launch_game_inner(&app_handle, &id).await {
                        tracing::error!(error = %e, "attached --run workflow failed");
                    }
                    app_handle.exit(0);
                });

                return Ok(());
            }

            // ── Normal tray-resident startup (unchanged behavior) ────────

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
                // `main` is now created hidden — show it explicitly (also
                // removes the startup white-flash).
                let _ = main.show();
                main.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                        emit_tray_intro_once(&app_handle);
                    }
                });
            }

            // Ensure Spool's owned ludusavi config dir + config.yaml exist and
            // meet the required invariants (backup path, manifest enabled,
            // simple format). Idempotent — fast no-op on subsequent launches.
            if let Err(e) = ludusavi_config::ensure_config() {
                tracing::warn!(error = %e, "failed to initialise ludusavi config dir");
            }

            // Re-stamp the rclone binary path on every startup. Spool ships
            // rclone as an AppImage sidecar, and on Linux that resolves to a
            // path inside the AppImage's FUSE mount (`/tmp/.mount_Spool_XXXX`)
            // whose name is randomised per launch. A path persisted on a prior
            // run is dead today, which makes `ludusavi backup --cloud-sync`
            // fail silently (`cloudSyncFailed`). Resolving + rewriting it here
            // keeps `apps.rclone.path` pointing at a binary that exists this
            // session, regardless of which mount we landed on.
            {
                let configured_rclone = app
                    .state::<SharedConfig>()
                    .lock()
                    .ok()
                    .map(|g| g.data.rclone_path.clone())
                    .unwrap_or_default();
                if let Some(rclone) = paths::resolve_rclone_path(&configured_rclone) {
                    if let Err(e) = ludusavi_config::set_cloud(
                        None,
                        None,
                        None,
                        Some(&rclone.to_string_lossy()),
                        None,
                    ) {
                        tracing::warn!(error = %e, "failed to re-stamp rclone path at startup");
                    }
                }
            }

            // If the user opted into the turnkey self-hosted save store, refresh
            // its WebDAV credentials on boot. The sync server's API key doubles
            // as the WebDAV password; if it rotates (or the stored remote drifts
            // out of sync), every cloud sync 401s. Re-fetching `/storage` and
            // re-applying the remote keeps it authenticated — and as a bonus
            // re-stamps the rclone path via the same path as Settings. Spawned
            // off the setup thread since it makes a network call.
            {
                let provider = app
                    .state::<SharedConfig>()
                    .lock()
                    .ok()
                    .map(|g| g.data.cloud_provider.clone())
                    .unwrap_or_default();
                if provider == "spool-server" {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = sync::use_server_save_storage(handle).await {
                            tracing::warn!(error = %e, "startup: failed to refresh self-hosted save storage credentials");
                        } else {
                            tracing::info!("startup: refreshed self-hosted save storage credentials");
                        }
                    });
                }
            }

            // Kick off LAN peer discovery in the background. Logs and
            // skips if the socket can't bind (port in use, firewall, etc.)
            // — peer count stays at 0 in that case but everything else
            // keeps working.
            lan::spawn_discovery(app.handle().clone());

            // Backfill accent colours for any legacy entries that have
            // a cover but no extracted accent yet. Cheap no-op when
            // every entry is already filled.
            accent_backfill::spawn_backfill(app.handle().clone());

            // Backfill install sizes for entries that have a folder on
            // disk but no recorded size — legacy C# library entries land
            // here with `install_size_mb: 0`. Walks the folder, sums file
            // sizes, saves once at the end.
            size_backfill::spawn_backfill(app.handle().clone());

            // Sync server health poll. Runs forever, every 30s — emits
            // `sync:status-changed` so the chrome cloud icon can tint
            // itself. No-op (Unconfigured) when the user hasn't set
            // a server URL / API key.
            sync::spawn_health_poller(app.handle().clone());

            // One-shot cross-device merge: pull last-played, playtime
            // and latest-backup events, fold into the library. Runs
            // ~4 s after launch so the health poll has a chance to
            // confirm reachability first.
            sync::spawn_startup_sync(app.handle().clone());

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

/// Headless one-shot backup: load config + library, run ludusavi backup for
/// the named game, mark the session record, then return a process exit code.
/// No GUI / tray / single-instance. Used by `spool --backup "Name"` (the
/// Decky plugin's forced-close fallback).
fn run_backup_headless(game_name: &str) -> i32 {
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "--backup: failed to load config");
            return 1;
        }
    };
    let library = match Library::load() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "--backup: failed to load library");
            return 1;
        }
    };
    let Some(game_id) = library
        .entries
        .iter()
        .find(|e| e.game_name == game_name)
        .map(|e| e.id.clone())
    else {
        tracing::error!(name = %game_name, "--backup: no library entry matches");
        return 1;
    };
    let Some(ludusavi_exe) = paths::resolve_ludusavi_path(&config.data.ludusavi_path) else {
        tracing::error!("--backup: ludusavi not configured");
        return 1;
    };

    // Make sure Spool's ludusavi config (backup path, cloud remote) exists.
    if let Err(e) = ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "--backup: ensure_config failed");
    }

    let config_dir = paths::ludusavi_config_dir();
    let lib_state: SharedLibrary = Mutex::new(library);
    let client = LudusaviClient::new();

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "--backup: failed to start tokio runtime");
            return 1;
        }
    };
    let result = rt.block_on(async {
        runner::backup_game_core(&client, &ludusavi_exe, &config_dir, &lib_state, &game_id).await
    });

    match result {
        Ok(r) => {
            tracing::info!(game_name, games = r.game_count, "--backup complete");
            session::mark_backed_up();
            0
        }
        Err(e) => {
            tracing::error!(error = %e, "--backup failed");
            1
        }
    }
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
            "tray:quit" => quit_with_graceful_drain(app),
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

/// Triggers a clean shutdown: signals the LAN HTTP server to stop
/// accepting new connections, waits for in-flight responses to drain
/// (bounded by `LanServerShutdown::shutdown`'s internal 2 s timeout),
/// then calls `app.exit(0)`. Spawned on the runtime so the menu
/// callback returns immediately.
fn quit_with_graceful_drain(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<LanServerShutdown>().shutdown().await;
        app.exit(0);
    });
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
///     path the C# app used, so existing support workflows
///     ("send me your debug.log") still work.
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
        CliMode::Backup { game_name } => {
            tracing::warn!(name = %game_name, "forwarded --backup: headless backup not yet implemented in forwarded-launch path");
        }
    }
}


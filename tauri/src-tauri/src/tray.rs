//! System-tray icon, context menu, and the window-visibility helpers it drives.
//!
//! Spool is tray-resident: closing the library window hides it rather than
//! quitting (see the crate-root lifecycle docs). The tray icon is the only
//! always-present affordance — left-click toggles the window, the context menu
//! offers Show / Quit, and "Quit Spool" is the sole path that actually exits.

use crate::config::SharedConfig;
use crate::lan::LanServerShutdown;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

/// Builds the tray icon + context menu and registers click handlers.
pub(crate) fn mount_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
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

pub(crate) fn show_library(app: &AppHandle) {
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

/// Fires `tray:first-hide` the first time the user hides Spool to the
/// tray, then marks the flag in Config so it never fires again. No-op on
/// subsequent hides. All-or-nothing — if either the flag read or the save
/// fails we just skip the event (the worst case is the user never sees
/// the intro, which is a minor regression, not a crash).
pub(crate) fn emit_tray_intro_once(app: &AppHandle) {
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

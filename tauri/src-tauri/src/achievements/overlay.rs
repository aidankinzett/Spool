//! Desktop achievement overlay: a small always-on-top window in the corner of
//! the primary monitor, shown for a few seconds per unlock.
//!
//! OS notifications aren't reliable for this: Windows Focus Assist holds them
//! back while a fullscreen game runs. The overlay window is borderless,
//! transparent, skipped in the taskbar, ignores the mouse, and is created
//! without focus and non-focusable (`WS_EX_NOACTIVATE` on Windows) so it never
//! takes input from the game. It draws over windowed and borderless-fullscreen
//! games; a game in true exclusive fullscreen bypasses the desktop compositor,
//! so nothing outside the game's own process can draw over it.
//!
//! Unlocks are queued and shown one after another. Each gets a fresh window
//! (label `achievement-toast-<n>`): a window that has been hidden and shown
//! again can be activated by the show, while a window created with
//! `focused(false)` is shown with `SW_SHOWNOACTIVATE`. The page
//! (`routes/achievement-toast`) fetches what to draw through
//! [`achievement_toast_payload`].

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, LogicalPosition, LogicalSize, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::mpsc;

/// Window labels start with this (also the capability + window-state filter).
pub const LABEL_PREFIX: &str = "achievement-toast";
/// Logical size of the overlay window.
const WIDTH: f64 = 380.0;
const HEIGHT: f64 = 96.0;
/// Gap from the screen edge.
const MARGIN: f64 = 24.0;
/// How long each toast stays up, including the page's slide-in/out.
const SHOW_FOR: Duration = Duration::from_millis(6000);
/// Pause between consecutive toasts.
const GAP: Duration = Duration::from_millis(400);
/// How long to wait for the page to load before starting the clock anyway.
const READY_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize)]
pub struct OverlayToast {
    pub display_name: String,
    pub description: String,
    pub game_name: String,
    /// Steam CDN URL or a local path (the page maps it to an asset URL).
    pub icon: Option<String>,
    /// How long the page should stay visible before sliding out, in ms.
    pub duration_ms: u64,
}

/// The toast the currently open overlay window draws.
static CURRENT: Mutex<Option<OverlayToast>> = Mutex::new(None);
static QUEUE: OnceLock<mpsc::UnboundedSender<OverlayToast>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
/// Signalled when the page fetches its payload. [`SHOW_FOR`] is timed from
/// then, the same moment the page starts its own slide-out timer, so a slow
/// webview start can't cut the slide-out short.
static READY: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Queues an unlock for the overlay.
pub fn show(
    app: &AppHandle,
    display_name: String,
    description: String,
    game_name: String,
    icon: Option<String>,
) {
    let tx = QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(run(app.clone(), rx));
        tx
    });
    let _ = tx.send(OverlayToast {
        display_name,
        description,
        game_name,
        icon,
        duration_ms: SHOW_FOR.as_millis() as u64,
    });
}

async fn run(app: AppHandle, mut rx: mpsc::UnboundedReceiver<OverlayToast>) {
    while let Some(toast) = rx.recv().await {
        if let Ok(mut c) = CURRENT.lock() {
            *c = Some(toast);
        }
        match open(&app) {
            Ok(window) => {
                let _ = tokio::time::timeout(READY_TIMEOUT, READY.notified()).await;
                tokio::time::sleep(SHOW_FOR).await;
                if let Err(e) = window.destroy() {
                    tracing::warn!(error = %e, "achievement overlay: close failed");
                }
            }
            Err(e) => tracing::warn!(error = %e, "achievement overlay: couldn't open window"),
        }
        tokio::time::sleep(GAP).await;
    }
}

/// Bottom-right of the primary monitor's work area (above the taskbar), in
/// logical pixels.
fn corner(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok()??;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let right = (area.position.x as f64 + area.size.width as f64) / scale;
    let bottom = (area.position.y as f64 + area.size.height as f64) / scale;
    Some((right - WIDTH - MARGIN, bottom - HEIGHT - MARGIN))
}

fn open(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let label = format!("{LABEL_PREFIX}-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let mut builder =
        WebviewWindowBuilder::new(app, label, WebviewUrl::App("achievement-toast".into()))
            .title("Achievement unlocked")
            .inner_size(WIDTH, HEIGHT)
            .decorations(false)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false);
    // macOS only offers transparency behind the private-API feature; it isn't
    // a target platform, so there the card just draws on an opaque window.
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.transparent(true);
    }
    if let Some((x, y)) = corner(app) {
        builder = builder.position(x, y);
    }
    let window = builder.build()?;
    // Clicks pass through to the game underneath.
    let _ = window.set_ignore_cursor_events(true);
    // Some window managers place a new window themselves; re-assert the corner.
    if let Some((x, y)) = corner(app) {
        let _ = window.set_position(LogicalPosition::new(x, y));
    }
    let _ = window.set_size(LogicalSize::new(WIDTH, HEIGHT));
    Ok(window)
}

/// What the overlay page should draw.
#[tauri::command]
pub fn achievement_toast_payload() -> Option<OverlayToast> {
    READY.notify_waiters();
    CURRENT.lock().ok().and_then(|c| c.clone())
}

/// Shows a sample toast, so the user can check where the overlay appears and
/// whether it draws over a game they're running.
#[tauri::command]
pub fn preview_achievement_overlay(app: AppHandle) {
    show(
        &app,
        "Achievement unlocked".into(),
        "This is how unlocks appear while you play.".into(),
        "Spool".into(),
        None,
    );
}

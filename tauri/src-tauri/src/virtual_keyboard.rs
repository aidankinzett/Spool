//! KDE Plasma on-screen keyboard control.
//!
//! WebKitGTK (Tauri's Linux webview) doesn't drive the Wayland `text-input-v3`
//! protocol when an HTML input gains focus, so KWin never learns that text is
//! wanted and its on-screen keyboard stays hidden. On a handheld or any touch
//! KDE session with no physical keyboard, tapping a text field does nothing.
//!
//! Rather than wait on WebKitGTK, the frontend tracks focus on editable
//! elements (see `+layout.svelte`) and calls these commands, which ask KWin to
//! show/hide its keyboard directly over D-Bus (`org.kde.KWin`,
//! `/VirtualKeyboard`). `forceActivate` overrides KWin's "only show when the
//! focused client requested text input" heuristic, which our webview never
//! trips.
//!
//! KDE-only by construction: the calls target `org.kde.KWin`. On other desktops,
//! in Steam Game Mode (gamescope, where Steam owns the on-screen keyboard), or
//! on Windows there's no such service, so the calls fail and are treated as
//! no-ops. The frontend further gates the listeners to Linux to avoid needless
//! IPC elsewhere.

/// Show the KDE on-screen keyboard. No-op (logged at debug) when there's no
/// KWin session bus to talk to.
#[tauri::command]
pub async fn show_virtual_keyboard() {
    #[cfg(target_os = "linux")]
    linux::set_active(true).await;
}

/// Hide the KDE on-screen keyboard.
#[tauri::command]
pub async fn hide_virtual_keyboard() {
    #[cfg(target_os = "linux")]
    linux::set_active(false).await;
}

#[cfg(target_os = "linux")]
mod linux {
    use zbus::{Connection, Proxy};

    const KWIN_DEST: &str = "org.kde.KWin";
    const VK_PATH: &str = "/VirtualKeyboard";
    const VK_IFACE: &str = "org.kde.kwin.VirtualKeyboard";

    /// Toggle KWin's on-screen keyboard. Connects to the session bus per call —
    /// focus changes are infrequent and a cached connection would have to cope
    /// with the bus restarting, which isn't worth the bookkeeping here.
    pub async fn set_active(active: bool) {
        let conn = match Connection::session().await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(error = %e, "vk: no session bus — keyboard toggle skipped");
                return;
            }
        };
        let proxy = match Proxy::new(&conn, KWIN_DEST, VK_PATH, VK_IFACE).await {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(error = %e, "vk: KWin proxy failed — not a KDE session?");
                return;
            }
        };
        if active {
            // forceActivate ignores `activeClientSupportsTextInput`, which the
            // webview reports false — a plain `active = true` write would be
            // rejected straight back to false.
            if let Err(e) = proxy.call_method("forceActivate", &()).await {
                tracing::debug!(error = %e, "vk: forceActivate failed");
            }
        } else if let Err(e) = proxy.set_property("active", false).await {
            tracing::debug!(error = %e, "vk: hide failed");
        }
    }
}

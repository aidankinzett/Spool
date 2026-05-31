//! System-suspend watcher for the session marker.
//!
//! When a device sleeps mid-session — most importantly a Steam Deck that
//! suspends while a game is running — every userspace process freezes, so the
//! session-marker heartbeat stops writing. Without intervention the marker's
//! `updated_at` would age past the stale window (180 s) and a peer launching
//! the same game would see a stale-Active marker and classify it as an
//! unsynced session rather than "in use and asleep".
//!
//! This watcher subscribes to systemd-logind's `PrepareForSleep` D-Bus signal,
//! which fires *before* the freeze. On the way down it sets `suspended=true` on
//! the marker (staleness check is skipped for suspended markers); on resume it
//! clears the flag and re-asserts our ownership, warning if another device took
//! over while we slept.
//!
//! To guarantee the suspend write lands before the system freezes, the watcher
//! holds a logind *delay* inhibitor and only releases it once the write has
//! been attempted — the standard "react before sleep" pattern.
//!
//! Non-Linux targets get a no-op watcher: the returned handle is an
//! already-finished task, so the caller's unconditional `.abort()` is harmless.

use tauri::AppHandle;
use tokio::task::JoinHandle;

/// Starts a per-session task that marks the session marker suspended when the
/// system sleeps. The returned handle is aborted by the runner when the session
/// ends (see `run_workflow`). No-op when cloud is unconfigured.
pub fn start_suspend_watcher(app: AppHandle, game_name: String) -> JoinHandle<()> {
    #[cfg(target_os = "linux")]
    {
        tokio::spawn(linux::watch(app, game_name))
    }
    #[cfg(not(target_os = "linux"))]
    {
        // No suspend integration off Linux; hand back a completed task so the
        // caller's `.abort()` is a harmless no-op.
        let _ = (&app, &game_name);
        tokio::spawn(async {})
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::rclone::{self, MarkerClass};
    use zbus::zvariant::OwnedFd;
    use zbus::{Connection, Proxy};

    const LOGIND_DEST: &str = "org.freedesktop.login1";
    const LOGIND_PATH: &str = "/org/freedesktop/login1";
    const LOGIND_IFACE: &str = "org.freedesktop.login1.Manager";

    /// Take a `delay` inhibitor on `sleep`. logind blocks the actual suspend
    /// until this fd is dropped (up to `InhibitDelayMaxSec`), giving us a window
    /// to write the marker before everything freezes.
    async fn take_delay_inhibitor(proxy: &Proxy<'_>) -> Option<OwnedFd> {
        match proxy
            .call::<_, _, OwnedFd>(
                "Inhibit",
                &(
                    "sleep",
                    "Spool",
                    "Marking game session as suspended",
                    "delay",
                ),
            )
            .await
        {
            Ok(fd) => Some(fd),
            Err(e) => {
                tracing::warn!(error = %e, "suspend: failed to take logind delay inhibitor");
                None
            }
        }
    }

    pub async fn watch(app: AppHandle, game_name: String) {
        let conn = match Connection::system().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "suspend: no system D-Bus — suspend handling disabled");
                return;
            }
        };
        let proxy = match Proxy::new(&conn, LOGIND_DEST, LOGIND_PATH, LOGIND_IFACE).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "suspend: logind proxy failed — suspend handling disabled");
                return;
            }
        };

        let mut signal = match proxy.receive_signal("PrepareForSleep").await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "suspend: PrepareForSleep subscribe failed");
                return;
            }
        };

        // Hold a delay inhibitor while awake so we get a window to react.
        // Re-taken after each resume.
        let mut inhibitor = take_delay_inhibitor(&proxy).await;

        use futures_util::StreamExt;
        while let Some(msg) = signal.next().await {
            // PrepareForSleep carries a single bool: true = about to sleep,
            // false = just resumed.
            let about_to_sleep: bool = match msg.body().deserialize() {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, "suspend: bad PrepareForSleep payload");
                    continue;
                }
            };

            if about_to_sleep {
                tracing::info!(game = %game_name, "suspend: system sleeping — marking session suspended");
                rclone::suspend_marker_for_app(&app, &game_name).await;
                // Release the inhibitor so the suspend can proceed now that
                // the marker is written.
                inhibitor = None;
            } else {
                tracing::info!(game = %game_name, "suspend: system resumed — re-checking session marker");
                // Re-evaluate the marker: was our session taken over while we slept?
                match rclone::resume_marker_for_app(&app, &game_name).await {
                    Some(MarkerClass::Absent) | None => {}
                    Some(MarkerClass::ActivePlaying { device_name })
                    | Some(MarkerClass::Unsynced { device_name }) => {
                        warn_marker_taken(&app, &game_name, &device_name);
                    }
                }
                inhibitor = take_delay_inhibitor(&proxy).await;
            }
        }

        // Keep the inhibitor fd alive for the whole loop; dropping here on task
        // abort releases it cleanly.
        drop(inhibitor);
    }

    /// Surface a best-effort warning when another device's marker replaced ours
    /// while we slept.
    fn warn_marker_taken(app: &AppHandle, game_name: &str, device_name: &str) {
        use tauri::Emitter;
        tracing::warn!(game = %game_name, %device_name, "suspend: session marker taken over while suspended");
        let msg = format!(
            "While this device slept, {device_name} started playing {game_name}. \
             Your unsynced progress here may be overwritten."
        );
        let _ = app.emit("sync:lock-taken", &msg);
        crate::runner::os_toast_if_hidden(app, "Spool: session taken over", &msg);
    }
}

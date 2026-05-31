//! System-suspend watcher for the play-state lock.
//!
//! When a device sleeps mid-session — most importantly a Steam Deck that
//! suspends while a game is running — every userspace process freezes, so the
//! lock heartbeat (`sync::start_heartbeat`) stops pinging. Without intervention
//! the sync server would mark the lock stale after a few minutes and let
//! another device grab it, even though the original session is merely asleep
//! and about to resume.
//!
//! This watcher subscribes to systemd-logind's `PrepareForSleep` D-Bus signal,
//! which fires *before* the freeze. On the way down it marks the lock suspended
//! (`POST /locks/:game/suspend`) so the server keeps it alive for a bounded
//! grace window; on resume it lets the normal heartbeat un-suspend it, and
//! warns if the lock was taken over while we slept.
//!
//! To guarantee the suspend POST actually lands before the system freezes, the
//! watcher holds a logind *delay* inhibitor lock and only releases it once the
//! suspend call has been fired — the standard "react before sleep" pattern.
//!
//! Non-Linux targets get a no-op watcher: the returned handle is an
//! already-finished task, so the caller's unconditional `.abort()` is harmless.

use tauri::AppHandle;
use tokio::task::JoinHandle;

/// Starts a per-session task that marks the play-state lock suspended when the
/// system sleeps. The returned handle is aborted by the runner when the session
/// ends (see `run_workflow`). No-op when sync is unconfigured — the inner calls
/// short-circuit on a missing server/device id.
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
    use crate::sync;
    use zbus::zvariant::OwnedFd;
    use zbus::{Connection, Proxy};

    const LOGIND_DEST: &str = "org.freedesktop.login1";
    const LOGIND_PATH: &str = "/org/freedesktop/login1";
    const LOGIND_IFACE: &str = "org.freedesktop.login1.Manager";

    /// Take a `delay` inhibitor on `sleep`. logind blocks the actual suspend
    /// until this fd is dropped (up to `InhibitDelayMaxSec`), giving us a window
    /// to mark the lock suspended before everything freezes.
    async fn take_delay_inhibitor(proxy: &Proxy<'_>) -> Option<OwnedFd> {
        match proxy
            .call::<_, _, OwnedFd>(
                "Inhibit",
                &(
                    "sleep",
                    "Spool",
                    "Marking game session lock as suspended",
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

        // Hold a delay inhibitor while awake so we get a window to react to the
        // next suspend. Re-taken after each resume.
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
                tracing::info!(game = %game_name, "suspend: system sleeping — marking lock suspended");
                sync::suspend_lock(&app, &game_name).await;
                // Release the inhibitor so the suspend can proceed now that the
                // lock is marked. Re-taken on resume.
                inhibitor = None;
            } else {
                tracing::info!(game = %game_name, "suspend: system resumed — refreshing lock");
                // Re-assert the lock immediately rather than waiting up to 30s
                // for the next heartbeat. If another device stole the suspended
                // lock while we slept, warn the user; the local game keeps
                // running regardless.
                match sync::acquire_lock(&app, &game_name, false).await {
                    sync::AcquireOutcome::Acquired => {}
                    sync::AcquireOutcome::Conflict { device_name, .. } => {
                        warn_lock_taken(&app, &game_name, &device_name);
                    }
                }
                inhibitor = take_delay_inhibitor(&proxy).await;
            }
        }

        // Keep the inhibitor fd alive for the whole loop; dropping here on task
        // abort releases it cleanly.
        drop(inhibitor);
    }

    /// Surface a best-effort warning when our suspended lock was taken over by
    /// another device while we slept. Emits an app event (toast on the library
    /// window) and an OS notification since the window is usually hidden during
    /// a Game-Mode session.
    fn warn_lock_taken(app: &AppHandle, game_name: &str, device_name: &str) {
        use tauri::Emitter;
        tracing::warn!(game = %game_name, %device_name, "suspend: lock taken over while suspended");
        let msg = format!(
            "While this device slept, {device_name} started playing {game_name}. \
             Your unsynced progress here may be overwritten."
        );
        let _ = app.emit("sync:lock-taken", &msg);
        crate::runner::os_toast_if_hidden(app, "Spool: session taken over", &msg);
    }
}

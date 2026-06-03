//! Gamepad input bridge.
//!
//! The webview can't read controllers itself: the Linux AppImage's WebKitGTK
//! ships without libmanette, so `navigator.getGamepads()` is permanently empty
//! even in Steam Game Mode where Steam Input presents a virtual Xbox pad. So we
//! read the pad here in Rust via [`gilrs`] (evdev on Linux, XInput/WGI on
//! Windows, IOKit on macOS — the same evdev layer Steam's virtual pad lives on)
//! and forward normalised events to the frontend as `gamepad:input`. The
//! frontend owns the focus/navigation semantics; this module is only the input
//! source.
//!
//! gilrs's `Gilrs` context isn't `Send` on every platform, so it's constructed
//! and owned inside a dedicated OS thread rather than moved across one.

use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// A single normalised input event sent to the frontend on `gamepad:input`.
///
/// `kind` is the discriminant the JS side switches on: `"button-down"`,
/// `"button-up"`, `"axis"`, `"connected"`, or `"disconnected"`. `button` /
/// `axis` carry gilrs's debug names (e.g. `"South"`, `"DPadUp"`,
/// `"LeftStickX"`); both are `None` for connect/disconnect events.
#[derive(Clone, Serialize)]
struct GamepadInput {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    button: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    axis: Option<String>,
    value: f32,
    /// Stable per-controller id (gilrs `GamepadId`, debug-formatted) so the
    /// frontend can tell two pads apart.
    gamepad: String,
}

/// Spawn the gamepad reader thread. Logs and no-ops if gilrs can't initialise
/// (no input subsystem, missing permissions) — the rest of the app is
/// unaffected and mouse/touch navigation still work.
pub fn spawn_bridge(app: AppHandle) {
    std::thread::Builder::new()
        .name("gamepad-bridge".into())
        .spawn(move || run(app))
        .ok();
}

fn run(app: AppHandle) {
    use gilrs::{Event, EventType, Gilrs};

    let mut gilrs = match Gilrs::new() {
        Ok(g) => g,
        Err(e) => {
            // `gilrs::Error` isn't `std::error::Error` on all versions, so format it.
            tracing::warn!(error = ?e, "gamepad bridge: gilrs init failed; controller input disabled");
            return;
        }
    };

    let pads = gilrs.gamepads().count();
    tracing::info!(pads, "gamepad bridge started");

    // Sticks emit a stream of AxisChanged events; only forward when the axis is
    // pushed well past centre and report a single crossing per direction so the
    // frontend gets discrete "moved" signals instead of a firehose. Keyed by
    // (gamepad, axis) debug string → last side (-1, 0, +1).
    let mut axis_side: std::collections::HashMap<String, i8> = std::collections::HashMap::new();

    loop {
        while let Some(Event { id, event, .. }) = gilrs.next_event() {
            let gamepad = format!("{id:?}");
            let payload = match event {
                EventType::ButtonPressed(btn, _) => Some(GamepadInput {
                    kind: "button-down",
                    button: Some(button_name(btn)),
                    axis: None,
                    value: 1.0,
                    gamepad,
                }),
                EventType::ButtonReleased(btn, _) => Some(GamepadInput {
                    kind: "button-up",
                    button: Some(button_name(btn)),
                    axis: None,
                    value: 0.0,
                    gamepad,
                }),
                EventType::AxisChanged(axis, value, _) => {
                    let key = format!("{gamepad}:{axis:?}");
                    let side: i8 = if value > 0.6 {
                        1
                    } else if value < -0.6 {
                        -1
                    } else {
                        0
                    };
                    let last = axis_side.get(&key).copied().unwrap_or(0);
                    if side != last {
                        axis_side.insert(key, side);
                        // Only emit on entering a pushed state, not on return to centre.
                        if side != 0 {
                            Some(GamepadInput {
                                kind: "axis",
                                button: None,
                                axis: Some(axis_name(axis)),
                                value,
                                gamepad,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                EventType::Connected => Some(GamepadInput {
                    kind: "connected",
                    button: None,
                    axis: None,
                    value: 0.0,
                    gamepad,
                }),
                EventType::Disconnected => Some(GamepadInput {
                    kind: "disconnected",
                    button: None,
                    axis: None,
                    value: 0.0,
                    gamepad,
                }),
                _ => None,
            };

            if let Some(p) = payload {
                if let Err(e) = app.emit("gamepad:input", p) {
                    tracing::warn!(error = %e, "gamepad bridge: emit failed");
                }
            }
        }

        // gilrs has no cross-platform blocking read we rely on here; poll at
        // ~120 Hz, which is responsive for menu nav and negligible CPU.
        std::thread::sleep(Duration::from_millis(8));
    }
}

/// gilrs `Button` → stable debug name (e.g. `"South"`, `"DPadUp"`).
fn button_name(btn: gilrs::Button) -> String {
    format!("{btn:?}")
}

/// gilrs `Axis` → stable debug name (e.g. `"LeftStickX"`).
fn axis_name(axis: gilrs::Axis) -> String {
    format!("{axis:?}")
}

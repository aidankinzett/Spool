//! Steam client process control — graceful shutdown + relaunch so a freshly
//! written `shortcuts.vdf` is picked up without the user restarting Steam by hand.
//!
//! Steam reads `shortcuts.vdf` only at startup and rewrites it from its in-memory
//! copy on a clean exit. So adding a non-Steam shortcut while Steam is running has
//! no effect until a restart — and worse, Steam's own shutdown write clobbers the
//! file we just wrote. The `add_to_steam` / `add_spool_to_steam` commands bracket
//! their write with this module: shut Steam down first (`steam -shutdown`), write
//! the shortcut, then relaunch so Steam reloads the new file fresh.
//!
//! Skipped in SteamOS / Steam Deck **Game Mode** — there Spool itself runs as a
//! Steam child, so restarting Steam would kill Spool mid-operation. The old
//! "restart Steam to see it" guidance still applies in that case.

use crate::error::AppResult;
use std::time::Duration;
use tokio::process::Command;

/// How long to wait for Steam to exit after `-shutdown` before giving up.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(20);
/// Poll interval while waiting for the Steam process to disappear.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// `CREATE_NO_WINDOW` — keep the brief `steam.exe -shutdown` invocation from
/// flashing a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What the caller should do about Steam once it has written the shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteamRestart {
    /// Steam wasn't running, or auto-restart was skipped (Game Mode / unsupported
    /// platform). Nothing to relaunch.
    NotRunning,
    /// Steam was running and has now exited. The caller should [`relaunch`] it
    /// after writing so Steam reloads the new `shortcuts.vdf`.
    ShutDown,
    /// Steam was running but didn't exit within the timeout. The shortcut is
    /// written anyway, but may not appear until the user restarts Steam.
    ShutdownFailed,
}

/// True when a local Steam client process is running. macOS is not a target of
/// the Add-to-Steam feature, so it always reports false there.
#[cfg(windows)]
async fn is_steam_running() -> bool {
    match Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq steam.exe", "/NH", "/FO", "CSV"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .await
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .to_lowercase()
            .contains("steam.exe"),
        Err(_) => false,
    }
}

#[cfg(target_os = "linux")]
async fn is_steam_running() -> bool {
    // `pgrep -x steam` matches the exact process name (the Steam bootstrap),
    // not games or helper processes with "steam" in their command line.
    Command::new("pgrep")
        .args(["-x", "steam"])
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(not(any(windows, target_os = "linux")))]
async fn is_steam_running() -> bool {
    false
}

/// True when Steam currently has a game running. Used by the frontend to warn
/// before Add-to-Steam shuts Steam down (which would close the running game).
///
/// Steam records the running game's appid (0 when none): on Windows in
/// `HKCU\Software\Valve\Steam\RunningAppID`, on Linux in the text `registry.vdf`.
/// Advisory only — the file/registry can lag a few seconds behind reality.
#[cfg(windows)]
fn game_running() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(r"Software\Valve\Steam") {
        Ok(key) => key.get_value::<u32, _>("RunningAppID").unwrap_or(0) != 0,
        Err(_) => false,
    }
}

#[cfg(target_os = "linux")]
fn game_running() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    for rel in [".steam/registry.vdf", ".steam/steam/registry.vdf"] {
        if let Ok(text) = std::fs::read_to_string(home.join(rel)) {
            if let Some(id) = parse_running_app_id(&text) {
                return id != 0;
            }
        }
    }
    false
}

#[cfg(not(any(windows, target_os = "linux")))]
fn game_running() -> bool {
    false
}

/// Pulls the `RunningAppID` value out of Steam's Linux `registry.vdf` (a nested
/// quoted-token text format): finds the `"RunningAppID"` key and parses the
/// quoted token that follows it. Returns `None` when the key is absent.
#[cfg(any(target_os = "linux", test))]
fn parse_running_app_id(text: &str) -> Option<u64> {
    const KEY: &str = "\"RunningAppID\"";
    let after_key = &text[text.find(KEY)? + KEY.len()..];
    let start = after_key.find('"')? + 1;
    let end = after_key[start..].find('"')? + start;
    after_key[start..end].trim().parse::<u64>().ok()
}

/// True when Steam currently has a game running (see [`game_running`]).
#[tauri::command]
pub fn steam_game_running() -> bool {
    game_running()
}

/// Asks Steam to shut down cleanly via `steam -shutdown`.
#[cfg(windows)]
async fn request_shutdown() -> AppResult<()> {
    let exe = crate::steam::steam_executable()?;
    Command::new(exe)
        .arg("-shutdown")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| crate::error::AppError::Other(format!("failed to run steam -shutdown: {e}")))
}

#[cfg(target_os = "linux")]
async fn request_shutdown() -> AppResult<()> {
    let mut cmd = Command::new("steam");
    cmd.arg("-shutdown");
    // Hand Steam the host environment, not Spool's AppImage bundle (see
    // `process::strip_appimage_env`), or its own libraries break.
    crate::process::strip_appimage_env(&mut cmd);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| crate::error::AppError::Other(format!("failed to run steam -shutdown: {e}")))
}

#[cfg(not(any(windows, target_os = "linux")))]
async fn request_shutdown() -> AppResult<()> {
    Ok(())
}

/// Shut Steam down if it's running, in preparation for writing `shortcuts.vdf`.
///
/// Returns [`SteamRestart::ShutDown`] when Steam was running and has now exited,
/// so the caller knows to [`relaunch`] it once the shortcut is written. Returns
/// [`SteamRestart::NotRunning`] when Steam was already closed or auto-restart is
/// skipped (Game Mode / unsupported platform), and [`SteamRestart::ShutdownFailed`]
/// when Steam was running but didn't exit in time.
pub async fn shut_down_for_write() -> SteamRestart {
    // In Game Mode Spool is a child of Steam — shutting Steam down would kill us.
    if crate::gamemode::is_steam_game_mode() {
        return SteamRestart::NotRunning;
    }
    if !is_steam_running().await {
        return SteamRestart::NotRunning;
    }

    if let Err(e) = request_shutdown().await {
        tracing::warn!(%e, "steam shutdown request failed");
        return SteamRestart::ShutdownFailed;
    }

    let deadline = tokio::time::Instant::now() + SHUTDOWN_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(POLL_INTERVAL).await;
        if !is_steam_running().await {
            tracing::debug!("steam exited after -shutdown");
            return SteamRestart::ShutDown;
        }
    }
    tracing::warn!("steam did not exit within {SHUTDOWN_TIMEOUT:?} after -shutdown");
    SteamRestart::ShutdownFailed
}

/// Relaunch the Steam client (detached). Best-effort: a failure to start Steam
/// back up is logged, not surfaced — the shortcut is already written.
pub async fn relaunch() {
    #[cfg(windows)]
    {
        match crate::steam::steam_executable() {
            Ok(exe) => {
                if let Err(e) = Command::new(exe).creation_flags(CREATE_NO_WINDOW).spawn() {
                    tracing::warn!(%e, "failed to relaunch steam");
                }
            }
            Err(e) => tracing::warn!(%e, "failed to resolve steam.exe for relaunch"),
        }
    }
    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("steam");
        crate::process::strip_appimage_env(&mut cmd);
        if let Err(e) = cmd.spawn() {
            tracing::warn!(%e, "failed to relaunch steam");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_running_app_id;

    const REGISTRY: &str = r#"
"Registry"
{
	"HKCU"
	{
		"Software"
		{
			"Valve"
			{
				"Steam"
				{
					"language"		"english"
					"RunningAppID"		"PLACEHOLDER"
					"SourceModInstallPath"		"/home/user/.steam"
				}
			}
		}
	}
}
"#;

    #[test]
    fn parses_running_app_id_when_a_game_runs() {
        let text = REGISTRY.replace("PLACEHOLDER", "440");
        assert_eq!(parse_running_app_id(&text), Some(440));
    }

    #[test]
    fn parses_zero_when_no_game_runs() {
        let text = REGISTRY.replace("PLACEHOLDER", "0");
        assert_eq!(parse_running_app_id(&text), Some(0));
    }

    #[test]
    fn returns_none_when_key_absent() {
        assert_eq!(parse_running_app_id("\"Steam\" { }"), None);
    }
}

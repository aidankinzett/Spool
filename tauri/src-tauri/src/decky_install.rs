//! One-click installer for the companion **Spool Backup** Decky Loader plugin.
//!
//! The plugin (a tiny TS frontend + Python backend, source in `decky/`) closes
//! the SteamOS forced-close backup gap: when a user picks *Exit Game* from the
//! Quick Access menu, Steam SIGKILLs Spool before its post-session backup runs;
//! the plugin's backend lives outside the game's process tree and re-runs
//! `spool --backup` as a safety net. See
//! `docs/superpowers/specs/2026-05-30-decky-forced-close-backup-design.md`.
//!
//! This module lets Spool install/update that plugin for the user instead of
//! making them shuffle files in Desktop Mode. The plugin payload is **embedded**
//! into the Spool binary at compile time (same pattern as the Armoury Crate
//! `launcher_stub.exe`), so the install works offline and is version-locked to
//! the running Spool build.
//!
//! Linux-only. Decky's plugin dir (`~/homebrew/plugins`) and its
//! `plugin_loader` systemd service are root-owned, so installing means a
//! privileged copy + service restart — done via a single `pkexec` prompt.
//! Everything is `#[cfg(target_os = "linux")]`; on Windows/macOS the commands
//! report "unsupported" and the embedded payload is never compiled in (so those
//! builds don't even need the plugin to have been built).

use crate::error::{AppError, AppResult};
use serde::Serialize;

/// Status of the companion Decky plugin, for the Settings UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckyPluginInfo {
    /// Whether this platform can install the plugin at all (Linux only).
    pub supported: bool,
    /// Whether a copy is already present in `~/homebrew/plugins/spool-backup`.
    pub installed: bool,
    /// `version` from the installed plugin's `package.json`, if readable.
    pub installed_version: Option<String>,
    /// `version` from the embedded (bundled) plugin's `package.json`.
    pub bundled_version: String,
    /// Whether Decky Loader itself appears to be installed (`~/homebrew`).
    pub decky_present: bool,
}

/// The plugin's directory name under `~/homebrew/plugins/`.
#[allow(dead_code)] // only read by the Linux install path
const PLUGIN_DIR_NAME: &str = "spool-backup";

// ── Embedded plugin payload (Linux only) ────────────────────────────────────
// include_str! paths are relative to THIS source file. `../../../` climbs
// src-tauri/src → src-tauri → tauri → repo root, where `decky/` lives. On Linux
// these require the plugin to have been built (`pnpm build` → dist/index.js)
// before Spool is compiled; CI's Linux build does this. The whole block is
// cfg'd out on Windows/macOS so their builds never reference the files.
#[cfg(target_os = "linux")]
mod embedded {
    pub const INDEX_JS: &str = include_str!("../../../decky/dist/index.js");
    pub const MAIN_PY: &str = include_str!("../../../decky/main.py");
    pub const BACKUP_LOGIC_PY: &str = include_str!("../../../decky/backup_logic.py");
    pub const PLUGIN_JSON: &str = include_str!("../../../decky/plugin.json");
    pub const PACKAGE_JSON: &str = include_str!("../../../decky/package.json");
}

/// Best-effort parse of the `"version"` string out of a `package.json` blob.
/// Avoids pulling the whole serde value tree for one field.
#[allow(dead_code)] // on non-Linux only the test references this
fn parse_package_version(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.get("version")?.as_str().map(|s| s.to_string())
}

/// Embedded plugin version (compile-time payload). Empty string only if the
/// bundled `package.json` somehow lacks a version.
#[cfg(target_os = "linux")]
fn bundled_version() -> String {
    parse_package_version(embedded::PACKAGE_JSON).unwrap_or_default()
}

#[cfg(not(target_os = "linux"))]
fn bundled_version() -> String {
    String::new()
}

// ── Linux implementation ────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::path::PathBuf;

    /// `~/homebrew` — Decky's root. Plugins live in `plugins/` under it.
    fn homebrew_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join("homebrew"))
    }

    fn plugins_dir() -> Option<PathBuf> {
        homebrew_dir().map(|h| h.join("plugins"))
    }

    fn installed_dir() -> Option<PathBuf> {
        plugins_dir().map(|p| p.join(PLUGIN_DIR_NAME))
    }

    pub fn status() -> DeckyPluginInfo {
        let decky_present = homebrew_dir().map(|h| h.is_dir()).unwrap_or(false);
        let installed_dir = installed_dir();
        let installed = installed_dir
            .as_ref()
            .map(|d| d.join("plugin.json").is_file())
            .unwrap_or(false);
        let installed_version = installed_dir
            .as_ref()
            .filter(|_| installed)
            .and_then(|d| std::fs::read_to_string(d.join("package.json")).ok())
            .and_then(|s| parse_package_version(&s));

        DeckyPluginInfo {
            supported: true,
            installed,
            installed_version,
            bundled_version: bundled_version(),
            decky_present,
        }
    }

    /// Stage the embedded payload into a private dir, then run one privileged
    /// `pkexec` action to copy it into Decky's (root-owned) plugin dir and
    /// restart the loader so the plugin shows up.
    pub fn install() -> AppResult<()> {
        let plugins = plugins_dir()
            .ok_or_else(|| AppError::Other("could not resolve your home directory".into()))?;

        // 1. Write the payload to a staging dir we own.
        let staging = crate::paths::app_data_dir()
            .join("decky-staging")
            .join(PLUGIN_DIR_NAME);
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir_all(staging.join("dist"))?;
        std::fs::write(staging.join("dist").join("index.js"), embedded::INDEX_JS)?;
        std::fs::write(staging.join("main.py"), embedded::MAIN_PY)?;
        std::fs::write(staging.join("backup_logic.py"), embedded::BACKUP_LOGIC_PY)?;
        std::fs::write(staging.join("plugin.json"), embedded::PLUGIN_JSON)?;
        std::fs::write(staging.join("package.json"), embedded::PACKAGE_JSON)?;

        // 2. Ensure pkexec exists before we try to use it (clearer error).
        if crate::paths::find_system_binary("pkexec").is_none() {
            return Err(AppError::Other(
                "pkexec not found — install polkit, or copy the plugin manually from Desktop Mode."
                    .into(),
            ));
        }

        // 3. One elevated step: replace the plugin dir, fix ownership, restart
        //    the loader. Args are passed positionally so paths with spaces are
        //    safe ($1 = plugins dir, $2 = staging source).
        let script = r#"
set -e
PLUGINS="$1"
SRC="$2"
mkdir -p "$PLUGINS"
rm -rf "$PLUGINS/spool-backup"
cp -r "$SRC" "$PLUGINS/spool-backup"
chown -R root:root "$PLUGINS/spool-backup" 2>/dev/null || true
systemctl restart plugin_loader 2>/dev/null || true
"#;

        let status = std::process::Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(script)
            .arg("sh") // $0
            .arg(plugins.as_os_str())
            .arg(staging.as_os_str())
            .status()
            .map_err(|e| AppError::Other(format!("failed to launch pkexec: {e}")))?;

        if !status.success() {
            // pkexec exits 126 when the user dismisses/declines the auth dialog,
            // 127 when authorization can't be obtained (e.g. no agent in Game
            // Mode). Give a useful hint either way.
            let code = status.code().unwrap_or(-1);
            let hint = match code {
                126 => " (authorization dialog was dismissed)",
                127 => " (no polkit agent — run this from Desktop Mode)",
                _ => "",
            };
            return Err(AppError::Other(format!(
                "plugin install did not complete{hint}. Exit code {code}."
            )));
        }

        Ok(())
    }
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn decky_plugin_status() -> AppResult<DeckyPluginInfo> {
    #[cfg(target_os = "linux")]
    {
        Ok(imp::status())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(DeckyPluginInfo {
            supported: false,
            installed: false,
            installed_version: None,
            bundled_version: bundled_version(),
            decky_present: false,
        })
    }
}

#[tauri::command]
pub async fn install_decky_plugin() -> AppResult<()> {
    #[cfg(target_os = "linux")]
    {
        // The pkexec call blocks on a GUI prompt — keep it off the async runtime
        // worker so we don't stall other IPC while the user authenticates.
        tauri::async_runtime::spawn_blocking(imp::install)
            .await
            .map_err(|e| AppError::Other(format!("install task panicked: {e}")))?
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(AppError::Other(
            "The Decky plugin is only available on SteamOS / Linux.".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_package_version;

    #[test]
    fn parses_version_field() {
        assert_eq!(
            parse_package_version(r#"{"name":"x","version":"0.1.0"}"#).as_deref(),
            Some("0.1.0"),
        );
    }

    #[test]
    fn missing_version_is_none() {
        assert_eq!(parse_package_version(r#"{"name":"x"}"#), None);
        assert_eq!(parse_package_version("not json"), None);
    }
}

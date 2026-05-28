//! Application settings — persisted to `%LOCALAPPDATA%\Spool\config.json`.
//!
//! The on-disk format mirrors the C# `ConfigData` exactly so an existing
//! Spool installation's config loads without migration. Fields that aren't
//! yet exposed in the new UI (LAN share, sync server, TorBox, download
//! sources) are still modelled so the file round-trips cleanly with the
//! C# app — they're just inert until v2.

use crate::error::{AppError, AppResult};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

/// On-disk shape. Every field has a default so older config.json files
/// without newer fields parse cleanly via `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigData {
    pub ludusavi_path: String,
    pub steamgriddb_enabled: bool,
    pub steamgriddb_api_key: String,
    pub spool_exe: String,
    /// `"system"`, `"dark"`, or `"light"`. The new design is dark-only but
    /// the field is preserved for compatibility with the C# config.
    pub theme: String,

    // ── Identity (assigned once at first run) ────────────────────────────
    pub device_id: String,
    pub device_name: String,

    // ── v2 — deferred but modelled for JSON round-trip compat ─────────────
    pub sync_server_enabled: bool,
    pub sync_server_url: String,
    pub sync_server_api_key: String,

    pub lan_share_enabled: bool,
    pub lan_share_port: u16,
    pub lan_install_dir: String,

    pub torbox_enabled: bool,
    pub torbox_api_key: String,
    pub download_dir: String,
    pub download_sources: Vec<String>,

    /// `"auto"`, `"on"`, or `"off"`. Touch-optimised UI mode (handheld).
    pub touch_mode: String,

    /// Set to true after the user has been shown the "Spool is in the tray"
    /// intro toast at least once. Defaults to false on legacy configs (and
    /// new installs) so the toast appears on the first close-to-tray.
    pub tray_intro_seen: bool,
}

impl Default for ConfigData {
    fn default() -> Self {
        Self {
            ludusavi_path: String::new(),
            steamgriddb_enabled: false,
            steamgriddb_api_key: String::new(),
            spool_exe: String::new(),
            theme: "system".to_string(),
            device_id: String::new(),
            device_name: String::new(),
            sync_server_enabled: false,
            sync_server_url: String::new(),
            sync_server_api_key: String::new(),
            lan_share_enabled: true,
            lan_share_port: 47632,
            lan_install_dir: String::new(),
            torbox_enabled: false,
            torbox_api_key: String::new(),
            download_dir: String::new(),
            download_sources: Vec::new(),
            touch_mode: "auto".to_string(),
            tray_intro_seen: false,
        }
    }
}

/// Wrapper around [`ConfigData`] handling persistence and one-time setup
/// (device identity, ludusavi auto-detect, current-exe stamping).
#[derive(Debug, Default)]
pub struct Config {
    pub data: ConfigData,
}

impl Config {
    /// Loads from disk, then runs first-time setup if needed (device ID,
    /// ludusavi auto-detection, current exe path). Saves if any of those
    /// touched the data so the on-disk file matches in-memory state.
    pub fn load() -> AppResult<Self> {
        let path = paths::config_file();
        let mut data = if path.exists() {
            let json = std::fs::read_to_string(&path)?;
            serde_json::from_str::<ConfigData>(&json).unwrap_or_default()
        } else {
            ConfigData::default()
        };

        let mut changed = false;
        changed |= ensure_device_identity(&mut data);
        changed |= auto_detect_ludusavi(&mut data);
        changed |= stamp_current_exe(&mut data);

        let cfg = Self { data };
        if changed {
            // Best-effort save — if it fails, the next mutating op retries.
            let _ = cfg.save();
        }
        Ok(cfg)
    }

    /// Atomic save: write-temp + rename, with a `.bak` of the previous file.
    pub fn save(&self) -> AppResult<()> {
        let path = paths::config_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(&tmp, json)?;
        if path.exists() {
            let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        }
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// True iff `ludusavi_path` points at a file that actually exists.
    pub fn is_ludusavi_ok(&self) -> bool {
        !self.data.ludusavi_path.is_empty() && PathBuf::from(&self.data.ludusavi_path).is_file()
    }
}

/// Shared config state. Locks are short (read/clone or mutate+save) so a
/// std::sync::Mutex is fine — same rule as the library: never hold across
/// `.await`. See `library.rs` for the rationale.
pub type SharedConfig = Mutex<Config>;

// ── First-run helpers ───────────────────────────────────────────────────────

/// Assigns a stable device id (uuid v4) and device name (OS hostname).
/// Returns true if anything was written.
fn ensure_device_identity(data: &mut ConfigData) -> bool {
    let mut changed = false;
    if data.device_id.is_empty() {
        data.device_id = uuid::Uuid::new_v4().to_string();
        changed = true;
    }
    if data.device_name.is_empty() {
        data.device_name = hostname();
        changed = true;
    }
    changed
}

/// Walks the current-exe directory and the system PATH looking for
/// `ludusavi(.exe)`. Returns true if a path was set.
fn auto_detect_ludusavi(data: &mut ConfigData) -> bool {
    if !data.ludusavi_path.is_empty() && PathBuf::from(&data.ludusavi_path).is_file() {
        return false;
    }

    let exe_name = if cfg!(windows) { "ludusavi.exe" } else { "ludusavi" };

    // 1. Beside our own executable
    if let Ok(self_exe) = env::current_exe() {
        if let Some(dir) = self_exe.parent() {
            let candidate = dir.join(exe_name);
            if candidate.is_file() {
                data.ludusavi_path = candidate.to_string_lossy().to_string();
                return true;
            }
        }
    }

    // 2. PATH
    if let Some(path_env) = env::var_os("PATH") {
        for dir in env::split_paths(&path_env) {
            let candidate = dir.join(exe_name);
            if candidate.is_file() {
                data.ludusavi_path = candidate.to_string_lossy().to_string();
                return true;
            }
        }
    }

    false
}

/// Stamps `spool_exe` with the current process path so generated launcher
/// stubs (the Armoury Crate stub, etc.) know where to call back to.
fn stamp_current_exe(data: &mut ConfigData) -> bool {
    if let Ok(exe) = env::current_exe() {
        let s = exe.to_string_lossy().to_string();
        if data.spool_exe != s {
            data.spool_exe = s;
            return true;
        }
    }
    false
}

fn hostname() -> String {
    // %COMPUTERNAME% on Windows, $HOSTNAME / $HOST elsewhere — fall back
    // to "Spool device" if nothing's set.
    env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .or_else(|_| env::var("HOST"))
        .unwrap_or_else(|_| "Spool device".to_string())
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_config(state: State<'_, SharedConfig>) -> AppResult<ConfigData> {
    let cfg = state.lock().map_err(|_| AppError::LockPoisoned)?;
    Ok(cfg.data.clone())
}

/// Replaces the in-memory config with `data` and persists to disk. The
/// frontend sends back the full ConfigData; partial patches happen client-
/// side. Simpler than a per-field patch surface, matches a "live save" UX.
#[tauri::command]
pub fn update_config(
    state: State<'_, SharedConfig>,
    data: ConfigData,
) -> AppResult<ConfigData> {
    let mut cfg = state.lock().map_err(|_| AppError::LockPoisoned)?;
    cfg.data = data;
    cfg.save()?;
    Ok(cfg.data.clone())
}

/// Runs the ludusavi auto-detection routine on demand. Returns the
/// resulting path (empty string if nothing was found). Persists if found.
#[tauri::command]
pub fn detect_ludusavi(state: State<'_, SharedConfig>) -> AppResult<String> {
    let mut cfg = state.lock().map_err(|_| AppError::LockPoisoned)?;
    if auto_detect_ludusavi(&mut cfg.data) {
        cfg.save()?;
    }
    Ok(cfg.data.ludusavi_path.clone())
}

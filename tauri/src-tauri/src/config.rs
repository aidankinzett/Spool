//! Application settings — persisted to `%LOCALAPPDATA%\Spool\config.json`.
//!
//! The on-disk format mirrors the C# `ConfigData` exactly so an existing
//! Spool installation's config loads without migration. Fields that aren't
//! yet exposed in the new UI (LAN share) are still modelled so
//! the file round-trips cleanly with the C# app — they're just inert until
//! v2.

use crate::error::{AppError, AppResult};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

/// UI density / layout mode. `Auto` resolves at runtime (frontend) to
/// `desktop` or `touch` from pointer + panel size; `Desktop`/`Touch` force
/// it. Serialized lowercase (`"auto"`/`"desktop"`/`"touch"`) to match the
/// `UiMode` union in types.ts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiMode {
    #[default]
    Auto,
    Desktop,
    Touch,
}

/// On-disk shape. Every field has a default so older config.json files
/// without newer fields parse cleanly via `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigData {
    pub steamgriddb_enabled: bool,
    pub steamgriddb_api_key: String,
    pub spool_exe: String,
    /// `"system"`, `"dark"`, or `"light"`. The new design is dark-only but
    /// the field is preserved for compatibility with the C# config.
    pub theme: String,

    // ── Identity (assigned once at first run) ────────────────────────────
    pub device_id: String,
    pub device_name: String,

    pub lan_share_enabled: bool,
    pub lan_share_port: u16,
    pub lan_install_dir: String,
    /// Max aggregate LAN download throughput in Mbps (megabits/s,
    /// decimal). `0` = unlimited. Applied across all parallel file
    /// fetches by the throttle in `download_one_file` — convergent
    /// rather than precise, so brief bursts can exceed the cap before
    /// the next sleep brings the average back.
    pub lan_download_max_mbps: f64,

    // ── Proton / Linux launch ────────────────────────────────────────────
    /// Path to the `umu-run` launcher. `""` = autodetect (`/usr/bin/umu-run`
    /// then PATH). Linux-only; ignored on Windows.
    pub umu_run_path: String,
    /// Default Proton build directory used when a game doesn't override it.
    /// `""` = auto-pick the newest discovered Proton.
    pub default_proton_path: String,

    /// Touch-optimised UI mode (handheld). Resolved to a concrete
    /// desktop/touch density at boot by `lib/uiMode.svelte.ts`.
    pub ui_mode: UiMode,

    /// Set to true after the user has been shown the "Spool is in the tray"
    /// intro toast at least once. Defaults to false on legacy configs (and
    /// new installs) so the toast appears on the first close-to-tray.
    pub tray_intro_seen: bool,

    // ── Cloud / rclone settings ──────────────────────────────────────────
    pub cloud_provider: String,
    pub cloud_remote: String,
    /// Base folder on the remote. Ludusavi saves go to
    /// `<cloud_base_path>/ludusavi-backup`; Spool's cross-device control plane
    /// (session markers, per-device blobs) lives under `<cloud_base_path>/_spool`.
    pub cloud_base_path: String,
    /// Legacy: the exact ludusavi remote subpath. Superseded by
    /// `cloud_base_path` (the ludusavi path is now derived). Kept for JSON
    /// round-trip with older config files; no longer read.
    pub cloud_path: String,
    pub rclone_args: String,
    /// WebDAV connection details for the `webdav` provider. Written when the
    /// user connects a WebDAV remote (manually or via the self-hosted Spool
    /// server). The password is never stored here — ludusavi obscures it into
    /// rclone.conf; these two fields only let the settings form re-display the
    /// active connection.
    pub cloud_webdav_url: String,
    pub cloud_webdav_username: String,

    /// Number of full save revisions ludusavi retains per game (the
    /// `backup.retention.full` knob). More revisions = more rollback points,
    /// at the cost of more disk + cloud upload per game. Differentials stay at
    /// 0 (see `ludusavi_config::ensure_config`). Default 3; clamped to 1–10
    /// when applied.
    pub save_retention_full: u32,
}

impl Default for ConfigData {
    fn default() -> Self {
        Self {
            steamgriddb_enabled: false,
            steamgriddb_api_key: String::new(),
            spool_exe: String::new(),
            theme: "system".to_string(),
            device_id: String::new(),
            device_name: String::new(),
            lan_share_enabled: true,
            lan_share_port: 47632,
            lan_install_dir: String::new(),
            lan_download_max_mbps: 0.0,
            umu_run_path: String::new(),
            default_proton_path: String::new(),
            ui_mode: UiMode::default(),
            tray_intro_seen: false,
            cloud_provider: String::new(),
            cloud_remote: String::new(),
            cloud_base_path: "Spool".to_string(),
            cloud_path: "Spool/ludusavi-backup".to_string(),
            rclone_args: "--fast-list --ignore-checksum".to_string(),
            cloud_webdav_url: String::new(),
            cloud_webdav_username: String::new(),
            save_retention_full: 3,
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
    /// umu-run auto-detection, current exe path). Saves if any of those
    /// touched the data so the on-disk file matches in-memory state.
    pub fn load() -> AppResult<Self> {
        let path = paths::config_file();
        let mut data = if path.exists() {
            let json = std::fs::read_to_string(&path)?;
            match serde_json::from_str::<ConfigData>(&json) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(error = %e, "config.json is malformed; attempting .bak recovery");
                    let bak = path.with_extension("json.bak");
                    let recovered = bak
                        .exists()
                        .then(|| std::fs::read_to_string(&bak).ok())
                        .flatten()
                        .and_then(|s| serde_json::from_str::<ConfigData>(&s).ok());
                    match recovered {
                        Some(d) => {
                            tracing::info!("recovered config from .bak");
                            d
                        }
                        None => {
                            tracing::warn!("config.bak also unreadable; using defaults");
                            ConfigData::default()
                        }
                    }
                }
            }
        } else {
            ConfigData::default()
        };

        let mut changed = false;
        changed |= ensure_device_identity(&mut data);
        changed |= auto_detect_umu_run(&mut data);
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

/// Locates `umu-run` (`/usr/bin/umu-run` then PATH) on non-Windows. Returns
/// true if a path was set. No-op on Windows where Proton launch isn't used.
fn auto_detect_umu_run(data: &mut ConfigData) -> bool {
    if cfg!(windows) {
        return false;
    }
    if !data.umu_run_path.is_empty() && PathBuf::from(&data.umu_run_path).is_file() {
        return false;
    }
    if let Ok(p) = crate::proton::resolve_umu_run(None) {
        data.umu_run_path = p.to_string_lossy().to_string();
        return true;
    }
    false
}

/// Stamps `spool_exe` with the process path so generated launcher stubs (the
/// Armoury Crate stub, etc.) know where to call back to. Uses the AppImage-
/// aware resolver so this is the stable `.AppImage` path, not the ephemeral
/// /tmp mount, when running as an AppImage.
fn stamp_current_exe(data: &mut ConfigData) -> bool {
    if let Some(exe) = paths::spool_executable() {
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

    let rclone_val = crate::paths::resolve_rclone_path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Sync cloud/rclone settings to Spool-owned ludusavi config.yaml. The
    // ludusavi remote subpath is derived from the base folder so it always sits
    // beside Spool's `_spool` control-plane dir.
    let ludusavi_remote_path = format!(
        "{}/ludusavi-backup",
        cfg.data.cloud_base_path.trim().trim_end_matches('/')
    );
    let _ = crate::ludusavi_config::set_cloud(
        Some(&cfg.data.cloud_provider),
        Some(&cfg.data.cloud_remote),
        Some(&ludusavi_remote_path),
        Some(&rclone_val),
        Some(&cfg.data.rclone_args),
    );

    // Push the save-revision retention knob into the owned config.yaml.
    let _ = crate::ludusavi_config::set_retention(cfg.data.save_retention_full);

    Ok(cfg.data.clone())
}

/// The host OS, so the frontend can gate Linux-only UI (Proton settings).
/// Returns `"windows"`, `"linux"`, or `"macos"` (Rust's `std::env::consts::OS`).
#[tauri::command]
pub fn app_platform() -> String {
    std::env::consts::OS.to_string()
}

/// Runs umu-run auto-detection on demand (Settings → Compatibility). Returns
/// the resulting path (empty string if nothing was found). Persists if found.
#[tauri::command]
pub fn detect_umu_run(state: State<'_, SharedConfig>) -> AppResult<String> {
    let mut cfg = state.lock().map_err(|_| AppError::LockPoisoned)?;
    if auto_detect_umu_run(&mut cfg.data) {
        cfg.save()?;
    }
    Ok(cfg.data.umu_run_path.clone())
}

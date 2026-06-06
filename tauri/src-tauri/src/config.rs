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
///
/// The cloud / LAN / Proton-launch settings are grouped into sub-structs for
/// clarity in Rust, but `#[serde(flatten)]` keeps the on-disk JSON flat — the
/// historical `cloud_*` / `lan_*` / `umu_run_path` keys are unchanged, so
/// existing config.json files and the frontend's flat `ConfigData` mirror load
/// without migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigData {
    pub steamgriddb_enabled: bool,
    pub steamgriddb_api_key: String,
    pub spool_exe: String,

    // ── Identity (assigned once at first run) ────────────────────────────
    pub device_id: String,
    pub device_name: String,

    /// Touch-optimised UI mode (handheld). Resolved to a concrete
    /// desktop/touch density at boot by `lib/uiMode.svelte.ts`.
    pub ui_mode: UiMode,

    /// Set to true after the user has been shown the "Spool is in the tray"
    /// intro toast at least once. Defaults to false on legacy configs (and
    /// new installs) so the toast appears on the first close-to-tray.
    pub tray_intro_seen: bool,

    /// Set to true once the first-run onboarding flow has been finished (or
    /// dismissed). Defaults to false so it shows on a fresh install. A
    /// pre-existing config that predates this field is migrated to `true` on
    /// load (see `migrate_onboarding_completed`) so upgrading users don't get
    /// the first-run flow thrown at them.
    pub onboarding_completed: bool,

    /// Number of full save revisions ludusavi retains per game (the
    /// `backup.retention.full` knob). More revisions = more rollback points,
    /// at the cost of more disk + cloud upload per game. Differentials stay at
    /// 0 (see `ludusavi_config::ensure_config`). Default 3; clamped to 1–10
    /// when applied.
    pub save_retention_full: u32,

    /// Cloud-save / rclone settings (flattened to the flat `cloud_*` JSON keys).
    #[serde(flatten)]
    pub cloud: CloudConfig,

    /// LAN game-sharing settings (flattened to the flat `lan_*` JSON keys).
    #[serde(flatten)]
    pub lan: LanConfig,

    /// Proton / Linux launch settings (flattened; field names match their keys).
    #[serde(flatten)]
    pub launch: LaunchConfig,
}

impl Default for ConfigData {
    fn default() -> Self {
        Self {
            steamgriddb_enabled: false,
            steamgriddb_api_key: String::new(),
            spool_exe: String::new(),
            device_id: String::new(),
            device_name: String::new(),
            ui_mode: UiMode::default(),
            tray_intro_seen: false,
            onboarding_completed: false,
            save_retention_full: 3,
            cloud: CloudConfig::default(),
            lan: LanConfig::default(),
            launch: LaunchConfig::default(),
        }
    }
}

/// Cloud-save + rclone settings. Flattened into [`ConfigData`], so each field
/// maps to its historical flat JSON key (`cloud_provider`, …) — existing
/// config.json files and the frontend's flat mirror round-trip unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudConfig {
    #[serde(rename = "cloud_provider")]
    pub provider: String,
    #[serde(rename = "cloud_remote")]
    pub remote: String,
    /// Base folder on the remote. Ludusavi saves go to
    /// `<base_path>/ludusavi-backup`; Spool's cross-device control plane
    /// (session markers, per-device blobs) lives under `<base_path>/_spool`.
    #[serde(rename = "cloud_base_path")]
    pub base_path: String,
    /// Legacy: the exact ludusavi remote subpath. Superseded by `base_path`
    /// (the ludusavi path is now derived). Kept for JSON round-trip with older
    /// config files; no longer read.
    #[serde(rename = "cloud_path")]
    pub path: String,
    pub rclone_args: String,
    /// WebDAV connection details for the `webdav` provider. Written when the
    /// user connects a WebDAV remote (manually or via the self-hosted Spool
    /// server). The password is never stored here — ludusavi obscures it into
    /// rclone.conf; these two fields only let the settings form re-display the
    /// active connection.
    #[serde(rename = "cloud_webdav_url")]
    pub webdav_url: String,
    #[serde(rename = "cloud_webdav_username")]
    pub webdav_username: String,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            remote: String::new(),
            base_path: "Spool".to_string(),
            path: "Spool/ludusavi-backup".to_string(),
            rclone_args: "--fast-list --ignore-checksum".to_string(),
            webdav_url: String::new(),
            webdav_username: String::new(),
        }
    }
}

/// LAN game-sharing settings. Flattened into [`ConfigData`] with the historical
/// flat `lan_*` JSON keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LanConfig {
    #[serde(rename = "lan_share_enabled")]
    pub share_enabled: bool,
    #[serde(rename = "lan_share_port")]
    pub share_port: u16,
    #[serde(rename = "lan_install_dir")]
    pub install_dir: String,
    /// Max aggregate LAN download throughput in Mbps (megabits/s, decimal).
    /// `0` = unlimited. Applied across all parallel file fetches by the throttle
    /// in `download_one_file` — convergent rather than precise, so brief bursts
    /// can exceed the cap before the next sleep brings the average back.
    #[serde(rename = "lan_download_max_mbps")]
    pub download_max_mbps: f64,
}

impl Default for LanConfig {
    fn default() -> Self {
        Self {
            share_enabled: true,
            share_port: 47632,
            install_dir: String::new(),
            download_max_mbps: 0.0,
        }
    }
}

/// Proton / Linux launch settings. Flattened into [`ConfigData`]; the field
/// names already match their JSON keys, so no renames are needed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchConfig {
    /// Path to the `umu-run` launcher. `""` = autodetect (`/usr/bin/umu-run`
    /// then PATH). Linux-only; ignored on Windows.
    pub umu_run_path: String,
    /// Default Proton build directory used when a game doesn't override it.
    /// `""` = auto-pick the newest discovered Proton.
    pub default_proton_path: String,
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
        // Keep the raw JSON so the onboarding migration can tell a key that
        // was absent (legacy file) from one explicitly set to false.
        let raw_json = path.exists().then(|| std::fs::read_to_string(&path).ok()).flatten();
        let mut data = if let Some(json) = raw_json.as_deref() {
            match serde_json::from_str::<ConfigData>(json) {
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
        changed |= migrate_cloud_base_path(&mut data);
        changed |= migrate_onboarding_completed(raw_json.as_deref(), &mut data);

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
    if !data.launch.umu_run_path.is_empty() && PathBuf::from(&data.launch.umu_run_path).is_file() {
        return false;
    }
    if let Ok(p) = crate::proton::resolve_umu_run(None) {
        data.launch.umu_run_path = p.to_string_lossy().to_string();
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

/// Migrates pre-rclone-control-plane configs onto `cloud_base_path`.
///
/// Older versions stored the exact ludusavi remote subpath in `cloud_path`
/// (a user-editable Settings field) and had no `cloud_base_path`. With the
/// container-level `#[serde(default)]`, such a config loads with
/// `cloud_base_path` at its default `"Spool"` — so a user who'd customized
/// `cloud_path` would silently have their remote folder switched to
/// `Spool/ludusavi-backup`, hiding their existing saves. When the default
/// base no longer matches a non-default `cloud_path`, derive the base from
/// `cloud_path` (stripping the conventional `/ludusavi-backup` leaf) and
/// normalize `cloud_path` to the canonical derived value so this never fires
/// again. Idempotent and inert for new installs (base already non-default, or
/// `cloud_path` already canonical). Returns true if anything changed.
fn migrate_cloud_base_path(data: &mut ConfigData) -> bool {
    const DEFAULT_BASE: &str = "Spool";
    let old_path = data.cloud.path.trim().trim_end_matches('/').to_string();
    if old_path.is_empty() || data.cloud.base_path.trim() != DEFAULT_BASE {
        // No legacy path to migrate, or the base was already set explicitly
        // (new-scheme config) — leave it alone.
        return false;
    }
    let canonical = format!("{DEFAULT_BASE}/ludusavi-backup");
    if old_path == canonical {
        // The default path — nothing to preserve.
        return false;
    }
    let base = old_path
        .strip_suffix("/ludusavi-backup")
        .unwrap_or(&old_path)
        .trim_end_matches('/');
    if base.is_empty() || base == DEFAULT_BASE {
        return false;
    }
    data.cloud.base_path = base.to_string();
    data.cloud.path = format!("{base}/ludusavi-backup");
    tracing::info!(base, "migrated legacy cloud_path to cloud_base_path");
    true
}

/// Marks a pre-existing config as having finished onboarding.
///
/// The first-run onboarding flow shows whenever `onboarding_completed` is
/// false. A fresh install has no config file at all, so it correctly starts
/// false and the flow runs. But a config written by a Spool build that predates
/// this field would also deserialize to `false` (via the container-level
/// `#[serde(default)]`) — and we don't want to throw the first-run flow at a
/// returning user. Such legacy files are recognised by the
/// `onboarding_completed` key being *absent* from the raw JSON; when it's
/// missing we flip the flag to true. Brand-new installs write the key
/// explicitly (as false), so it's present on every subsequent load and this
/// never fires for them. Returns true if it changed anything.
fn migrate_onboarding_completed(raw_json: Option<&str>, data: &mut ConfigData) -> bool {
    if data.onboarding_completed {
        return false;
    }
    let Some(json) = raw_json else {
        // No file on disk — a genuine first run. Leave it false so the flow shows.
        return false;
    };
    let key_present = serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.as_object().map(|o| o.contains_key("onboarding_completed")))
        .unwrap_or(false);
    if key_present {
        return false;
    }
    // Legacy config without the key — a returning user. Skip onboarding.
    data.onboarding_completed = true;
    tracing::info!("marked pre-existing config as onboarding-completed");
    true
}

fn hostname() -> String {
    // The OS hostname via gethostname(2) (GetComputerNameExW on Windows) is the
    // authoritative source on both platforms. The env vars are only a fallback:
    // $HOSTNAME / $HOST are shell variables that interactive shells set but don't
    // export, so a GUI / Game-Mode launch sees neither — which is why Linux used
    // to land on the "Spool device" fallback every time. %COMPUTERNAME% is a real
    // process env var on Windows, kept as a secondary path. Final fallback is a
    // literal so device identity is never empty.
    let from_os = gethostname::gethostname().to_string_lossy().trim().to_string();
    if !from_os.is_empty() {
        return from_os;
    }
    env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .or_else(|_| env::var("HOST"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Spool device".to_string())
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

    // Sync cloud/rclone settings to Spool-owned ludusavi config.yaml. The
    // ludusavi remote subpath is derived from the base folder so it always sits
    // beside Spool's `_spool` control-plane dir. Use the same normalizer the
    // control plane uses so a blank base folder can't put saves under
    // `/ludusavi-backup` while `_spool` falls back to `Spool/_spool`.
    let ludusavi_remote_path = format!("{}/ludusavi-backup", crate::rclone::base_path(&cfg.data));
    let _ = crate::ludusavi_config::set_cloud(
        Some(&cfg.data.cloud.provider),
        Some(&cfg.data.cloud.remote),
        Some(&ludusavi_remote_path),
        None, // rclone path stays as "rclone"; resolved via PATH at run_api spawn time
        Some(&cfg.data.cloud.rclone_args),
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
    Ok(cfg.data.launch.umu_run_path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pre-grouping config.json (a subset of the flat keys) must still load:
    /// present keys map into their sub-structs, missing keys fall back to each
    /// sub-struct's defaults. This is the `#[serde(flatten)]` + `default`
    /// contract the on-disk + frontend compatibility relies on.
    #[test]
    fn old_flat_config_loads_into_subgroups() {
        let json = r#"{
            "cloud_provider": "dropbox",
            "cloud_webdav_url": "https://dav.example.com",
            "lan_share_port": 12345,
            "umu_run_path": "/usr/bin/umu-run"
        }"#;
        let data: ConfigData = serde_json::from_str(json).unwrap();
        assert_eq!(data.cloud.provider, "dropbox");
        assert_eq!(data.cloud.webdav_url, "https://dav.example.com");
        assert_eq!(data.lan.share_port, 12345);
        assert_eq!(data.launch.umu_run_path, "/usr/bin/umu-run");
        // Everything absent from the JSON gets its sub-struct default.
        assert_eq!(data.cloud.base_path, "Spool");
        assert_eq!(data.cloud.rclone_args, "--fast-list --ignore-checksum");
        assert!(data.lan.share_enabled);
        assert_eq!(data.lan.download_max_mbps, 0.0);
        assert_eq!(data.save_retention_full, 3);
    }

    /// The grouping is internal: the serialized JSON stays flat (the historical
    /// `cloud_*` / `lan_*` / `umu_run_path` keys), and the Rust sub-struct names
    /// (`cloud`, `lan`, `launch`) never appear on the wire.
    #[test]
    fn serializes_to_flat_keys() {
        let json = serde_json::to_value(ConfigData::default()).unwrap();
        let obj = json.as_object().unwrap();
        for key in [
            "cloud_provider", "cloud_remote", "cloud_base_path", "cloud_path",
            "rclone_args", "cloud_webdav_url", "cloud_webdav_username",
            "lan_share_enabled", "lan_share_port", "lan_install_dir",
            "lan_download_max_mbps", "umu_run_path", "default_proton_path",
        ] {
            assert!(obj.contains_key(key), "missing flat key: {key}");
        }
        assert!(!obj.contains_key("cloud"));
        assert!(!obj.contains_key("lan"));
        assert!(!obj.contains_key("launch"));
    }

    #[test]
    fn legacy_config_without_onboarding_key_is_marked_completed() {
        // A config that predates the onboarding flag (key absent) is a
        // returning user — migrate to completed so the flow doesn't reappear.
        let json = r#"{ "device_id": "abc", "tray_intro_seen": true }"#;
        let mut data: ConfigData = serde_json::from_str(json).unwrap();
        assert!(!data.onboarding_completed); // serde default before migration
        assert!(migrate_onboarding_completed(Some(json), &mut data));
        assert!(data.onboarding_completed);
    }

    #[test]
    fn fresh_install_keeps_onboarding_pending() {
        // No file on disk (None) → genuine first run, flag stays false.
        let mut data = ConfigData::default();
        assert!(!migrate_onboarding_completed(None, &mut data));
        assert!(!data.onboarding_completed);
    }

    #[test]
    fn explicit_onboarding_false_is_left_pending() {
        // A new-scheme config that wrote the key as false (e.g. relaunched
        // mid-onboarding) keeps showing the flow — the key is present.
        let json = r#"{ "device_id": "abc", "onboarding_completed": false }"#;
        let mut data: ConfigData = serde_json::from_str(json).unwrap();
        assert!(!migrate_onboarding_completed(Some(json), &mut data));
        assert!(!data.onboarding_completed);
    }

    #[test]
    fn round_trips_through_json() {
        let original = ConfigData {
            cloud: CloudConfig {
                provider: "gdrive".to_string(),
                base_path: "Games/Spool".to_string(),
                ..CloudConfig::default()
            },
            lan: LanConfig {
                share_port: 50000,
                download_max_mbps: 12.5,
                ..LanConfig::default()
            },
            launch: LaunchConfig {
                default_proton_path: "/opt/proton".to_string(),
                ..LaunchConfig::default()
            },
            ..ConfigData::default()
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ConfigData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cloud.provider, "gdrive");
        assert_eq!(parsed.cloud.base_path, "Games/Spool");
        assert_eq!(parsed.lan.share_port, 50000);
        assert_eq!(parsed.lan.download_max_mbps, 12.5);
        assert_eq!(parsed.launch.default_proton_path, "/opt/proton");
    }
}

//! Run workflow — the marquee feature.
//!
//! Orchestrates the five-phase game launch:
//!
//!   restoring → launching → playing → backing-up → done
//!
//! Each transition emits a `run:phase` event so the UI can update the
//! Play button label. Cloud-sync conflicts during restore are surfaced
//! as an error phase and the launch aborts (we won't blindly overwrite
//! the user's cloud save). Backup failures after a successful play
//! session are logged but don't fail the workflow — the game ran fine
//! and the user shouldn't see a red toast for a flaky network call.
//!
//! Single-launch model: only one game can be running at a time. A second
//! `launch_game` while a workflow is in flight returns an error rather
//! than overlapping. This matches the cassette-shelf metaphor (one tape
//! in the deck) and avoids two restores trampling the same save dir.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use crate::ludusavi::LudusaviClient;
use crate::ludusavi_config;
use crate::redirects;
use crate::sync::{self, AcquireOutcome};
use crate::{process, paths, registry};
use chrono::Utc;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

// `paths` import retained for future log-file work; not used yet.
#[allow(unused_imports)]
use paths as _paths;

/// Shared runner state. v1 only tracks "is anything running?" — when we
/// allow concurrent launches for different games this becomes a HashMap.
#[derive(Default)]
pub struct RunState {
    current: Mutex<Option<String>>,
}

impl RunState {
    fn try_acquire(&self, game_id: &str) -> AppResult<RunGuard<'_>> {
        let mut guard = self.current.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(running) = guard.as_ref() {
            return Err(AppError::Other(format!(
                "Another game is already running (id {running})"
            )));
        }
        *guard = Some(game_id.to_string());
        Ok(RunGuard { state: self })
    }
}

/// RAII guard — drops the running-id when the workflow finishes (or
/// panics). Without this a crashed workflow would leave the slot occupied
/// and the user could never launch another game until they restarted Spool.
struct RunGuard<'a> {
    state: &'a RunState,
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.state.current.lock() {
            *guard = None;
        }
    }
}

#[derive(Debug, Serialize, Clone)]
struct RunPhaseEvent {
    game_id: String,
    phase: String,
    message: Option<String>,
    /// True when a cloud remote is configured and this session synced
    /// (or attempted to sync) with it. False for local-only sessions.
    cloud_used: bool,
    /// Duration of the play session in minutes. Set on `backing-up` and
    /// `done` phases (after the game exits); null for pre-exit phases.
    session_minutes: Option<i32>,
    /// True when the local backup succeeded but the cloud upload failed.
    /// Only ever true on the `done` phase. The save is safe on disk —
    /// the UI should show the cloud leg as amber, not red.
    cloud_upload_failed: bool,
}

fn emit_phase(
    app: &AppHandle,
    game_id: &str,
    phase: &str,
    message: Option<&str>,
    cloud_used: bool,
    session_minutes: Option<i32>,
    cloud_upload_failed: bool,
) {
    let payload = RunPhaseEvent {
        game_id: game_id.to_string(),
        phase: phase.to_string(),
        message: message.map(String::from),
        cloud_used,
        session_minutes,
        cloud_upload_failed,
    };
    // Log every transition so a Game-Mode launch is diagnosable from
    // debug.log alone — confirms the workflow advanced past "restoring"
    // and shows how long each phase took (e.g. a slow cloud-sync restore).
    tracing::info!(game_id, phase, ?message, "run:phase");
    if let Err(e) = app.emit("run:phase", &payload) {
        tracing::warn!(phase = phase, error = %e, "failed to emit run:phase");
    }
}

/// Fires a native OS notification, but only when the main Spool
/// window isn't visible — otherwise we'd double up with the in-app
/// toasts the frontend renders for the same run-phase events.
///
/// The intent is: while the user is in-game (Spool hidden / tray-
/// resident), they get desktop toasts in the OS notification centre
/// for things they need to know about ("saves backed up", "launch
/// failed"). While Spool's window is in focus they see the in-app
/// toast instead.
///
/// Best-effort: a notification failure is logged and otherwise
/// ignored — never blocks the workflow.
fn os_toast_if_hidden(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;

    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        return;
    }

    if let Err(e) = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        tracing::warn!(error = %e, "OS toast failed");
    }
}

/// AppHandle-free backup core. Resolves the game's name + wine prefix from the
/// library, runs `ludusavi backup`, and persists the entry's backup stats.
/// Returns the bundle count + total bytes. Callers handle event emission and
/// sync-server recording (best-effort) themselves.
pub async fn backup_game_core(
    ludusavi_client: &LudusaviClient,
    ludusavi_exe: &Path,
    config_dir: &Path,
    library: &SharedLibrary,
    game_id: &str,
) -> AppResult<ManualBackupResult> {
    let (game_name, use_proton, prefix_override) = {
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.game_name.clone(),
            entry.use_proton,
            entry.wine_prefix_path.clone(),
        )
    };
    let wine_prefix: Option<PathBuf> = if cfg!(not(windows)) && use_proton {
        Some(
            prefix_override
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| crate::proton::game_prefix_path(game_id)),
        )
    } else {
        None
    };

    let out = ludusavi_client
        .backup(ludusavi_exe, config_dir, &game_name, wine_prefix.as_deref())
        .await
        .map_err(|e| AppError::Other(format!("ludusavi backup: {e}")))?;

    let (game_count, bytes_total) = out
        .overall
        .as_ref()
        .map(|o| (o.total_games, o.total_bytes))
        .unwrap_or((0, 0));

    if game_count > 0 {
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
            entry.save_backup_count += 1;
            entry.save_last_backed_up_at = Some(Utc::now());
            entry.save_backup_size_mb = (bytes_total as f64) / (1024.0 * 1024.0);
            entry.sync_badge = Some("synced".to_string());
        }
        lib.save()?;
    }

    Ok(ManualBackupResult {
        game_count,
        bytes_total,
    })
}

/// Manual backup — runs `ludusavi backup` for a single game outside
/// the full play workflow. Used by the right-click "Back up saves
/// now" action so users can snapshot saves before risky operations
/// without launching the game.
///
/// Returns the count of backup bundles ludusavi produced and the
/// total size in bytes. Persists the entry's
/// `save_last_backed_up_at` / `save_backup_count` / `save_backup_size_mb`
/// the same way the post-session backup phase does, then records a
/// sync event so peers can see the new save.
#[tauri::command]
pub async fn manual_backup(app: AppHandle, game_id: String) -> AppResult<ManualBackupResult> {
    let ludusavi_exe = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        crate::paths::resolve_ludusavi_path(&cfg.data.ludusavi_path).ok_or_else(|| {
            AppError::Other(
                "Ludusavi is not configured. Place ludusavi in your PATH or configure it in Settings.".into(),
            )
        })?
    };
    let config_dir = crate::paths::ludusavi_config_dir();
    let ludusavi_client = app.state::<LudusaviClient>();
    let library = app.state::<SharedLibrary>();

    let result =
        backup_game_core(&ludusavi_client, &ludusavi_exe, &config_dir, &library, &game_id).await?;

    if result.game_count > 0 {
        let _ = app.emit("library:changed", &game_id);
        let game_name = {
            let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
            lib.find(&game_id).map(|e| e.game_name.clone())
        };
        if let Some(name) = game_name {
            // Record on the sync server so peers see the new event.
            sync::record_backup_event(&app, &name).await;
        }
    }
    Ok(result)
}

/// Manual restore — runs `ludusavi restore` for a single game.
/// Surfaces cloud-sync conflicts as an explicit error so the UI can
/// prompt the user to open Ludusavi (same behaviour as the launch
/// path).
#[tauri::command]
pub async fn manual_restore(app: AppHandle, game_id: String) -> AppResult<ManualRestoreResult> {
    let (game_name, ludusavi_exe, config_dir, wine_prefix) = manual_prep(&app, &game_id)?;
    let game_folder = {
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        lib.find(&game_id)
            .and_then(|e| e.game_folder_path.as_ref().map(PathBuf::from))
    };
    let ludusavi_client = app.state::<LudusaviClient>();
    let out = restore_with_redirects(
        &ludusavi_client,
        &ludusavi_exe,
        &config_dir,
        &game_name,
        wine_prefix.as_deref(),
        game_folder.as_deref(),
    )
    .await
    .map_err(|e| AppError::Other(format!("ludusavi restore: {e}")))?;

    if out
        .errors
        .as_ref()
        .and_then(|e| e.cloud_conflict.as_ref())
        .is_some()
    {
        return Err(AppError::Other(
            "Cloud sync conflict — open Ludusavi to resolve before restoring.".into(),
        ));
    }

    let game_count = out
        .overall
        .as_ref()
        .map(|o| o.total_games)
        .unwrap_or(0);

    // Record the restore on the sync server so peers know we just
    // pulled the latest. Best-effort.
    if game_count > 0 {
        sync::record_restore_event(&app, &game_name).await;
    }

    Ok(ManualRestoreResult { game_count })
}

/// Resolve a cloud-sync conflict in-app, then land the reconciled saves.
///
/// `side` is the frontend's choice of which copy wins:
///   `"local"` → keep this device's saves (`cloud upload --force`)
///   `"cloud"` → keep the cloud's saves (`cloud download --force`)
///
/// Flow:
///   1. `ludusavi cloud {upload,download} --force <game>` — mirrors the chosen
///      side over the loser, which is what clears the `cloudConflict` guard.
///   2. A normal restore (with cross-platform redirects) — lands the now-
///      reconciled backup into the live save location so the game is ready to
///      launch, and confirms the conflict is actually gone.
///
/// The follow-up launch (frontend re-triggers `launch_game` on success) then
/// restores idempotently and proceeds without a conflict. Used by the in-app
/// Cloud Save Conflict resolver, replacing the "Open Ludusavi" hop-out.
#[tauri::command]
pub async fn resolve_cloud_conflict(
    app: AppHandle,
    game_id: String,
    side: String,
) -> AppResult<ManualRestoreResult> {
    let op = crate::ludusavi::CloudOp::from_side(&side)?;
    let (game_name, ludusavi_exe, config_dir, wine_prefix) = manual_prep(&app, &game_id)?;
    let game_folder = {
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        lib.find(&game_id)
            .and_then(|e| e.game_folder_path.as_ref().map(PathBuf::from))
    };
    let ludusavi_client = app.state::<LudusaviClient>();

    // ── Step 1: mirror the chosen side, clearing the conflict ─────────────
    tracing::info!(game_name, ?op, "resolving cloud conflict");
    ludusavi_client
        .cloud_resolve(&ludusavi_exe, &config_dir, op, &game_name)
        .await
        .map_err(|e| AppError::Other(format!("ludusavi cloud {side}: {e}")))?;

    // ── Step 2: restore the reconciled backup into the live save location ─
    let out = restore_with_redirects(
        &ludusavi_client,
        &ludusavi_exe,
        &config_dir,
        &game_name,
        wine_prefix.as_deref(),
        game_folder.as_deref(),
    )
    .await
    .map_err(|e| AppError::Other(format!("ludusavi restore: {e}")))?;

    // A conflict here would mean the mirror didn't take — surface it rather
    // than silently launching with mismatched saves.
    if out
        .errors
        .as_ref()
        .and_then(|e| e.cloud_conflict.as_ref())
        .is_some()
    {
        return Err(AppError::Other(
            "Cloud sync conflict persisted after resolving — open Ludusavi to inspect.".into(),
        ));
    }

    let game_count = out.overall.as_ref().map(|o| o.total_games).unwrap_or(0);
    if game_count > 0 {
        sync::record_restore_event(&app, &game_name).await;
    }

    Ok(ManualRestoreResult { game_count })
}

#[derive(Debug, Clone, Serialize)]
pub struct RawSaveDetails {
    pub modified: Option<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawConflictDetails {
    pub local: Option<RawSaveDetails>,
    pub cloud: Option<RawSaveDetails>,
}

fn get_local_backup_details(game_name: &str) -> Option<RawSaveDetails> {
    let backup_dir = ludusavi_config::backup_dir();
    let candidates = [
        backup_dir.join(game_name),
        backup_dir.join(redirects::windows_safe_name(game_name)),
    ];
    for dir in &candidates {
        if dir.is_dir() {
            let mapping_file = dir.join("mapping.yaml");
            let mut modified = if mapping_file.is_file() {
                std::fs::metadata(&mapping_file)
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|sys_time| {
                        let dt: chrono::DateTime<chrono::Utc> = chrono::DateTime::from(sys_time);
                        dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    })
            } else {
                None
            };
            
            let mut total_size = 0u64;
            for entry in walkdir::WalkDir::new(dir).follow_links(true) {
                let Ok(entry) = entry else { continue };
                if !entry.file_type().is_file() {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    total_size += meta.len();
                    if modified.is_none() {
                        if let Ok(mod_time) = meta.modified() {
                            let dt: chrono::DateTime<chrono::Utc> = chrono::DateTime::from(mod_time);
                            modified = Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
                        }
                    } else if let Ok(mod_time) = meta.modified() {
                        let dt: chrono::DateTime<chrono::Utc> = chrono::DateTime::from(mod_time);
                        let dt_str = dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                        if let Some(ref m) = modified {
                            if &dt_str > m {
                                modified = Some(dt_str);
                            }
                        }
                    }
                }
            }
            if modified.is_some() || total_size > 0 {
                return Some(RawSaveDetails {
                    modified,
                    size_bytes: total_size,
                });
            }
        }
    }
    None
}

async fn get_local_active_save_details(
    ludusavi_exe: &Path,
    config_dir: &Path,
    game_name: &str,
    wine_prefix: Option<&Path>,
) -> Option<RawSaveDetails> {
    let mut args = vec!["backup", "--preview", "--api", game_name];
    let prefix_str;
    if let Some(pfx) = wine_prefix {
        prefix_str = pfx.to_string_lossy().into_owned();
        args.push("--wine-prefix");
        args.push(&prefix_str);
    }
    
    let mut cmd = tokio::process::Command::new(ludusavi_exe);
    cmd.arg("--config").arg(config_dir);
    cmd.args(&args);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        #[allow(unused_imports)]
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    
    let child = cmd.spawn().ok()?;
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(6),
        child.wait_with_output(),
    )
    .await
    .ok()? // timeout
    .ok()?; // process run error
    
    if !output.status.success() {
        tracing::warn!(
            "get_local_active_save_details: ludusavi preview failed with status {:?}. Stderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }
    
    #[derive(Debug, serde::Deserialize)]
    struct LocalPreviewFile {
        bytes: u64,
    }
    
    #[derive(Debug, serde::Deserialize)]
    struct LocalPreviewGame {
        #[serde(default)]
        files: std::collections::HashMap<String, LocalPreviewFile>,
    }
    
    #[derive(Debug, serde::Deserialize)]
    struct LocalPreviewOutput {
        #[serde(default)]
        games: std::collections::HashMap<String, LocalPreviewGame>,
    }
    
    let parsed: LocalPreviewOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| {
            tracing::error!(
                "get_local_active_save_details: failed to parse ludusavi output: {:?}. Output: {}",
                e,
                String::from_utf8_lossy(&output.stdout)
            );
            e
        })
        .ok()?;
        
    let mut total_size = 0u64;
    let mut modified: Option<String> = None;
    
    for game in parsed.games.values() {
        for (path_str, file_info) in &game.files {
            total_size += file_info.bytes;
            let path = Path::new(path_str);
            if let Ok(meta) = std::fs::metadata(path) {
                if let Ok(mod_time) = meta.modified() {
                    let dt: chrono::DateTime<chrono::Utc> = chrono::DateTime::from(mod_time);
                    let dt_str = dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                    if let Some(ref m) = modified {
                        if &dt_str > m {
                            modified = Some(dt_str);
                        }
                    } else {
                        modified = Some(dt_str);
                    }
                }
            }
        }
    }
    
    if total_size > 0 {
        tracing::info!(
            "get_local_active_save_details: found active local saves. size={}, modified={:?}",
            total_size,
            modified
        );
        Some(RawSaveDetails {
            modified,
            size_bytes: total_size,
        })
    } else {
        None
    }
}


fn get_rclone_remote_name(config: &serde_yaml::Value) -> Option<String> {
    let cloud = config.get("cloud")?;
    let remote = cloud.get("remote")?;
    match remote {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Mapping(m) => {
            if let Some(custom) = m.get(&serde_yaml::Value::String("Custom".into())) {
                if let Some(id) = custom.get(&serde_yaml::Value::String("id".into())) {
                    return id.as_str().map(String::from);
                }
            }
            if let Some(webdav) = m.get(&serde_yaml::Value::String("WebDav".into())) {
                if let Some(id) = webdav.get(&serde_yaml::Value::String("id".into())) {
                    return id.as_str().map(String::from);
                }
            }
            None
        }
        _ => None,
    }
}

async fn query_rclone_details(
    rclone_exe: &Path,
    remote_name: &str,
    remote_path: &str,
    game_folder_name: &str,
) -> Option<RawSaveDetails> {
    let target = format!("{}:{}/{}", remote_name, remote_path, game_folder_name);
    tracing::info!("query_rclone_details: target={}", target);
    
    let mut cmd = tokio::process::Command::new(rclone_exe);
    cmd.arg("lsjson")
        .arg("--no-mimetype")
        .arg("--recursive")
        .arg(&target);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        #[allow(unused_imports)]
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("query_rclone_details: failed to spawn rclone: {:?}", e);
            return None;
        }
    };
    
    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(6),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            tracing::error!("query_rclone_details: rclone process run error: {:?}", e);
            return None;
        }
        Err(_) => {
            tracing::warn!("query_rclone_details: rclone command timed out");
            return None;
        }
    };
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            "query_rclone_details: rclone failed with status {:?}. Stderr: {}",
            output.status.code(),
            stderr.trim()
        );
        return None;
    }
    
    #[derive(Debug, serde::Deserialize)]
    struct RcloneItem {
        #[serde(rename = "Size")]
        size: i64,
        #[serde(rename = "ModTime")]
        mod_time: String,
        #[serde(rename = "IsDir")]
        is_dir: bool,
    }
    
    let items: Vec<RcloneItem> = match serde_json::from_slice(&output.stdout) {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::error!(
                "query_rclone_details: failed to deserialize JSON from rclone: {:?}. Output: {}",
                e,
                String::from_utf8_lossy(&output.stdout)
            );
            return None;
        }
    };
    
    if items.is_empty() {
        tracing::info!("query_rclone_details: target contains no files");
        return None;
    }
    
    let total_size: u64 = items
        .iter()
        .filter(|i| !i.is_dir)
        .map(|i| i.size.max(0) as u64)
        .sum();
    let latest_mod = items
        .iter()
        .filter(|i| !i.is_dir)
        .map(|i| &i.mod_time)
        .max()
        .cloned();
        
    tracing::info!(
        "query_rclone_details success: files_count={}, total_size={}, latest_mod={:?}",
        items.iter().filter(|i| !i.is_dir).count(),
        total_size,
        latest_mod
    );
    
    Some(RawSaveDetails {
        modified: latest_mod,
        size_bytes: total_size,
    })
}

#[tauri::command]
pub async fn get_cloud_conflict_details(
    app: AppHandle,
    game_id: String,
) -> AppResult<RawConflictDetails> {
    tracing::info!("get_cloud_conflict_details called for game_id={}", game_id);
    // 1. Get local details
    let (game_name, ludusavi_exe, config_dir, wine_prefix) = manual_prep(&app, &game_id)?;
    let mut local = get_local_active_save_details(&ludusavi_exe, &config_dir, &game_name, wine_prefix.as_deref()).await;
    if local.is_none() {
        tracing::info!("get_local_active_save_details returned None; falling back to local backup directory stats");
        local = get_local_backup_details(&game_name);
    }
    tracing::info!("local details for {}: {:?}", game_name, local);
    
    // 2. Get cloud details if cloud is configured
    let config_file = crate::paths::ludusavi_config_file();
    if !config_file.exists() {
        tracing::warn!("get_cloud_conflict_details: config.yaml does not exist at {:?}", config_file);
        return Ok(RawConflictDetails { local, cloud: None });
    }
    let raw = std::fs::read_to_string(&config_file)
        .map_err(|e| AppError::Other(format!("failed to read config.yaml: {e}")))?;
    let config: serde_yaml::Value = serde_yaml::from_str(&raw)
        .map_err(|e| AppError::Other(format!("failed to parse config.yaml: {e}")))?;
        
    let Some(remote_name) = get_rclone_remote_name(&config) else {
        tracing::warn!("get_cloud_conflict_details: cloud remote is not configured in config.yaml");
        return Ok(RawConflictDetails { local, cloud: None });
    };
    
    let remote_path = config
        .get("cloud")
        .and_then(|c| c.get("path"))
        .and_then(|p| p.as_str())
        .unwrap_or("ludusavi-backup");
        
    let rclone_exe = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        crate::paths::resolve_rclone_path(&cfg.data.rclone_path).ok_or_else(|| {
            AppError::Other("rclone binary not found".into())
        })?
    };
    
    tracing::info!(
        "get_cloud_conflict_details: querying rclone_exe={:?}, remote_name={}, remote_path={}",
        rclone_exe,
        remote_name,
        remote_path
    );
    
    // Query cloud remote (try exact name first, then windows safe name)
    let mut cloud = query_rclone_details(&rclone_exe, &remote_name, remote_path, &game_name).await;
    if cloud.is_none() {
        let safe_name = redirects::windows_safe_name(&game_name);
        if safe_name != game_name {
            tracing::info!(
                "get_cloud_conflict_details: exact name failed, retrying with windows safe name: {}",
                safe_name
            );
            cloud = query_rclone_details(&rclone_exe, &remote_name, remote_path, &safe_name).await;
        }
    }
    
    tracing::info!("get_cloud_conflict_details results: local={:?}, cloud={:?}", local, cloud);
    Ok(RawConflictDetails { local, cloud })
}

#[derive(Debug, Serialize)]
pub struct ManualBackupResult {
    pub game_count: i32,
    pub bytes_total: u64,
}

#[derive(Debug, Serialize)]
pub struct ManualRestoreResult {
    pub game_count: i32,
}

/// Snapshot for the manual backup/restore commands. Returns:
///   (game_name, ludusavi_exe, config_dir, wine_prefix)
///
/// `wine_prefix` is `Some` only on non-Windows when the game has `use_proton`
/// set; it is the prefix ROOT (not drive_c) passed as `--wine-prefix` to
/// backup. Restore never takes a prefix — cross-device remapping is handled
/// by redirects (Phase 3).
fn manual_prep(app: &AppHandle, game_id: &str) -> AppResult<(String, PathBuf, PathBuf, Option<PathBuf>)> {
    let (game_name, use_proton, prefix_override) = {
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.game_name.clone(),
            entry.use_proton,
            entry.wine_prefix_path.clone(),
        )
    };
    let ludusavi_exe = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        crate::paths::resolve_ludusavi_path(&cfg.data.ludusavi_path).ok_or_else(|| {
            AppError::Other(
                "Ludusavi is not configured. Place ludusavi in your PATH or configure it in Settings.".into(),
            )
        })?
    };
    let config_dir = crate::paths::ludusavi_config_dir();
    let wine_prefix = if cfg!(not(windows)) && use_proton {
        Some(
            prefix_override
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| crate::proton::game_prefix_path(game_id)),
        )
    } else {
        None
    };
    Ok((game_name, ludusavi_exe, config_dir, wine_prefix))
}

#[tauri::command]
pub async fn launch_game(app: AppHandle, game_id: String) -> AppResult<()> {
    launch_game_inner(&app, &game_id).await
}

/// Inner launch function callable from non-command contexts (e.g. the
/// `tauri-plugin-single-instance` callback when a forwarded `--run` arrives).
/// Same behaviour as the `launch_game` command — single-launch guard +
/// full workflow + phase emission.
pub async fn launch_game_inner(app: &AppHandle, game_id: &str) -> AppResult<()> {
    let run_state = app.state::<RunState>();
    let _guard = run_state.try_acquire(game_id)?;

    // Snapshot what we need from state up front so we don't hold any
    // sync Mutex across the long-running awaits below. We also fold
    // the registry-level Run-As-Admin compat flag into the effective
    // `needs_admin` here so the launch path doesn't have to know
    // about the registry concept.
    let (game_name, exe_path, needs_admin, use_proton, proton_version_path, wine_prefix_path, launch_args) = {
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        if entry.exe_path.is_empty() {
            return Err(AppError::Other("Game has no executable configured".into()));
        }
        let needs_admin =
            entry.run_as_admin || registry::run_as_admin_in_registry(&entry.exe_path);
        (
            entry.game_name.clone(),
            entry.exe_path.clone(),
            needs_admin,
            entry.use_proton,
            entry.proton_version_path.clone(),
            entry.wine_prefix_path.clone(),
            entry.launch_args.clone(),
        )
    };

    let ludusavi_exe = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        crate::paths::resolve_ludusavi_path(&cfg.data.ludusavi_path).ok_or_else(|| {
            AppError::Other(
                "Ludusavi is not configured. Place ludusavi in your PATH or configure it in Settings.".into(),
            )
        })?
    };

    let (umu_run_path, default_proton_path) = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        (
            cfg.data.umu_run_path.clone(),
            cfg.data.default_proton_path.clone(),
        )
    };

    // Resolve the launch plan (umu-run + Proton paths) *before* the long
    // awaits below so a misconfiguration surfaces as a clean launch error.
    let launch_plan = build_launch_plan(
        game_id,
        use_proton,
        proton_version_path,
        wine_prefix_path,
        launch_args,
        needs_admin,
        &umu_run_path,
        &default_proton_path,
        &exe_path,
    )?;

    let ludusavi_client = app.state::<LudusaviClient>();
    let result = run_workflow(
        app,
        game_id,
        &game_name,
        &exe_path,
        &launch_plan,
        &ludusavi_exe,
        &ludusavi_client,
    )
    .await;

    if let Err(e) = &result {
        emit_phase(app, game_id, "error", Some(&e.to_string()), false, None, false);
        // Surface the failure via the OS notification centre too —
        // most workflow errors happen while the user is mid-launch
        // with Spool tucked into the tray.
        os_toast_if_hidden(
            app,
            "Spool: launch failed",
            &format!("{game_name} — {e}"),
        );
    }
    result
}

/// Fully-resolved instructions for spawning a game, built once (before the
/// async workflow) so any Proton/umu misconfiguration fails fast.
struct LaunchPlan {
    use_proton: bool,
    umu_run: Option<PathBuf>,
    proton_path: Option<PathBuf>,
    prefix_root: PathBuf,
    extra_args: Vec<String>,
    run_as_admin: bool,
}

/// Resolves a [`LaunchPlan`] from the game's settings + app config. On
/// non-Windows, a `.exe` without Proton enabled is a hard error (we won't
/// try to exec a Windows binary natively). On Windows, Proton is ignored.
#[allow(clippy::too_many_arguments)]
fn build_launch_plan(
    game_id: &str,
    use_proton: bool,
    proton_version_path: Option<String>,
    wine_prefix_path: Option<String>,
    launch_args: Option<String>,
    needs_admin: bool,
    umu_run_path: &str,
    default_proton_path: &str,
    exe_path: &str,
) -> AppResult<LaunchPlan> {
    let prefix_root = wine_prefix_path
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::proton::game_prefix_path(game_id));
    let extra_args: Vec<String> = launch_args
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(String::from)
        .collect();

    let effective_proton = use_proton && cfg!(not(windows));

    if effective_proton {
        let umu_run = crate::proton::resolve_umu_run(Some(umu_run_path))?;
        let proton_path = crate::proton::resolve_proton_path(
            proton_version_path.as_deref(),
            Some(default_proton_path),
        )?;
        return Ok(LaunchPlan {
            use_proton: true,
            umu_run: Some(umu_run),
            proton_path: Some(proton_path),
            prefix_root,
            extra_args,
            run_as_admin: false,
        });
    }

    // Native path. Guard against trying to run a Windows exe natively on Linux.
    if cfg!(not(windows)) && exe_path.to_ascii_lowercase().ends_with(".exe") {
        return Err(AppError::Other(
            "This is a Windows game — enable 'Run with Proton' in the game's Launch settings.".into(),
        ));
    }

    Ok(LaunchPlan {
        use_proton: false,
        umu_run: None,
        proton_path: None,
        prefix_root,
        extra_args,
        run_as_admin: needs_admin,
    })
}

/// Run a ludusavi restore with automatic cross-platform redirect generation.
///
/// Flow:
///  1. Restore once — this pulls the latest cloud backup (via `--cloud-sync`)
///     and lands files at the *recorded* absolute paths.
///  2. Read the backup's `mapping.yaml` to discover the origin OS + paths.
///  3. If the backup is foreign-origin (different OS / different prefix):
///     a. Derive redirect rules (Windows paths → Proton prefix, or reverse).
///     b. Write them into Spool's `config.yaml`.
///     c. Restore *again* — now the redirects steer files to the right place.
///  4. If same-origin: clear any stale redirects (idempotent).
///
/// The double-restore is safe: saves are small, restores are idempotent, and
/// the single-launch lock ensures nothing else is touching the prefix.
///
/// Returns the `ApiOutput` from the effective (second) restore, or the first
/// restore's output if no redirect was needed.
#[allow(clippy::too_many_arguments)]
async fn restore_with_redirects(
    ludusavi_client: &LudusaviClient,
    ludusavi_exe: &Path,
    config_dir: &Path,
    game_name: &str,
    prefix_root: Option<&Path>,
    game_folder: Option<&Path>,
) -> AppResult<crate::ludusavi::ApiOutput> {
    // ── Pass 1: restore (pulls cloud) ─────────────────────────────────────
    let first = ludusavi_client.restore(ludusavi_exe, config_dir, game_name).await?;

    // ── Read mapping.yaml to detect origin ────────────────────────────────
    let backup_dir = ludusavi_config::backup_dir();
    let Some(origin) = redirects::read_backup_origin(&backup_dir, game_name) else {
        // No backup on disk yet (first-ever session). Nothing to redirect.
        tracing::info!(game_name, "no mapping.yaml found — skipping redirect generation");
        return Ok(first);
    };

    tracing::info!(
        game_name,
        origin_os = ?origin.os,
        path_count = origin.paths.len(),
        "mapping.yaml read"
    );

    let local_win_user = redirects::local_windows_username();
    let n = redirects::apply_redirects_for_restore(
        &origin,
        prefix_root,
        game_folder,
        local_win_user.as_deref(),
    )?;

    if n == 0 {
        // Same-origin backup — clear any redirects left from a prior cross-
        // device restore so they don't linger.
        let _ = ludusavi_config::set_redirects(&[]);
        tracing::info!(game_name, "same-origin backup — no redirects needed");
        return Ok(first);
    }

    tracing::info!(
        game_name,
        redirects = n,
        "foreign-origin backup — running second restore with redirects"
    );

    // ── Pass 2: restore with redirects in place ───────────────────────────
    let second = ludusavi_client.restore(ludusavi_exe, config_dir, game_name).await?;

    // Clear redirects after the restore so they don't affect unrelated
    // operations (e.g. a manual backup). We regenerate on every restore.
    let _ = ludusavi_config::set_redirects(&[]);

    Ok(second)
}

async fn run_workflow(
    app: &AppHandle,
    game_id: &str,
    game_name: &str,
    exe_path: &str,
    launch: &LaunchPlan,
    ludusavi_exe: &Path,
    ludusavi_client: &LudusaviClient,
) -> AppResult<()> {
    tracing::info!(game_id, game_name, "starting run workflow");

    let config_dir = crate::paths::ludusavi_config_dir();
    // Wine prefix for backup (Proton games on Linux only).
    let wine_prefix: Option<PathBuf> = if launch.use_proton {
        Some(launch.prefix_root.clone())
    } else {
        None
    };

    // Check once whether a cloud remote is configured so phase messages
    // can tell the user whether saves are cloud-synced or local-only.
    let cloud_configured = ludusavi_config::cloud_remote_is_configured();

    // ── Phase 1: restore ──────────────────────────────────────────────
    let restore_msg = if cloud_configured {
        "Syncing + restoring saves…"
    } else {
        "Restoring local saves…"
    };
    emit_phase(app, game_id, "restoring", Some(restore_msg), cloud_configured, None, false);
    os_toast_if_hidden(
        app,
        "Restoring saves",
        &format!("{game_name} — restoring before launch"),
    );
    tracing::info!(game_name, "ludusavi restore");
    let game_folder = {
        // Snapshot the install folder path for install-dir save redirect (Phase 3).
        let library = app.state::<SharedLibrary>();
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        lib.find(game_id)
            .and_then(|e| e.game_folder_path.as_ref().map(PathBuf::from))
    };
    let restore = restore_with_redirects(
        ludusavi_client,
        ludusavi_exe,
        &config_dir,
        game_name,
        wine_prefix.as_deref(),
        game_folder.as_deref(),
    ).await?;
    if restore
        .errors
        .as_ref()
        .and_then(|e| e.cloud_conflict.as_ref())
        .is_some()
    {
        return Err(AppError::Other(
            "Cloud sync conflict — open Ludusavi to resolve before launching.".into(),
        ));
    }
    // "No saves to restore" is fine — game just hasn't been played yet.
    let no_saves = restore
        .errors
        .as_ref()
        .map(|e| !e.unknown_games.is_empty())
        .unwrap_or(false)
        || restore
            .overall
            .as_ref()
            .map(|o| o.total_games == 0)
            .unwrap_or(false);

    // Record the restore event on the sync server (best-effort, fires
    // only when sync is configured + reachable). The server uses these
    // events to power the cross-device save sync status badges.
    sync::record_restore_event(app, game_name).await;

    // ── Phase 1.5: acquire play-state lock ────────────────────────────
    // Asks the sync server to lock this game to this device so a
    // second device can't simultaneously launch and trample the
    // save. No-op when sync is disabled / unreachable; only blocks on
    // a real 409 conflict.
    match sync::acquire_lock(app, game_name).await {
        AcquireOutcome::Acquired => {}
        AcquireOutcome::Conflict { device_name } => {
            return Err(AppError::Other(format!(
                "Already playing on {device_name}. Close it there before launching here."
            )));
        }
    }

    // ── Phase 2: launch + wait ───────────────────────────────────────
    emit_phase(app, game_id, "launching", Some("Launching game…"), cloud_configured, None, false);
    let exe_pathbuf = PathBuf::from(exe_path);
    if !exe_pathbuf.is_file() {
        return Err(AppError::Other(format!(
            "Game executable not found at {exe_path}"
        )));
    }

    emit_phase(app, game_id, "playing", None, cloud_configured, None, false);
    tracing::info!(exe_path, use_proton = launch.use_proton, "launching game process");
    let session_start = Utc::now();

    // For Proton launches, make sure the prefix root exists; umu/Proton
    // populates it (drive_c, registry) on first run.
    if launch.use_proton {
        if let Err(e) = std::fs::create_dir_all(&launch.prefix_root) {
            return Err(AppError::Other(format!(
                "failed to create Proton prefix dir {:?}: {e}",
                launch.prefix_root
            )));
        }
    }
    let spec = if launch.use_proton {
        process::LaunchSpec::Proton {
            umu_run: launch
                .umu_run
                .as_deref()
                .expect("umu_run resolved for proton launch"),
            prefix_root: &launch.prefix_root,
            proton_path: launch
                .proton_path
                .as_deref()
                .expect("proton_path resolved for proton launch"),
            game_id,
            extra_args: &launch.extra_args,
        }
    } else {
        process::LaunchSpec::Native {
            run_as_admin: launch.run_as_admin,
        }
    };

    // Spawn the lock-heartbeat task. Pings /heartbeat every 30s so
    // the sync server doesn't mark our lock stale during long
    // sessions. Aborted unconditionally on exit so it doesn't
    // outlive the game.
    let heartbeat = sync::start_heartbeat(app.clone(), game_name.to_string());

    let spawn_result = process::run_game(&exe_pathbuf, spec).await;
    let session_end = Utc::now();

    // Always abort the heartbeat + release the lock — even if launch
    // failed mid-spawn. Lock release is fire-and-forget; the server
    // stale-detection would eventually reclaim a missed release but
    // we want the next device to be able to launch immediately.
    heartbeat.abort();
    sync::release_lock(app, game_name).await;

    tracing::info!(
        game_name,
        duration_min = (session_end - session_start).num_minutes(),
        "game exited"
    );

    if let Err(e) = spawn_result {
        return Err(AppError::Other(format!("Game failed to launch: {e}")));
    }

    // ── Update last_played + playtime (best-effort) ───────────────────
    let session_minutes = (session_end - session_start).num_minutes().max(0) as i32;
    {
        let library = app.state::<SharedLibrary>();
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
            entry.last_played_at = Some(session_end);
            entry.playtime_minutes += session_minutes;
        }
        lib.save()?;
    }
    let _ = app.emit("library:changed", &game_id.to_string());

    // Cross-device sync — push the session timestamps to the server
    // so other devices pick them up on their next startup_sync. The
    // timestamp is RFC 3339 / ISO 8601; the server requires the
    // playtime delta to be a positive integer, which `push_playtime_delta`
    // already enforces.
    sync::push_last_played(app, game_name, &session_end.to_rfc3339()).await;
    sync::push_playtime_delta(app, game_name, session_minutes).await;

    // ── Phase 3: backup (skip if ludusavi didn't recognise the game) ──
    // Tracks whether the local backup succeeded but the cloud upload
    // (`--cloud-sync`) failed — we still finish the workflow (the save is
    // safe on disk) but warn the user rather than claiming a clean sync.
    let mut cloud_upload_failed = false;
    if !no_saves {
        let backup_msg = if cloud_configured {
            "Backing up + syncing saves…"
        } else {
            "Backing up locally…"
        };
        emit_phase(app, game_id, "backing-up", Some(backup_msg), cloud_configured, Some(session_minutes), false);
        os_toast_if_hidden(
            app,
            "Backing up saves",
            &format!("{game_name} — session ended"),
        );
        // Phase 3 prelude — canonicalise save paths for Proton games. The
        // restore phase steered a foreign-origin (e.g. Windows) save into the
        // local Proton prefix; without matching backup redirects ludusavi would
        // now record the *local prefix* paths, flipping the backup from Windows
        // paths to Linux paths and breaking the next restore on Windows. Mirror
        // the restore redirects (inverted) so the backup stays portable. Cleared
        // after the backup so they never affect an unrelated operation.
        let mut backup_redirects_set = false;
        if let Some(prefix) = wine_prefix.as_deref() {
            let backup_dir = ludusavi_config::backup_dir();
            if let Some(origin) = redirects::read_backup_origin(&backup_dir, game_name) {
                let local_win_user = redirects::local_windows_username();
                match redirects::apply_redirects_for_backup(
                    &origin,
                    Some(prefix),
                    game_folder.as_deref(),
                    local_win_user.as_deref(),
                ) {
                    Ok(n) if n > 0 => {
                        backup_redirects_set = true;
                        tracing::info!(
                            game_name,
                            redirects = n,
                            "applied backup redirects — storing canonical save paths"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(game_name, error = %e, "failed to apply backup redirects");
                    }
                }
            }
        }

        tracing::info!(game_name, "ludusavi backup");
        let backup_outcome =
            ludusavi_client.backup(ludusavi_exe, &config_dir, game_name, wine_prefix.as_deref()).await;

        // Clear backup redirects regenerated fresh next session — matches the
        // restore phase's clean-up so stale entries can never linger.
        if backup_redirects_set {
            let _ = ludusavi_config::set_redirects(&[]);
        }

        match backup_outcome {
            Ok(out) => {
                // ludusavi reports a cloud-sync failure as a non-fatal field on
                // an otherwise-successful backup (the local snapshot still
                // landed). Surface it — silently swallowing this is what made a
                // dead rclone path / bad WebDAV creds look like "backup
                // succeeded" while nothing reached the remote.
                if out
                    .errors
                    .as_ref()
                    .and_then(|e| e.cloud_sync_failed.as_ref())
                    .is_some()
                {
                    cloud_upload_failed = true;
                    tracing::warn!(
                        game_name,
                        "post-session cloud sync failed — saves backed up locally but not uploaded"
                    );
                }
                if let Some(overall) = &out.overall {
                    if overall.total_games > 0 {
                        let library = app.state::<SharedLibrary>();
                        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
                        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
                            entry.save_backup_count += 1;
                            entry.save_last_backed_up_at = Some(Utc::now());
                            entry.save_backup_size_mb =
                                (overall.total_bytes as f64) / (1024.0 * 1024.0);
                        } else {
                            tracing::warn!(game_id, "backup stats not persisted: library entry missing after session");
                        }
                        lib.save()?;
                        let _ = app.emit("library:changed", &game_id.to_string());
                    }
                }
                // Tell the sync server we backed up — peers can use
                // this to know they're behind on saves. Best-effort.
                sync::record_backup_event(app, game_name).await;

                // Mark the entry as synced. After a successful backup
                // we ARE the most recent device on the server (assuming
                // the event recorded). If the event recording failed
                // silently (offline), the badge will flip to
                // local-newer on the next startup_sync.
                let library = app.state::<SharedLibrary>();
                let badge_changed = if let Ok(mut lib) = library.lock() {
                    let mut changed = false;
                    if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
                        if entry.sync_badge.as_deref() != Some("synced") {
                            entry.sync_badge = Some("synced".to_string());
                            changed = true;
                        }
                    }
                    if changed {
                        let _ = lib.save();
                    }
                    changed
                } else {
                    false
                };
                if badge_changed {
                    let _ = app.emit("library:changed", &game_id.to_string());
                }
            }
            Err(e) => {
                // Don't fail the workflow — the user already played the game
                // successfully and getting a red toast for a flaky network
                // call would be misleading. Surface it in the log instead.
                tracing::warn!(game_id = %game_id, error = %e, "post-session backup failed");
            }
        }
    }

    // Game Mode: flag the active-session record so the Decky plugin's
    // forced-close fallback knows this session already backed up. No-op
    // when there's no record (desktop / Windows launches).
    crate::session::mark_backed_up();

    // Final completion ping — the most useful native toast since the
    // user may have closed the game and walked away from the PC. When the
    // cloud upload failed we carry a message on the `done` phase so the
    // frontend shows a sticky warning toast instead of a clean "synced".
    if cloud_upload_failed {
        let warning = "Saves backed up locally, but cloud upload failed. Check your cloud save settings.";
        emit_phase(app, game_id, "done", Some(warning), cloud_configured, Some(session_minutes), true);
        os_toast_if_hidden(
            app,
            "Cloud upload failed",
            &format!("{game_name} — saves are safe locally but didn't reach the cloud"),
        );
    } else {
        emit_phase(app, game_id, "done", None, cloud_configured, Some(session_minutes), false);
        os_toast_if_hidden(
            app,
            "Saves backed up",
            &format!("{game_name} — session complete"),
        );
    }
    tracing::info!(game_name, "run workflow complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_rclone_remote_name() {
        let yaml_str = r#"
cloud:
  remote:
    WebDav:
      id: ludusavi-1780143898
      url: http://192.168.86.34:47634
      username: DESKTOP-OAA3RS6
      provider: Other
  path: ludusavi-backup
  synchronize: true
        "#;
        let val: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let remote = get_rclone_remote_name(&val);
        assert_eq!(remote, Some("ludusavi-1780143898".to_string()));
    }
}

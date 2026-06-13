//! Run workflow — the marquee feature.
//!
//! Orchestrates the game launch:
//!
//!   restoring → launching → playing → backing-up → uploading → done
//!
//! The post-session backup is split into two observable steps: `backing-up`
//! writes the local ludusavi revision, then `uploading` mirrors it to the
//! cloud remote (only when a remote is configured — otherwise the workflow
//! goes straight from `backing-up` to `done`). Splitting the old combined
//! `backup --cloud-sync` call gives the splash a real boundary to show an
//! upload spinner instead of jumping from the local backup straight to done.
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
use crate::rclone::{self, SessionClass};
use crate::redirects;
use crate::{paths, process, registry};
use chrono::{DateTime, Utc};
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

/// Surfaces an informational note about an automatic cloud-sync resolution
/// (a fast-forward) without interrupting the launch. Emitted as a dedicated
/// `cloud:notice` event the frontend shows as a brief success toast — kept off
/// the `run:phase` channel because non-terminal phases carry no toast (their
/// generic "Syncing…" label would swallow this message). The conflict modal is
/// reserved for true divergence. Also fires a native toast when Spool is hidden
/// so a Game-Mode launch still gets feedback.
fn emit_cloud_notice(app: &AppHandle, _game_id: &str, message: &str) {
    if let Err(e) = app.emit("cloud:notice", message.to_string()) {
        tracing::warn!(error = %e, "failed to emit cloud:notice");
    }
    os_toast_if_hidden(app, "Saves synced", message);
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
pub(crate) fn os_toast_if_hidden(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;

    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        return;
    }

    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!(error = %e, "OS toast failed");
    }
}

/// Persist a game's save-backup stats after a successful backup. The revision
/// count and latest-backup timestamp come from `ludusavi backups` — ludusavi's
/// real backup store is authoritative, so the card stays correct even across
/// pruned revisions or backups made outside Spool. The just-written snapshot's
/// source size is recorded from this run's reported bytes (ludusavi exposes no
/// per-backup or total on-disk size via its API). If the backup list can't be
/// queried we fall back to a simple increment so the signal isn't lost.
async fn persist_backup_stats(
    ludusavi_client: &LudusaviClient,
    ludusavi_exe: &Path,
    config_dir: &Path,
    library: &SharedLibrary,
    game_id: &str,
    game_name: &str,
    bytes_total: u64,
) -> AppResult<()> {
    let stats = ludusavi_client
        .list_backups(ludusavi_exe, config_dir, game_name)
        .await;
    let (count, last_at) = match &stats {
        Ok(s) => (s.count, s.last_backed_up_at),
        Err(e) => {
            tracing::warn!(game_name, error = %e, "ludusavi backups query failed; incrementing count");
            // No fresh count from ludusavi — bump the entry's current count by one.
            let current = library
                .find(game_id)
                .await?
                .map(|e| e.save_backup_count)
                .unwrap_or(0);
            (current + 1, Some(Utc::now()))
        }
    };
    let size_mb = (bytes_total as f64) / (1024.0 * 1024.0);
    if !library
        .record_backup_stats(game_id, count, last_at, Some(size_mb))
        .await?
    {
        tracing::warn!(
            game_id,
            "backup stats not persisted: library entry missing after session"
        );
    }
    Ok(())
}

/// Whether ludusavi cloud sync should actually run right now: a remote is
/// configured AND offline mode isn't suspending it. The manual restore/backup
/// paths use this to decide `--cloud-sync`; the play workflow computes the
/// same split once in `WorkflowCtx::new`.
pub(crate) fn cloud_sync_active() -> bool {
    ludusavi_config::cloud_remote_is_configured() && !crate::config::offline_mode_enabled()
}

/// AppHandle-free backup core. Resolves the game's name + wine prefix from the
/// library, runs `ludusavi backup`, and persists the entry's backup stats.
/// Returns the bundle count + total bytes. Callers handle event emission and
/// rclone recording (best-effort) themselves.
pub async fn backup_game_core(
    ludusavi_client: &LudusaviClient,
    ludusavi_exe: &Path,
    config_dir: &Path,
    library: &SharedLibrary,
    game_id: &str,
) -> AppResult<ManualBackupResult> {
    let (game_name, uses_proton, prefix_override) = {
        let entry = library
            .find(game_id)
            .await?
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.game_name.clone(),
            entry.uses_proton(),
            entry.wine_prefix_path.clone(),
        )
    };
    let wine_prefix =
        crate::proton::resolve_prefix_root(uses_proton, prefix_override.as_deref(), game_id);

    // Serialise the ludusavi backup + cloud sync against any other Spool
    // process on this machine (an attached `--run` workflow, the Decky
    // headless server's game-stop backup). Taken before any ludusavi/rclone
    // work: if it stays contended past the timeout we fail rather than run
    // unlocked — a concurrent write could corrupt the backup or clobber the
    // remote, while the live save sits safe on disk, so the caller (a UI toast
    // for the manual command, an error response for the plugin server) just retries.
    let _backup_lock =
        crate::proc_lock::acquire_backup(std::time::Duration::from_secs(180)).await?;

    // Offline mode with a remote configured: write the local revision only —
    // no `--cloud-sync` network attempt — and report it as not cloud-synced so
    // the badge lands on `local-newer` and `go_online` knows to upload it.
    let cloud_suspended =
        ludusavi_config::cloud_remote_is_configured() && crate::config::offline_mode_enabled();
    let out = if cloud_suspended {
        ludusavi_client
            .backup_local(ludusavi_exe, config_dir, &game_name, wine_prefix.as_deref())
            .await
    } else {
        ludusavi_client
            .backup(ludusavi_exe, config_dir, &game_name, wine_prefix.as_deref())
            .await
    }
    .map_err(|e| AppError::Other(format!("ludusavi backup: {e}")))?;

    // A backup only counts as "in the cloud" when the cloud leg actually ran
    // and ludusavi reported neither a failed cloud sync NOR a cloud conflict.
    // A conflict means the upload was skipped (local and cloud genuinely
    // diverged), so the saves did NOT reach the remote — treat it the same as
    // an outright failure here. The full play workflow force-resolves
    // conflicts (local is authoritative post-play); the headless/manual
    // callers of this core instead leave the unsynced-session marker in place
    // so the next real launch resolves the divergence.
    let cloud_synced = !cloud_suspended
        && out
            .errors
            .as_ref()
            .is_none_or(|e| e.cloud_sync_failed.is_none() && e.cloud_conflict.is_none());

    let (game_count, bytes_total) = out
        .overall
        .as_ref()
        .map(|o| (o.total_games, o.total_bytes))
        .unwrap_or((0, 0));

    if game_count > 0 {
        persist_backup_stats(
            ludusavi_client,
            ludusavi_exe,
            config_dir,
            library,
            game_id,
            &game_name,
            bytes_total,
        )
        .await?;
        // Reflect the real cloud state in the badge: "synced" only when the
        // upload actually reached the remote, otherwise "local-newer" so the
        // user sees the local save hasn't been backed up to the cloud yet.
        let badge = if cloud_synced {
            "synced"
        } else {
            "local-newer"
        };
        library.set_sync_badge(game_id, badge).await?;
    }

    Ok(ManualBackupResult {
        game_count,
        bytes_total,
        cloud_synced,
        game_name,
    })
}

/// Backs up a game's saves, then removes its install files but KEEPS the
/// library entry (the "Remove from disk" option). The backup runs FIRST so
/// unbacked-up or newer-than-last-backup saves — including the live state inside
/// the Proton prefix, which the wipe also deletes — are captured in the ludusavi
/// backup tree (which survives the uninstall). It's the same snapshot the
/// right-click "Back up saves now" takes, so a game with no save tracking is a
/// harmless no-op (`game_count == 0`).
///
/// A hard backup failure ABORTS the uninstall: better to keep the files than to
/// wipe saves we couldn't protect. A cloud-sync conflict/failure is NOT a hard
/// failure — the LOCAL backup still captured the saves, which is what matters
/// for the wipe — so [`backup_game_core`] returns `Ok` there and we proceed.
pub async fn uninstall_game_with_backup(
    ludusavi_client: &LudusaviClient,
    ludusavi_exe: &Path,
    config_dir: &Path,
    library: &SharedLibrary,
    game_id: &str,
) -> AppResult<()> {
    // Fail fast before the (potentially slow, cloud-uploading) backup if the
    // game is currently running — or already being wiped — in any Spool process.
    // `wipe_install_files` re-checks this authoritatively, but only after the
    // backup; without this peek we'd run a full ludusavi backup + cloud upload
    // (force-pushing a mid-session save to the remote) and *then* refuse the
    // wipe. Acquire-and-drop is a peek, not a hold: holding the run lock across
    // the backup would self-block wipe's own acquire (flock is per open file
    // description). A live session/wipe could still start in the gap before the
    // wipe re-checks, but then wipe refuses — only the backup is wasted, never a
    // wipe-out-from-under.
    if crate::proc_lock::try_acquire_run(game_id)?.is_none() {
        return Err(AppError::Other(
            "This game is busy — it's running, or being removed. Close it and try again.".into(),
        ));
    }

    backup_game_core(ludusavi_client, ludusavi_exe, config_dir, library, game_id)
        .await
        .map_err(|e| {
            AppError::Other(format!(
                "Couldn't back up saves before removing from disk: {e}. Your files are untouched — \
                 try again, or use \"Remove from disk and library\" if you no longer need the saves."
            ))
        })?;
    crate::library::uninstall_game_core(library, game_id).await
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
    let ludusavi_exe = crate::paths::resolve_ludusavi_path()
        .ok_or_else(|| AppError::Other("Ludusavi sidecar not found — reinstall Spool.".into()))?;
    let config_dir = crate::paths::ludusavi_config_dir();
    let ludusavi_client = app.state::<LudusaviClient>();
    let library = app.state::<SharedLibrary>();

    let result = backup_game_core(
        &ludusavi_client,
        &ludusavi_exe,
        &config_dir,
        &library,
        &game_id,
    )
    .await?;

    if result.game_count > 0 {
        let _ = app.emit("library:changed", &game_id);
        // Only clear the unsynced-session marker when the saves actually reached
        // the cloud. On a failed or conflicted upload the marker must stay so
        // peers keep warning until a real sync happens. Best-effort.
        if result.cloud_synced {
            rclone::complete_session_backup(&app, &result.game_name).await;
        }
    }
    Ok(result)
}

/// Refresh a game's save-backup stats (revision count + latest-backup
/// timestamp) from ludusavi's real backup store, without running a backup.
/// The detail view calls this when a game is selected so the card reflects
/// ludusavi truth — including backups made outside Spool and pre-existing
/// backups on freshly-added or migrated entries (which the old per-session
/// counter could never show). Best-effort and silent: an unconfigured or
/// missing ludusavi simply leaves the entry untouched. Emits `library:changed`
/// only when a value actually changed, to avoid pointless UI churn.
#[tauri::command]
pub async fn refresh_save_metadata(app: AppHandle, game_id: String) -> AppResult<()> {
    let Some(ludusavi_exe) = crate::paths::resolve_ludusavi_path() else {
        return Ok(());
    };
    let config_dir = crate::paths::ludusavi_config_dir();
    let library = app.state::<SharedLibrary>();
    let Some(entry) = library.find(&game_id).await? else {
        return Ok(());
    };
    let game_name = entry.game_name.clone();

    let ludusavi_client = app.state::<LudusaviClient>();
    let stats = match ludusavi_client
        .list_backups(&ludusavi_exe, &config_dir, &game_name)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(game_name, error = %e, "refresh_save_metadata: backups query failed");
            return Ok(());
        }
    };

    let changed = entry.save_backup_count != stats.count
        || entry.save_last_backed_up_at != stats.last_backed_up_at;
    if changed {
        library
            .record_backup_stats(&game_id, stats.count, stats.last_backed_up_at, None)
            .await?;
        let _ = app.emit("library:changed", &game_id);
    }
    Ok(())
}

/// Manual restore — runs `ludusavi restore` for a single game.
/// Surfaces cloud-sync conflicts as an explicit error so the UI can
/// prompt the user to open Ludusavi (same behaviour as the launch
/// path).
#[tauri::command]
pub async fn manual_restore(app: AppHandle, game_id: String) -> AppResult<ManualRestoreResult> {
    let (game_name, ludusavi_exe, config_dir, wine_prefix) = manual_prep(&app, &game_id).await?;
    let game_folder = app
        .state::<SharedLibrary>()
        .find(&game_id)
        .await?
        .and_then(|e| e.game_folder_path.map(PathBuf::from));
    let ludusavi_client = app.state::<LudusaviClient>();
    let out = restore_with_redirects(
        &ludusavi_client,
        &ludusavi_exe,
        &config_dir,
        &game_name,
        wine_prefix.as_deref(),
        game_folder.as_deref(),
        None,
        cloud_sync_active(),
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

    let game_count = out.overall.as_ref().map(|o| o.total_games).unwrap_or(0);

    Ok(ManualRestoreResult { game_count })
}

/// Outcome of a pull-from-cloud sync, so the UI can tell the user what (if
/// anything) changed without launching the game.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PullOutcome {
    /// No cloud remote configured — nothing to pull.
    Unconfigured,
    /// Local and cloud already matched — nothing changed on disk.
    UpToDate,
    /// Cloud was ahead; its saves were pulled down and restored to disk.
    Pulled,
    /// Local saves are newer than the cloud — left untouched (a pull never
    /// pushes; the user can play to upload, or resolve a conflict explicitly).
    LocalNewer,
}

#[derive(Debug, Clone, Serialize)]
pub struct PullResult {
    pub outcome: PullOutcome,
    pub game_count: i32,
}

/// Pull cloud saves down to this device and restore them to disk, **without**
/// launching the game. The marquee feature's pre-launch restore already does
/// this as part of a play session; this command exposes the same "get the
/// latest saves from the cloud" step on its own, for the "Sync now" buttons on
/// the game pages and the Decky Quick Access menu.
///
/// Guarded by the single-launch lock so a pull can't race a running session.
/// After a successful pull the entry's backup metadata is refreshed (which
/// emits `library:changed`) so the card repaints. See [`pull_cloud_saves_core`]
/// for the pull-only semantics.
#[tauri::command]
pub async fn pull_cloud_saves(app: AppHandle, game_id: String) -> AppResult<PullResult> {
    let run_state = app.state::<RunState>();
    let _guard = run_state.try_acquire(&game_id)?;

    let ludusavi_exe = crate::paths::resolve_ludusavi_path()
        .ok_or_else(|| AppError::Other("Ludusavi sidecar not found — reinstall Spool.".into()))?;
    let config_dir = crate::paths::ludusavi_config_dir();
    let library = app.state::<SharedLibrary>();
    let ludusavi_client = app.state::<LudusaviClient>();

    let result = pull_cloud_saves_core(
        ludusavi_client.inner(),
        &ludusavi_exe,
        &config_dir,
        &library,
        &game_id,
    )
    .await?;

    // Refresh backup count / last-backed-up from ludusavi truth and emit
    // `library:changed` so the card repaints. Best-effort — the pull already
    // succeeded if this no-ops. Skipped when nothing landed (unconfigured /
    // local-newer leave the local store untouched).
    if matches!(result.outcome, PullOutcome::Pulled | PullOutcome::UpToDate) {
        let _ = refresh_save_metadata(app.clone(), game_id.clone()).await;
    }
    Ok(result)
}

/// Pull-only sync core, shared by the [`pull_cloud_saves`] command and the
/// headless plugin server (Decky). Never uploads.
///
/// `ludusavi restore --cloud-sync` pulls the remote into the local backup store
/// and lands the tip on disk. A clean run means we were already in sync. A
/// reported cloud conflict is reconciled against the last-synced baseline:
///   * cloud cleanly ahead → download + re-restore (the actual pull),
///   * local cleanly ahead → leave both sides alone and report `LocalNewer`
///     (pushing would be the opposite of a pull),
///   * both moved → return a "cloud sync conflict" error so the caller's UI
///     opens the same `CloudConflictModal` the launch path uses.
///
/// On a successful pull the entry's `cloud_sync_baseline` is advanced and its
/// `sync_badge` set to `synced`. Does not emit events or refresh ludusavi
/// metadata — that's the caller's job (the headless server has no event bus).
pub async fn pull_cloud_saves_core(
    ludusavi_client: &LudusaviClient,
    ludusavi_exe: &Path,
    config_dir: &Path,
    library: &SharedLibrary,
    game_id: &str,
) -> AppResult<PullResult> {
    // Acquire the machine-wide run lock so we can't sync cloud saves while a game session is running.
    let _run_lock = crate::proc_lock::try_acquire_run(game_id)?.ok_or_else(|| {
        AppError::Other(
            "This game is busy right now (running, or being modified) — close it and try again."
                .into(),
        )
    })?;

    // Without a remote there is nothing to pull — report it so the UI can hint
    // the user to configure cloud saves rather than showing a misleading
    // "up to date". Offline mode counts as unconfigured here: a pull is a
    // network op by definition, so it cleanly no-ops (this also covers the
    // Decky "Sync now" while offline). `go_offline` runs its pulls BEFORE
    // flipping the flag, so the prepare pass is unaffected.
    if !cloud_sync_active() {
        return Ok(PullResult {
            outcome: PullOutcome::Unconfigured,
            game_count: 0,
        });
    }

    // Snapshot what the restore needs from the library (game name, the Proton
    // prefix for cross-device redirects, and the install folder).
    let (game_name, wine_prefix, game_folder) = {
        let entry = library
            .find(game_id)
            .await?
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        let wine_prefix = crate::proton::resolve_prefix_root(
            entry.uses_proton(),
            entry.wine_prefix_path.as_deref(),
            game_id,
        );
        let game_folder = entry.game_folder_path.as_ref().map(PathBuf::from);
        (entry.game_name.clone(), wine_prefix, game_folder)
    };

    // ── Pass 1: restore --cloud-sync (pulls the remote, lands the tip) ────────
    let out = restore_with_redirects(
        ludusavi_client,
        ludusavi_exe,
        config_dir,
        &game_name,
        wine_prefix.as_deref(),
        game_folder.as_deref(),
        None,
        true,
    )
    .await
    .map_err(|e| AppError::Other(format!("ludusavi restore: {e}")))?;

    let conflict = out
        .errors
        .as_ref()
        .and_then(|e| e.cloud_conflict.as_ref())
        .is_some();

    if !conflict {
        // Clean pull — local now reflects the cloud tip. Record it as the synced
        // baseline so the next conflict check is exact, then mark the badge.
        let backup_dir = ludusavi_config::backup_dir();
        if let Some(tip) = redirects::read_local_backup_tip_async(&backup_dir, &game_name).await {
            let _ = set_baseline_in(library, game_id, &tip.name).await;
        }
        mark_synced_badge(library, game_id).await;
        let game_count = out.overall.as_ref().map(|o| o.total_games).unwrap_or(0);
        return Ok(PullResult {
            outcome: PullOutcome::UpToDate,
            game_count,
        });
    }

    // local ≠ cloud — reconcile against the baseline.
    let backup_dir = ludusavi_config::backup_dir();
    let local_tip = redirects::read_local_backup_tip_async(&backup_dir, &game_name).await;
    let cloud_tip = fetch_cloud_backup_tip(&game_name).await;
    let base = library
        .find(game_id)
        .await?
        .and_then(|e| e.cloud_sync_baseline);
    let decision = decide_cloud_sync(base.as_deref(), local_tip.as_ref(), &cloud_tip);
    tracing::info!(
        game_name,
        ?decision,
        base = ?base,
        local = ?local_tip.as_ref().map(|t| t.name.as_str()),
        cloud = ?cloud_tip,
        "pull_cloud_saves reconciliation"
    );
    match decision {
        CloudSyncDecision::InSync => {
            mark_synced_badge(library, game_id).await;
            Ok(PullResult {
                outcome: PullOutcome::UpToDate,
                game_count: 0,
            })
        }
        CloudSyncDecision::FastForwardDownload => {
            // Cloud cleanly ahead — pull it down and re-restore to disk.
            ludusavi_client
                .cloud_resolve(
                    ludusavi_exe,
                    config_dir,
                    crate::ludusavi::CloudOp::Download,
                    &game_name,
                )
                .await
                .map_err(|e| AppError::Other(format!("ludusavi cloud download: {e}")))?;
            let out = restore_with_redirects(
                ludusavi_client,
                ludusavi_exe,
                config_dir,
                &game_name,
                wine_prefix.as_deref(),
                game_folder.as_deref(),
                None,
                true,
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
                    "Cloud sync conflict — local and cloud saves both changed.".into(),
                ));
            }
            if let Some(tip) = cloud_tip.tip() {
                let _ = set_baseline_in(library, game_id, &tip.name).await;
            }
            mark_synced_badge(library, game_id).await;
            let game_count = out.overall.as_ref().map(|o| o.total_games).unwrap_or(0);
            Ok(PullResult {
                outcome: PullOutcome::Pulled,
                game_count,
            })
        }
        CloudSyncDecision::FastForwardUpload => {
            // Local cleanly ahead — a pull never pushes. Leave both sides as they
            // are; the user can play (which uploads on exit) to publish.
            Ok(PullResult {
                outcome: PullOutcome::LocalNewer,
                game_count: 0,
            })
        }
        CloudSyncDecision::Diverged => Err(AppError::Other(
            "Cloud sync conflict — local and cloud saves have both changed.".into(),
        )),
    }
}

/// Set a game's `sync_badge` to `synced` and persist (only writes on a change).
/// Best-effort: a poisoned/failed library lock leaves the badge untouched.
pub(crate) async fn mark_synced_badge(library: &SharedLibrary, game_id: &str) {
    let _ = library.set_sync_badge(game_id, "synced").await;
}

/// List the save revisions ludusavi currently retains for a game, newest
/// first, with the tip flagged. Backs the in-app "restore an earlier save"
/// picker in the game detail card. Reflects the local backup store, so
/// cloud-only revisions this device hasn't pulled aren't included.
#[tauri::command]
pub async fn list_save_revisions(
    app: AppHandle,
    game_id: String,
) -> AppResult<Vec<crate::ludusavi::SaveRevision>> {
    let (game_name, ludusavi_exe, config_dir, _wine_prefix) = manual_prep(&app, &game_id).await?;
    let ludusavi_client = app.state::<LudusaviClient>();
    ludusavi_client
        .list_revisions(&ludusavi_exe, &config_dir, &game_name)
        .await
}

/// Roll back to an earlier save revision. This is a deliberate, destructive
/// action the user invokes from the detail card — never part of the automatic
/// launch workflow.
///
/// A ludusavi restore only writes a backup into the live save dir; it does not
/// change the revision history, so a bare rollback would be silently clobbered
/// by the next pre-launch restore (which always lands the tip). To make the
/// rollback durable we **pin** it: restore the chosen revision locally, then
/// immediately back up so the rolled-back files become a new tip revision.
/// That backup is cloud-synced, so the rollback propagates to every device and
/// the cloud-conflict baseline advances to the new tip. Pinning consumes one
/// retention slot (the oldest revision rolls off).
///
/// Core of [`restore_save_revision`] without an `AppHandle` — restore the
/// chosen revision, then pin it as the new tip, advancing the cloud baseline
/// and clearing the unsynced-session marker when the pin's upload succeeds.
/// Used by the headless plugin server (Decky), which has no `AppHandle`; the
/// command wrapper layers the single-launch lock and the `library:changed`
/// emit on top. Mirrors `pull_cloud_saves_core`'s split between command and
/// plugin entry points.
pub async fn restore_save_revision_core(
    ludusavi_client: &LudusaviClient,
    ludusavi_exe: &Path,
    config_dir: &Path,
    library: &SharedLibrary,
    cfg: &crate::config::ConfigData,
    game_id: &str,
    backup_name: &str,
) -> AppResult<ManualRestoreResult> {
    // Acquire the machine-wide run lock so we can't restore saves while a game session is running.
    let _run_lock = crate::proc_lock::try_acquire_run(game_id)?.ok_or_else(|| {
        AppError::Other(
            "This game is busy right now (running, or being modified) — close it and try again."
                .into(),
        )
    })?;

    // Resolve the game's name, Proton prefix, and install folder from the
    // library (the same fields `manual_prep` derives for the command path).
    let (game_name, uses_proton, prefix_override, game_folder) = {
        let entry = library
            .find(game_id)
            .await?
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.game_name.clone(),
            entry.uses_proton(),
            entry.wine_prefix_path.clone(),
            entry.game_folder_path.clone().map(PathBuf::from),
        )
    };
    let wine_prefix =
        crate::proton::resolve_prefix_root(uses_proton, prefix_override.as_deref(), game_id);

    // ── Step 1: restore the chosen revision into the live save location ───
    // `Some(backup)` takes the no-cloud-sync rollback path, so the flag is
    // inert here — pass false for clarity.
    let out = restore_with_redirects(
        ludusavi_client,
        ludusavi_exe,
        config_dir,
        &game_name,
        wine_prefix.as_deref(),
        game_folder.as_deref(),
        Some(backup_name),
        false,
    )
    .await
    .map_err(|e| AppError::Other(format!("ludusavi restore: {e}")))?;

    let game_count = out.overall.as_ref().map(|o| o.total_games).unwrap_or(0);

    // ── Step 2: pin the rolled-back state as the new tip ──────────────────
    let pin = backup_game_core(ludusavi_client, ludusavi_exe, config_dir, library, game_id)
        .await
        .map_err(|e| AppError::Other(format!("failed to pin rolled-back save: {e}")))?;

    // Only treat the rollback as propagated to the cloud when the pin's upload
    // actually succeeded. If the cloud leg failed/conflicted, the rolled-back
    // tip exists locally but not in the remote — leave the baseline and the
    // unsynced-session marker as-is so the next launch reconciles rather than
    // assuming every device already has this revision.
    if pin.cloud_synced {
        // Advance the cloud-sync baseline to the freshly-written tip so the next
        // launch's conflict check is exact rather than falling back to timestamps.
        let backup_dir = ludusavi_config::backup_dir();
        if let Some(tip) = redirects::read_local_backup_tip_async(&backup_dir, &game_name).await {
            let _ = set_baseline_in(library, game_id, &tip.name).await;
        }
        // We're the latest backer: clear any marker + record the backer.
        rclone::complete_session_backup_from_config(cfg, &game_name).await;
    } else {
        tracing::warn!(game_name, "rollback pin: cloud upload failed — leaving baseline/marker for next launch to reconcile");
    }

    Ok(ManualRestoreResult { game_count })
}

/// Guarded by the single-launch lock so a rollback can't race a running
/// session (and vice versa).
#[tauri::command]
pub async fn restore_save_revision(
    app: AppHandle,
    game_id: String,
    backup_name: String,
) -> AppResult<ManualRestoreResult> {
    let run_state = app.state::<RunState>();
    let _guard = run_state.try_acquire(&game_id)?;

    let ludusavi_exe = crate::paths::resolve_ludusavi_path()
        .ok_or_else(|| AppError::Other("Ludusavi sidecar not found — reinstall Spool.".into()))?;
    let config_dir = crate::paths::ludusavi_config_dir();
    let library = app.state::<SharedLibrary>();
    let ludusavi_client = app.state::<LudusaviClient>();
    let cfg = {
        let cfg = app.state::<SharedConfig>();
        let g = cfg
            .lock()
            .map_err(|_| AppError::Other("config lock poisoned".into()))?;
        g.data.clone()
    };

    let result = restore_save_revision_core(
        &ludusavi_client,
        &ludusavi_exe,
        &config_dir,
        &library,
        &cfg,
        &game_id,
        &backup_name,
    )
    .await?;

    // Repaint the library (backup count / last-backed-up changed).
    if let Err(e) = app.emit("library:changed", &game_id) {
        tracing::warn!(error = %e, "failed to emit library:changed after rollback");
    }

    Ok(result)
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
    // A conflict resolution is forced cloud traffic both ways (mirror one
    // side, then a cloud-syncing restore). In offline mode a stale conflict
    // modal could otherwise still hit the network — fail fast with the way
    // out instead.
    if !cloud_sync_active() {
        return Err(AppError::Other(
            "Offline mode is on — go back online (Settings → Cloud sync) before resolving cloud conflicts.".into(),
        ));
    }
    let op = crate::ludusavi::CloudOp::from_side(&side)?;
    let (game_name, ludusavi_exe, config_dir, wine_prefix) = manual_prep(&app, &game_id).await?;
    let game_folder = app
        .state::<SharedLibrary>()
        .find(&game_id)
        .await?
        .and_then(|e| e.game_folder_path.map(PathBuf::from));
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
        None,
        true,
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

    // The user just reconciled a real divergence: both sides now mirror the
    // chosen copy. Record the resulting tip as the baseline so the very next
    // launch doesn't immediately re-prompt for the same (now-resolved) state.
    let backup_dir = ludusavi_config::backup_dir();
    if let Some(tip) = redirects::read_local_backup_tip_async(&backup_dir, &game_name).await {
        let _ = set_cloud_baseline(&app, &game_id, &tip.name).await;
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
                            let dt: chrono::DateTime<chrono::Utc> =
                                chrono::DateTime::from(mod_time);
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
    crate::capture_stdio!(cmd);
    cmd.kill_on_drop(true);

    let child = cmd.spawn().ok()?;
    let output = tokio::time::timeout(std::time::Duration::from_secs(6), child.wait_with_output())
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
                "get_local_active_save_details: failed to parse ludusavi output: {:?}. Output length: {} bytes",
                e,
                output.stdout.len()
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

/// Resolve `(rclone_exe, remote_name, remote_path)` from the ludusavi
/// `config.yaml` + app config. `None` when cloud isn't configured or the
/// rclone binary can't be found.
fn resolve_rclone_remote() -> Option<(PathBuf, String, String)> {
    let raw = std::fs::read_to_string(crate::paths::ludusavi_config_file()).ok()?;
    let config: serde_yaml::Value = serde_yaml::from_str(&raw).ok()?;
    let remote_name = crate::rclone::remote_name_from_yaml(&config)?;
    let remote_path = config
        .get("cloud")
        .and_then(|c| c.get("path"))
        .and_then(|p| p.as_str())
        .unwrap_or("ludusavi-backup")
        .to_string();
    let rclone_exe = crate::paths::resolve_rclone_path()?;
    Some((rclone_exe, remote_name, remote_path))
}

/// The cloud side of a reconciliation. Distinguishes a backup that's genuinely
/// absent (remote reachable, this game has no cloud backup yet — a clean first
/// upload) from one we simply couldn't read (unreachable / transient), so the
/// reconciler can push silently in the first case but stay conservative in the
/// second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloudTip {
    /// A readable cloud backup tip.
    Present(redirects::BackupTip),
    /// Remote reachable, no cloud backup for this game — safe to push.
    Absent,
    /// Couldn't determine (unreachable / transient / unparseable) — don't guess.
    Unknown,
}

impl CloudTip {
    /// The tip when present, else `None` — for callers that only need the tip.
    pub(crate) fn tip(&self) -> Option<&redirects::BackupTip> {
        match self {
            CloudTip::Present(t) => Some(t),
            _ => None,
        }
    }
}

/// Fetch the cloud copy of a game's `mapping.yaml` tip. Tries the exact game
/// folder name then the Windows-safe variant (mirrors the local lookup and
/// `query_rclone_details`). Returns [`CloudTip::Absent`] only when every
/// candidate is definitively not found (remote reachable); any unreachable or
/// unparseable result yields [`CloudTip::Unknown`]. Uses `rclone::cat_outcome`
/// (which applies FAST_FLAGS) so unreachable remotes fail quickly.
pub(crate) async fn fetch_cloud_backup_tip(game_name: &str) -> CloudTip {
    let Some((rclone_exe, remote_name, remote_path)) = resolve_rclone_remote() else {
        return CloudTip::Unknown;
    };
    let mut folders = vec![game_name.to_string()];
    let safe = redirects::windows_safe_name(game_name);
    if safe != game_name {
        folders.push(safe);
    }
    // Absent only if *every* candidate was a definite NotFound. A single
    // Unreachable (or a read-but-unparseable mapping.yaml) means we can't be
    // sure the cloud is empty, so we stay conservative.
    let mut all_not_found = true;
    for folder in folders {
        let target = format!("{remote_name}:{remote_path}/{folder}/mapping.yaml");
        match crate::rclone::cat_outcome(&rclone_exe, &target).await {
            crate::rclone::CatOutcome::Found(body) => {
                if let Some(tip) = redirects::read_backup_tip_from_str(&body) {
                    return CloudTip::Present(tip);
                }
                all_not_found = false; // read but couldn't parse — don't clobber
            }
            crate::rclone::CatOutcome::NotFound => {}
            crate::rclone::CatOutcome::Unreachable => all_not_found = false,
        }
    }
    if all_not_found {
        CloudTip::Absent
    } else {
        CloudTip::Unknown
    }
}

/// How to reconcile a ludusavi-reported cloud conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloudSyncDecision {
    /// Local and cloud already match — nothing to do, proceed.
    InSync,
    /// Cloud is cleanly ahead — pull it down (download + re-restore).
    FastForwardDownload,
    /// Local is cleanly ahead — push it up (upload).
    FastForwardUpload,
    /// Both sides advanced past the baseline — real conflict, prompt the user.
    Diverged,
}

/// Decide how to reconcile a cloud conflict from three backup-tip fingerprints.
///
/// `base` is the tip name last synced on THIS device (the merge-base);
/// `local`/`cloud` are the current tips. Only called once ludusavi has already
/// flagged a difference.
///
/// With a baseline the call is exact: the side still equal to `base` is the one
/// that didn't move, so the *other* side is a clean fast-forward; if neither
/// equals `base`, both moved → divergence. Without a baseline (legacy entry /
/// never synced) we fall back to a timestamp heuristic — the newer tip wins as
/// a fast-forward; ties or missing data are treated as divergence so we prompt
/// rather than guess.
///
/// A cloud side that's genuinely [`CloudTip::Absent`] (remote reachable, no
/// backup for this game) is a clean first upload, not a conflict — push it. Only
/// an *unreadable* cloud ([`CloudTip::Unknown`]) is treated as divergence: the
/// conflict ludusavi reported means the cloud may have something we couldn't
/// read, so we must not clobber it.
pub(crate) fn decide_cloud_sync(
    base: Option<&str>,
    local: Option<&redirects::BackupTip>,
    cloud: &CloudTip,
) -> CloudSyncDecision {
    match (local, cloud) {
        (Some(l), CloudTip::Present(c)) => {
            if l.name == c.name {
                return CloudSyncDecision::InSync;
            }
            if let Some(base) = base {
                return match (l.name == base, c.name == base) {
                    (true, false) => CloudSyncDecision::FastForwardDownload,
                    (false, true) => CloudSyncDecision::FastForwardUpload,
                    (false, false) => CloudSyncDecision::Diverged,
                    // Both equal base yet names differ is impossible; stay safe.
                    (true, true) => CloudSyncDecision::InSync,
                };
            }
            // No baseline — timestamp heuristic.
            match c.when.cmp(&l.when) {
                std::cmp::Ordering::Greater => CloudSyncDecision::FastForwardDownload,
                std::cmp::Ordering::Less => CloudSyncDecision::FastForwardUpload,
                std::cmp::Ordering::Equal => CloudSyncDecision::Diverged,
            }
        }
        // Local has a backup, cloud genuinely has none → clean first upload.
        (Some(_), CloudTip::Absent) => CloudSyncDecision::FastForwardUpload,
        // Local has a backup but the cloud is unreadable → don't guess, prompt.
        (Some(_), CloudTip::Unknown) => CloudSyncDecision::Diverged,
        // Local has no backup → nothing to lose locally, pulling is safe.
        (None, CloudTip::Present(_)) => CloudSyncDecision::FastForwardDownload,
        // Nothing on either side → nothing to reconcile (proceed).
        (None, CloudTip::Absent) => CloudSyncDecision::InSync,
        (None, CloudTip::Unknown) => CloudSyncDecision::Diverged,
    }
}

/// Record `tip_name` as the cloud-sync baseline for `game_id`.
async fn set_cloud_baseline(app: &AppHandle, game_id: &str, tip_name: &str) -> AppResult<()> {
    set_baseline_in(&app.state::<SharedLibrary>(), game_id, tip_name).await
}

/// Library-based variant of [`set_cloud_baseline`] for callers that hold the
/// `SharedLibrary` directly rather than an `AppHandle` (e.g. the headless
/// plugin server).
pub(crate) async fn set_baseline_in(
    library: &SharedLibrary,
    game_id: &str,
    tip_name: &str,
) -> AppResult<()> {
    library.set_cloud_baseline(game_id, tip_name).await?;
    Ok(())
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
    cmd.args(crate::rclone::FAST_FLAGS);
    cmd.arg("lsjson")
        .arg("--no-mimetype")
        .arg("--recursive")
        .arg(&target);
    crate::capture_stdio!(cmd);
    cmd.kill_on_drop(true);

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("query_rclone_details: failed to spawn rclone: {:?}", e);
            return None;
        }
    };

    let output =
        match tokio::time::timeout(std::time::Duration::from_secs(6), child.wait_with_output())
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
                "query_rclone_details: failed to deserialize JSON from rclone: {:?}. Output length: {} bytes",
                e,
                output.stdout.len()
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
        .filter_map(|i| {
            chrono::DateTime::parse_from_rfc3339(&i.mod_time)
                .ok()
                .map(|dt| (dt, &i.mod_time))
        })
        .max_by_key(|(dt, _)| *dt)
        .map(|(_, mod_time)| mod_time.clone());

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
    let (game_name, ludusavi_exe, config_dir, wine_prefix) = manual_prep(&app, &game_id).await?;
    let mut local = get_local_active_save_details(
        &ludusavi_exe,
        &config_dir,
        &game_name,
        wine_prefix.as_deref(),
    )
    .await;
    if local.is_none() {
        tracing::info!("get_local_active_save_details returned None; falling back to local backup directory stats");
        // The fallback recursively walks the backup dir (`walkdir` + per-file
        // metadata), so run it off the async runtime.
        let gn = game_name.clone();
        local = tokio::task::spawn_blocking(move || get_local_backup_details(&gn))
            .await
            .ok()
            .flatten();
    }
    tracing::info!("local details for {}: {:?}", game_name, local);

    // 2. Get cloud details if cloud is configured
    let config_file = crate::paths::ludusavi_config_file();
    if !config_file.exists() {
        tracing::warn!(
            "get_cloud_conflict_details: config.yaml does not exist at {:?}",
            config_file
        );
        return Ok(RawConflictDetails { local, cloud: None });
    }
    let raw = std::fs::read_to_string(&config_file)
        .map_err(|e| AppError::Other(format!("failed to read config.yaml: {e}")))?;
    let config: serde_yaml::Value = serde_yaml::from_str(&raw)
        .map_err(|e| AppError::Other(format!("failed to parse config.yaml: {e}")))?;

    let Some(remote_name) = crate::rclone::remote_name_from_yaml(&config) else {
        tracing::warn!("get_cloud_conflict_details: cloud remote is not configured in config.yaml");
        return Ok(RawConflictDetails { local, cloud: None });
    };

    let remote_path = config
        .get("cloud")
        .and_then(|c| c.get("path"))
        .and_then(|p| p.as_str())
        .unwrap_or("ludusavi-backup");

    let rclone_exe = crate::paths::resolve_rclone_path()
        .ok_or_else(|| AppError::Other("rclone sidecar not found — reinstall Spool.".into()))?;

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

    tracing::info!(
        "get_cloud_conflict_details results: local={:?}, cloud={:?}",
        local,
        cloud
    );
    Ok(RawConflictDetails { local, cloud })
}

#[derive(Debug, Serialize)]
pub struct ManualBackupResult {
    pub game_count: i32,
    pub bytes_total: u64,
    /// False when the local backup succeeded but the cloud upload leg failed.
    /// Callers that clear the unsynced-session marker must check this first.
    pub cloud_synced: bool,
    pub game_name: String,
}

#[derive(Debug, Serialize)]
pub struct ManualRestoreResult {
    pub game_count: i32,
}

/// Snapshot for the manual backup/restore commands. Returns:
///   (game_name, ludusavi_exe, config_dir, wine_prefix)
///
/// `wine_prefix` is `Some` only when the game launches through Proton (Windows
/// `.exe` on Linux — see [`GameEntry::uses_proton`]); it is the prefix ROOT
/// (not drive_c) passed as `--wine-prefix` to backup. Restore never takes a
/// prefix — cross-device remapping is handled by redirects (Phase 3).
async fn manual_prep(
    app: &AppHandle,
    game_id: &str,
) -> AppResult<(String, PathBuf, PathBuf, Option<PathBuf>)> {
    let (game_name, uses_proton, prefix_override) = {
        let entry = app
            .state::<SharedLibrary>()
            .find(game_id)
            .await?
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.game_name.clone(),
            entry.uses_proton(),
            entry.wine_prefix_path.clone(),
        )
    };
    let ludusavi_exe = crate::paths::resolve_ludusavi_path()
        .ok_or_else(|| AppError::Other("Ludusavi sidecar not found — reinstall Spool.".into()))?;
    let config_dir = crate::paths::ludusavi_config_dir();
    let wine_prefix =
        crate::proton::resolve_prefix_root(uses_proton, prefix_override.as_deref(), game_id);
    Ok((game_name, ludusavi_exe, config_dir, wine_prefix))
}

#[tauri::command]
pub async fn launch_game(app: AppHandle, game_id: String, steal: Option<bool>) -> AppResult<()> {
    launch_game_inner_steal(&app, &game_id, steal.unwrap_or(false)).await
}

/// Inner launch function callable from non-command contexts (e.g. the
/// `tauri-plugin-single-instance` callback when a forwarded `--run` arrives).
/// Same behaviour as the `launch_game` command — single-launch guard +
/// full workflow + phase emission.
pub async fn launch_game_inner(app: &AppHandle, game_id: &str) -> AppResult<()> {
    launch_game_inner_steal(app, game_id, false).await
}

/// Like [`launch_game_inner`] but with control over whether the play-state
/// lock acquire may steal a *suspended* lock from another device. Only the
/// user's explicit "Play here instead" override passes `steal = true`.
pub async fn launch_game_inner_steal(app: &AppHandle, game_id: &str, steal: bool) -> AppResult<()> {
    let run_state = app.state::<RunState>();
    let _guard = run_state.try_acquire(game_id)?;

    // Hold the machine-wide per-game run lock for the whole session so a disk
    // wipe (uninstall / delete-from-disk) in ANY Spool process can't delete this
    // game's install folder + Proton prefix out from under it mid-play — the
    // in-process `RunState` above only covers this process. `None` ⇒ the game is
    // already running in another process, or is being removed; fail fast rather
    // than race. The OS frees the lock when this process exits.
    let _run_lock = crate::proc_lock::try_acquire_run(game_id)?.ok_or_else(|| {
        AppError::Other(
            "This game is busy right now (already running, or being removed) — try again in a moment."
                .into(),
        )
    })?;

    // Snapshot what we need from state up front so we don't hold any
    // sync Mutex across the long-running awaits below. We also fold
    // the registry-level Run-As-Admin compat flag into the effective
    // `needs_admin` here so the launch path doesn't have to know
    // about the registry concept.
    let (game_name, exe_path, needs_admin, proton_version_path, wine_prefix_path, launch_args) = {
        let entry = app
            .state::<SharedLibrary>()
            .find(game_id)
            .await?
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        if !entry.installed {
            return Err(AppError::Other(
                "This game isn't installed — reinstall it first.".into(),
            ));
        }
        if entry.exe_path.is_empty() {
            return Err(AppError::Other("Game has no executable configured".into()));
        }
        let needs_admin = entry.run_as_admin || registry::run_as_admin_in_registry(&entry.exe_path);
        (
            entry.game_name.clone(),
            entry.exe_path.clone(),
            needs_admin,
            entry.proton_version_path.clone(),
            entry.wine_prefix_path.clone(),
            entry.launch_args.clone(),
        )
    };

    let ludusavi_exe = crate::paths::resolve_ludusavi_path()
        .ok_or_else(|| AppError::Other("Ludusavi sidecar not found — reinstall Spool.".into()))?;

    let (umu_run_path, default_proton_path) = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        (
            cfg.data.launch.umu_run_path.clone(),
            cfg.data.launch.default_proton_path.clone(),
        )
    };

    // Resolve the launch plan (umu-run + Proton paths) *before* the long
    // awaits below so a misconfiguration surfaces as a clean launch error.
    let launch_plan = build_launch_plan(
        game_id,
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
        steal,
    )
    .await;

    if let Err(e) = &result {
        emit_phase(
            app,
            game_id,
            "error",
            Some(&e.to_string()),
            false,
            None,
            false,
        );
        // Surface the failure via the OS notification centre too —
        // most workflow errors happen while the user is mid-launch
        // with Spool tucked into the tray.
        os_toast_if_hidden(app, "Spool: launch failed", &format!("{game_name} — {e}"));
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

/// Resolves a [`LaunchPlan`] from the game's settings + app config. Whether
/// Proton is used is derived from the platform + executable type
/// ([`crate::proton::exe_needs_proton`]): on Linux a Windows `.exe` always
/// launches through Proton, native binaries run directly, and on Windows Proton
/// is never used. There is no on/off toggle (issue #80).
#[allow(clippy::too_many_arguments)]
fn build_launch_plan(
    game_id: &str,
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

    let effective_proton = crate::proton::exe_needs_proton(exe_path);

    if effective_proton {
        let umu_run = crate::proton::resolve_umu_run(Some(umu_run_path))?;
        // `None` when the user hasn't pinned a Proton — we then leave
        // PROTONPATH unset and let umu-run pick its own default.
        let proton_path = crate::proton::resolve_proton_path(
            proton_version_path.as_deref(),
            Some(default_proton_path),
        );
        return Ok(LaunchPlan {
            use_proton: true,
            umu_run: Some(umu_run),
            proton_path,
            prefix_root,
            extra_args,
            run_as_admin: false,
        });
    }

    // Native path. On Linux, `exe_needs_proton` has already routed every `.exe`
    // through the Proton branch above, so anything reaching here is a native
    // binary (or we're on Windows, where games run natively).
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
    backup: Option<&str>,
    cloud_sync: bool,
) -> AppResult<crate::ludusavi::ApiOutput> {
    // Restore the requested revision, or the tip. `Some(id)` is a local
    // rollback (no cloud sync — see `restore_backup`); `None` restores the
    // latest backup, pulling the cloud first when `cloud_sync` is set (callers
    // pass false when no remote is configured or offline mode is on). Both
    // passes restore the same selection.
    macro_rules! do_restore {
        () => {
            match backup {
                Some(id) => {
                    ludusavi_client
                        .restore_backup(ludusavi_exe, config_dir, game_name, id)
                        .await
                }
                None => {
                    ludusavi_client
                        .restore(ludusavi_exe, config_dir, game_name, cloud_sync)
                        .await
                }
            }
        };
    }

    // ── Pass 1: restore (pulls cloud unless rolling back to an id) ─────────
    let first = do_restore!()?;

    // ── Read mapping.yaml to detect origin ────────────────────────────────
    let backup_dir = ludusavi_config::backup_dir();
    let Some(origin) = redirects::read_backup_origin_async(&backup_dir, game_name).await else {
        // No backup on disk yet (first-ever session). Nothing to redirect.
        tracing::info!(
            game_name,
            "no mapping.yaml found — skipping redirect generation"
        );
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
    let second = do_restore!()?;

    // Clear redirects after the restore so they don't affect unrelated
    // operations (e.g. a manual backup). We regenerate on every restore.
    let _ = ludusavi_config::set_redirects(&[]);

    Ok(second)
}

/// Inputs threaded through every phase of [`run_workflow`]. Built once at the
/// top of the workflow so each phase reads from one place instead of an 8–10
/// argument list. The borrowed fields are the workflow's inputs; the owned
/// fields are derived once: the ludusavi config dir, the Wine prefix (Proton
/// games on Linux only), whether a cloud remote is configured, and the install
/// folder used for the Phase 3 install-dir save redirect.
struct WorkflowCtx<'a> {
    app: &'a AppHandle,
    game_id: &'a str,
    game_name: &'a str,
    launch: &'a LaunchPlan,
    ludusavi_exe: &'a Path,
    ludusavi_client: &'a LudusaviClient,
    config_dir: PathBuf,
    wine_prefix: Option<PathBuf>,
    /// A cloud remote is configured in ludusavi's config.yaml — durable for
    /// the session. Whether cloud sync actually RUNS also depends on the
    /// offline-mode flag, which the user can flip mid-session, so each phase
    /// derives the live split via [`Self::cloud_split`] instead of trusting a
    /// launch-time snapshot.
    remote_configured: bool,
    game_folder: Option<PathBuf>,
}

impl<'a> WorkflowCtx<'a> {
    async fn new(
        app: &'a AppHandle,
        game_id: &'a str,
        game_name: &'a str,
        launch: &'a LaunchPlan,
        ludusavi_exe: &'a Path,
        ludusavi_client: &'a LudusaviClient,
    ) -> AppResult<Self> {
        let config_dir = crate::paths::ludusavi_config_dir();
        // Wine prefix for restore/backup (Proton games on Linux only).
        let wine_prefix: Option<PathBuf> = if launch.use_proton {
            Some(launch.prefix_root.clone())
        } else {
            None
        };
        // Check once whether a cloud remote is configured. The offline-mode
        // flag is deliberately NOT snapshotted here — each phase re-derives it
        // (see `cloud_split`) so a toggle made while the game is running is
        // honoured by the post-session backup.
        let remote_configured = ludusavi_config::cloud_remote_is_configured();
        // Snapshot the install folder path for the install-dir save redirect.
        let game_folder = app
            .state::<SharedLibrary>()
            .find(game_id)
            .await?
            .and_then(|e| e.game_folder_path.map(PathBuf::from));
        Ok(Self {
            app,
            game_id,
            game_name,
            launch,
            ludusavi_exe,
            ludusavi_client,
            config_dir,
            wine_prefix,
            remote_configured,
            game_folder,
        })
    }

    /// The live `(cloud_configured, cloud_suspended)` split:
    /// *configured* = a remote is set AND offline mode is off (cloud sync
    /// runs); *suspended* = a remote is set but offline mode pauses it — the
    /// session must end on the `local-newer` badge so `go_online` knows to
    /// upload it, while a genuinely remote-less library has no cloud to be
    /// newer than. Reads the offline flag fresh from disk so a mid-session
    /// toggle is honoured; call ONCE per phase so the steps within a phase
    /// can't disagree with each other.
    fn cloud_split(&self) -> (bool, bool) {
        let offline = crate::config::offline_mode_enabled();
        (
            self.remote_configured && !offline,
            self.remote_configured && offline,
        )
    }

    fn wine_prefix(&self) -> Option<&Path> {
        self.wine_prefix.as_deref()
    }

    fn game_folder(&self) -> Option<&Path> {
        self.game_folder.as_deref()
    }
}

/// Timing of the play session, produced by [`phase_launch`] and consumed by the
/// backup + completion phases.
struct SessionTiming {
    minutes: i32,
}

#[allow(clippy::too_many_arguments)]
async fn run_workflow(
    app: &AppHandle,
    game_id: &str,
    game_name: &str,
    exe_path: &str,
    launch: &LaunchPlan,
    ludusavi_exe: &Path,
    ludusavi_client: &LudusaviClient,
    steal_lock: bool,
) -> AppResult<()> {
    tracing::info!(game_id, game_name, "starting run workflow");

    let ctx = WorkflowCtx::new(
        app,
        game_id,
        game_name,
        launch,
        ludusavi_exe,
        ludusavi_client,
    )
    .await?;
    let exe_pathbuf = PathBuf::from(exe_path);

    // Emit the first phase immediately, before preflight. preflight's
    // `claim_session` makes a cloud round-trip (read + write the session marker)
    // that can take several seconds on a high-latency remote like Google Drive,
    // and phase_restore — which emits the first phase otherwise — only runs after
    // it. Without this the UI (desktop overlay + Game-Mode splash) shows nothing
    // until that returns, so the Play button looks dead. The splash already
    // defaults to `restoring`; phase_restore re-emits with its own message.
    let (cloud_configured, _) = ctx.cloud_split();
    let prep_msg = if cloud_configured {
        "Syncing + restoring saves…"
    } else {
        "Restoring local saves…"
    };
    emit_phase(
        ctx.app,
        ctx.game_id,
        "restoring",
        Some(prep_msg),
        cloud_configured,
        None,
        false,
    );

    preflight(&ctx, &exe_pathbuf, steal_lock).await?;
    let no_saves = phase_restore(&ctx).await?;
    let timing = phase_launch(&ctx, &exe_pathbuf).await?;
    let cloud_upload_failed = phase_backup(&ctx, no_saves, &timing).await?;
    finish(&ctx, no_saves, cloud_upload_failed, timing.minutes);

    tracing::info!(game_name, "run workflow complete");
    Ok(())
}

/// Launch preflight: validate the target and claim the cross-device session
/// marker BEFORE any saves are touched. Two reasons:
///   * A missing exe / un-creatable Proton prefix should fail fast, without
///     restoring saves or writing a marker (a marker written then abandoned
///     would block every peer until it ages out, ACTIVE_STALE_SECS).
///   * `claim_session` must run before restore, which mutates the live save
///     dir — otherwise a launch blocked by another device's active/unsynced
///     session would surface the modal only AFTER we'd already overwritten
///     local saves with the cloud copy.
///
/// The message suffixes below are what the frontend regexes on for the
/// "Play here instead" override (which re-runs with `steal_lock`).
async fn preflight(ctx: &WorkflowCtx<'_>, exe_pathbuf: &Path, steal_lock: bool) -> AppResult<()> {
    if !exe_pathbuf.is_file() {
        return Err(AppError::Other(format!(
            "Game executable not found at {}",
            exe_pathbuf.display()
        )));
    }
    // For Proton launches, make sure the prefix root exists; umu/Proton
    // populates it (drive_c, registry) on first run.
    if ctx.launch.use_proton {
        if let Err(e) = std::fs::create_dir_all(&ctx.launch.prefix_root) {
            return Err(AppError::Other(format!(
                "failed to create Proton prefix dir {:?}: {e}",
                ctx.launch.prefix_root
            )));
        }
    }

    // Refresh ludusavi's `customGames` block from the library before any restore
    // so a non-manifest game with a custom save location is *recognised* (else
    // ludusavi lists it under `unknownGames` and the workflow skips its backup).
    // Covers a definition just adopted from another device, or set this session.
    crate::custom_saves::sync_best_effort(ctx.app).await;

    // Validate the config Spool just (re)wrote before any ludusavi call relies on
    // it. An invalid config.yaml makes EVERY ludusavi op fail with a cryptic
    // "config file is invalid" mid-restore (this is how a ludusavi schema change
    // silently broke cloud sync — see the cloud.remote migration); catching it
    // here turns that into one clear, actionable message at the right moment.
    // Runs before claim_session so a failure leaves no session marker behind.
    if let Err(reason) = crate::ludusavi::validate_config(ctx.ludusavi_exe, &ctx.config_dir).await {
        tracing::error!(game_name = ctx.game_name, %reason, "ludusavi config rejected by its own loader");
        return Err(AppError::Other(format!(
            "Spool's save configuration is invalid, so saves can't be backed up or restored: {reason}. \
             This is usually a Spool bug — please report it."
        )));
    }

    match rclone::claim_session(ctx.app, ctx.game_name, steal_lock).await {
        SessionClass::Free => {}
        SessionClass::ActiveElsewhere { device_name } => {
            return Err(AppError::Other(format!(
                "Already playing on {device_name}. Close it there, or play here anyway."
            )));
        }
        SessionClass::UnsyncedElsewhere { device_name } => {
            return Err(AppError::Other(format!(
                "Unsynced session on {device_name}. Its latest saves aren't in the cloud yet — \
                 close it there and let it sync, or play here anyway."
            )));
        }
    }
    Ok(())
}

/// Phase 1: restore saves before launch. Returns whether ludusavi found no
/// saves to restore (a fresh game, fine). Any failure after the session claim
/// releases our marker so peers aren't blocked by a session that never started.
async fn phase_restore(ctx: &WorkflowCtx<'_>) -> AppResult<bool> {
    // One offline-mode read for the whole phase: the restore flag, the phase
    // messages, and the baseline advance must all agree.
    let (cloud_configured, _) = ctx.cloud_split();
    let restore_phase: AppResult<bool> = async {
        // Coordinate with any backup in flight on this machine before touching
        // saves. Restore runs `ludusavi restore --cloud-sync`, which reads the
        // same backup tree + cloud remote a concurrent backup is writing —
        // racing them risks a stale restore or a spurious cloud conflict. The
        // usual culprit is the Decky forced-close backup (via the headless
        // server) firing as the user immediately launches the next game: pause
        // the splash and wait
        // for that backup to finish first. The lock is held across the restore
        // so a new backup can't start mid-restore, then dropped before launch so
        // the in-session Decky fallback isn't blocked. If the holder doesn't
        // finish within the timeout (or the lock file can't be opened) we proceed
        // without it rather than block the launch — restore reads the backup
        // store, so a stale read is possible, but the live save is on disk and
        // the post-session backup reconciles.
        let _backup_lock: Option<crate::proc_lock::FileLock> =
            match crate::proc_lock::try_acquire_backup() {
                Ok(Some(guard)) => Some(guard),
                Ok(None) => {
                    emit_phase(
                        ctx.app,
                        ctx.game_id,
                        "restoring",
                        Some("Waiting for a backup to finish…"),
                        cloud_configured,
                        None,
                        false,
                    );
                    os_toast_if_hidden(
                        ctx.app,
                        "Waiting for backup",
                        &format!("{} — finishing a backup before launch", ctx.game_name),
                    );
                    match crate::proc_lock::acquire_backup(std::time::Duration::from_secs(180)).await
                    {
                        Ok(guard) => Some(guard),
                        Err(e) => {
                            tracing::warn!(game_name = ctx.game_name, error = %e, "restore: timed out waiting for backup lock, proceeding without it");
                            None
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(game_name = ctx.game_name, error = %e, "restore: backup lock unavailable, proceeding without it");
                    None
                }
            };

        let restore_msg = if cloud_configured {
            "Syncing + restoring saves…"
        } else {
            "Restoring local saves…"
        };
        emit_phase(ctx.app, ctx.game_id, "restoring", Some(restore_msg), cloud_configured, None, false);
        os_toast_if_hidden(
            ctx.app,
            "Restoring saves",
            &format!("{} — restoring before launch", ctx.game_name),
        );
        tracing::info!(game_name = ctx.game_name, "ludusavi restore");
        let restore = restore_with_redirects(
            ctx.ludusavi_client,
            ctx.ludusavi_exe,
            &ctx.config_dir,
            ctx.game_name,
            ctx.wine_prefix(),
            ctx.game_folder(),
            None,
            cloud_configured,
        ).await?;
        if restore
            .errors
            .as_ref()
            .and_then(|e| e.cloud_conflict.as_ref())
            .is_some()
        {
            // ludusavi saw local ≠ cloud and refused to sync. Reconcile before
            // continuing (fast-forward one side, or prompt on true divergence).
            reconcile_cloud_conflict(ctx).await?;
        } else if cloud_configured {
            // Clean restore with cloud configured — record the current local tip as
            // the synced baseline so the next conflict check is exact.
            let backup_dir = ludusavi_config::backup_dir();
            if let Some(tip) = redirects::read_local_backup_tip_async(&backup_dir, ctx.game_name).await {
                let _ = set_cloud_baseline(ctx.app, ctx.game_id, &tip.name).await;
            }
        }
        // "No saves to restore" is only true when ludusavi explicitly doesn't
        // recognise the game (unknown_games non-empty). total_games == 0 on
        // restore just means there's no existing backup yet (first session) —
        // we still want to back up after that session.
        let no_saves = restore
            .errors
            .as_ref()
            .map(|e| !e.unknown_games.is_empty())
            .unwrap_or(false);
        Ok(no_saves)
    }
    .await;
    match restore_phase {
        Ok(v) => Ok(v),
        Err(e) => {
            // Restore failed after we claimed the session — release our marker so
            // peers aren't blocked by a session that never started.
            rclone::delete_session_marker(ctx.app, ctx.game_name).await;
            Err(e)
        }
    }
}

/// Handles a restore that ludusavi refused because local ≠ cloud. The baseline
/// (last-synced tip) distinguishes a clean fast-forward — one side cleanly
/// ahead, auto-resolved — from a true divergence where both changed since the
/// last sync, which aborts the launch for the user to resolve in Ludusavi.
async fn reconcile_cloud_conflict(ctx: &WorkflowCtx<'_>) -> AppResult<()> {
    let backup_dir = ludusavi_config::backup_dir();
    let local_tip = redirects::read_local_backup_tip_async(&backup_dir, ctx.game_name).await;
    let cloud_tip = fetch_cloud_backup_tip(ctx.game_name).await;
    let base = ctx
        .app
        .state::<SharedLibrary>()
        .find(ctx.game_id)
        .await?
        .and_then(|e| e.cloud_sync_baseline);
    let decision = decide_cloud_sync(base.as_deref(), local_tip.as_ref(), &cloud_tip);
    // A genuinely empty cloud means this is the game's first upload, not a
    // conflict — note that distinctly so we can reassure the user it's expected.
    let first_upload = matches!(cloud_tip, CloudTip::Absent);
    tracing::info!(
        game_name = ctx.game_name,
        ?decision,
        base = ?base,
        local = ?local_tip.as_ref().map(|t| t.name.as_str()),
        cloud = ?cloud_tip,
        "cloud conflict reconciliation"
    );
    match decision {
        CloudSyncDecision::Diverged => {
            return Err(AppError::Other(
                "Cloud sync conflict — open Ludusavi to resolve before launching.".into(),
            ));
        }
        CloudSyncDecision::FastForwardDownload => {
            // Cloud is cleanly ahead — pull it down and re-restore.
            ctx.ludusavi_client
                .cloud_resolve(
                    ctx.ludusavi_exe,
                    &ctx.config_dir,
                    crate::ludusavi::CloudOp::Download,
                    ctx.game_name,
                )
                .await?;
            // Only reachable when the pass-1 restore ran with cloud sync
            // on (it reported the conflict), so the re-restore keeps it on.
            let out = restore_with_redirects(
                ctx.ludusavi_client,
                ctx.ludusavi_exe,
                &ctx.config_dir,
                ctx.game_name,
                ctx.wine_prefix(),
                ctx.game_folder(),
                None,
                true,
            )
            .await?;
            if out
                .errors
                .as_ref()
                .and_then(|e| e.cloud_conflict.as_ref())
                .is_some()
            {
                return Err(AppError::Other(
                    "Cloud sync conflict — open Ludusavi to resolve before launching.".into(),
                ));
            }
            if let Some(tip) = cloud_tip.tip() {
                let _ = set_cloud_baseline(ctx.app, ctx.game_id, &tip.name).await;
            }
            emit_cloud_notice(ctx.app, ctx.game_id, "Restored newer saves from the cloud");
        }
        CloudSyncDecision::FastForwardUpload => {
            // Local is cleanly ahead — push it up. Pass-1 restore already
            // landed the local saves, so no re-restore is needed.
            ctx.ludusavi_client
                .cloud_resolve(
                    ctx.ludusavi_exe,
                    &ctx.config_dir,
                    crate::ludusavi::CloudOp::Upload,
                    ctx.game_name,
                )
                .await?;
            if let Some(tip) = local_tip.as_ref() {
                let _ = set_cloud_baseline(ctx.app, ctx.game_id, &tip.name).await;
            }
            if first_upload {
                // The cloud was empty for this game — reassure rather than stay
                // silent, since the old behaviour wrongly showed a conflict here.
                emit_cloud_notice(
                    ctx.app,
                    ctx.game_id,
                    "Uploaded your saves to the cloud for the first time",
                );
            }
        }
        CloudSyncDecision::InSync => {}
    }
    Ok(())
}

/// Phase 2: spawn the game and wait for it to exit. Target validation and the
/// session claim already happened in [`preflight`]; saves are restored. Owns the
/// session heartbeat + suspend-watcher lifecycle, flips the marker to
/// `pending-backup` on exit, and records playtime / last-played.
async fn phase_launch(ctx: &WorkflowCtx<'_>, exe_pathbuf: &Path) -> AppResult<SessionTiming> {
    let (cloud_configured, _) = ctx.cloud_split();
    emit_phase(
        ctx.app,
        ctx.game_id,
        "launching",
        Some("Launching game…"),
        cloud_configured,
        None,
        false,
    );
    emit_phase(
        ctx.app,
        ctx.game_id,
        "playing",
        None,
        cloud_configured,
        None,
        false,
    );
    if ctx.launch.use_proton {
        tracing::info!(
            exe_path = %exe_pathbuf.display(),
            umu_run = %ctx.launch.umu_run.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "<none>".into()),
            prefix_root = %ctx.launch.prefix_root.display(),
            proton_path = %ctx.launch.proton_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "<umu default>".into()),
            "launching game via Proton"
        );
    } else {
        tracing::info!(
            exe_path = %exe_pathbuf.display(),
            run_as_admin = ctx.launch.run_as_admin,
            "launching game natively"
        );
    }
    let session_start = Utc::now();

    let spec = if ctx.launch.use_proton {
        process::LaunchSpec::Proton {
            umu_run: ctx
                .launch
                .umu_run
                .as_deref()
                .expect("umu_run resolved for proton launch"),
            prefix_root: &ctx.launch.prefix_root,
            proton_path: ctx.launch.proton_path.as_deref(),
            game_id: ctx.game_id,
            extra_args: &ctx.launch.extra_args,
            extra_env: &[],
        }
    } else {
        process::LaunchSpec::Native {
            run_as_admin: ctx.launch.run_as_admin,
        }
    };

    // Spawn the session heartbeat: rewrites our marker's `updated_at` every
    // 60s so peers see the session as live. Pass session_start so the heartbeat
    // preserves the real started_at on each tick. Aborted on exit.
    let heartbeat = rclone::start_heartbeat(
        ctx.app.clone(),
        ctx.game_name.to_string(),
        session_start.to_rfc3339(),
    );

    // On Linux, watch for system suspend (logind PrepareForSleep) for the
    // life of the session. When the device sleeps mid-session (Steam Deck
    // suspend), it marks the session marker suspended so a peer sees an
    // unsynced session rather than the marker silently going stale.
    // No-op on other platforms.
    // Accumulates time spent suspended mid-session (Linux), subtracted below so
    // sleep doesn't count as play time.
    let suspended_secs: crate::suspend::SuspendedSecs =
        std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));
    let suspend_watcher = crate::suspend::start_suspend_watcher(
        ctx.app.clone(),
        ctx.game_name.to_string(),
        suspended_secs.clone(),
    );

    let spawn_result = process::run_game(exe_pathbuf, spec).await;
    let session_end = Utc::now();

    // Always abort the heartbeat + suspend watcher, then flip our marker to
    // `pending-backup` — even if launch failed mid-spawn. The marker stays
    // until the post-session backup confirms the saves reached the cloud, so
    // a peer keeps warning until then.
    heartbeat.abort();
    suspend_watcher.abort();
    // Await both tasks so their rclone children are fully dropped (kill_on_drop)
    // before we write PendingBackup. Without this, an in-flight Active rcat
    // could complete on the remote after our PendingBackup write, reverting it.
    let _ = heartbeat.await;
    let _ = suspend_watcher.await;
    rclone::mark_session_pending_backup(ctx.app, ctx.game_name).await;

    tracing::info!(
        game_name = ctx.game_name,
        duration_secs = (session_end - session_start).num_seconds(),
        exit_code = spawn_result.as_ref().ok().map(|r| r.code).unwrap_or(-1),
        "game exited"
    );

    match spawn_result {
        Err(e) => {
            // Process never started — delete the marker so peers aren't blocked.
            rclone::delete_session_marker(ctx.app, ctx.game_name).await;
            return Err(AppError::Other(format!("Game failed to launch: {e}")));
        }
        Ok(ref result) if result.crash_hint.is_some() => {
            // Game exited in under 5 seconds with a non-zero code — almost
            // certainly a Wine/Proton crash before the window opened (missing
            // DLL, bad prefix, vcredist not installed, etc.). Treat it as a
            // failed launch so the user sees the relevant umu output rather than
            // a silent 0-minute session.
            rclone::delete_session_marker(ctx.app, ctx.game_name).await;
            let hint = result.crash_hint.as_deref().unwrap_or_default();
            let log_path = crate::paths::log_file();
            return Err(AppError::Other(format!(
                "Game exited immediately (code {}).\n\n{hint}\n\nFull log: {}",
                result.code,
                log_path.display()
            )));
        }
        Ok(_) => {}
    }

    // ── Update last_played + playtime (best-effort) ───────────────────
    // Subtract any time the device spent suspended mid-session so sleeping with
    // the game still running doesn't inflate the hours played.
    let suspended = suspended_secs.load(std::sync::atomic::Ordering::Relaxed);
    let played_secs = ((session_end - session_start).num_seconds() - suspended).max(0);
    let session_minutes = (played_secs / 60) as i32;

    // Record this launch as a discrete play-session row (the per-session,
    // per-device history) FIRST — the row is the dedup token. Its `INSERT OR
    // IGNORE` returns whether a *new* row landed; only then do we bump the local
    // playtime aggregate and push cross-device playtime, so a Game-Mode
    // forced-close fallback that already recorded this session (or a double
    // fire) can't double-count. (#1/#2) Best-effort: failures here must not fail
    // the run. The local table is the source of truth; the rclone push makes it
    // visible to peers.
    let newly_recorded = record_play_session(ctx, session_start, session_end, played_secs).await;
    if newly_recorded {
        // Playtime + last-played are derived from the session timeline (the row
        // just inserted), not accumulated — so a remote switch carries them over
        // for free via the history blob, and a re-fold can't drop them.
        // Best-effort: a transient recompute failure must not abort the run
        // (else phase_backup never runs and the pending-backup marker is never
        // cleared, blocking peers until it ages out). The cached value is
        // derived and self-heals on the next launch/fold, so swallowing is safe.
        if let Err(e) = ctx
            .app
            .state::<SharedLibrary>()
            .recompute_playtime(ctx.game_name)
            .await
        {
            tracing::warn!(error = %e, game = ctx.game_name, "failed to recompute playtime");
        }
        let _ = ctx.app.emit("library:changed", &ctx.game_id.to_string());
    }

    Ok(SessionTiming {
        minutes: session_minutes,
    })
}

/// The instant a play session's `session_id` is keyed on. Both the in-process
/// workflow and the forced-close fallback resolve it the same way: the
/// active-session record's `started_at` when a *live* one exists for this game
/// (a Game-Mode / streaming attached launch writes it fresh at launch with
/// `backed_up = false`), else `fallback` — the workflow's own session start,
/// used on desktop where there is no record.
///
/// One shared source is what makes the idempotency cross-cut both paths: if a
/// Game-Mode session is recorded by the in-process [`record_play_session`] and
/// then *also* by a forced-close backup that fired before the workflow flipped
/// `backed_up`, both derive the same `session_id`, so `insert_session`'s
/// `INSERT OR IGNORE` dedupes it instead of writing a second row and
/// double-counting playtime.
///
/// The record is only consulted when THIS process wrote it (an attached `--run`
/// launch — [`crate::session::wrote_start_this_process`]). A desktop launch
/// never writes a record, so any it finds is a stale leftover from a past
/// attached session whose forced-close backup failed (leaving `backed_up =
/// false`); adopting that record's `started_at` would key the fresh desktop
/// session on an old start and let `INSERT OR IGNORE` dedupe it away. Desktop
/// launches therefore fall through to `fallback` (their own session start), and
/// can't collide because no fallback fires for a non-Steam launch anyway.
fn session_id_seed(game_name: &str, fallback: DateTime<Utc>) -> DateTime<Utc> {
    seed_from_record(
        crate::session::wrote_start_this_process(),
        crate::session::read(),
        game_name,
        fallback,
    )
}

/// Pure decision behind [`session_id_seed`], split out so it's testable without
/// the process-global flag or on-disk record. Adopts `rec.started_at` only when
/// this process wrote the record (`wrote_start`) AND it's a live (`!backed_up`)
/// record for this game; otherwise returns `fallback`.
fn seed_from_record(
    wrote_start: bool,
    rec: Option<crate::session::ActiveSession>,
    game_name: &str,
    fallback: DateTime<Utc>,
) -> DateTime<Utc> {
    if !wrote_start {
        return fallback;
    }
    rec.filter(|r| r.game == game_name && !r.backed_up)
        .map(|r| r.started_at)
        .unwrap_or(fallback)
}

/// Insert a [`PlaySession`] row for the just-finished launch and, when the row
/// is new, push this session's cross-device playtime and the updated history to
/// the rclone remote. Returns whether a *new* row landed: `false` when the row
/// already existed (a forced-close fallback beat us to it, or no device
/// identity), so the caller skips the local-aggregate bump too.
///
/// The cross-device playtime push happens HERE, alongside the row insert, rather
/// than later in [`phase_backup`]. The backup can take many seconds, and a
/// Game-Mode force-kill in that window would otherwise leave the row written but
/// the device-blob playtime unpushed — and the fallback, seeing the row, would
/// dedupe and never push it (#1). Tying both to the same insert keeps the device
/// blob's additive playtime pushed exactly once per session, even on a kill.
///
/// Best-effort: every failure is logged and swallowed so it can't fail the run
/// workflow. No-op for the cross-device pushes when cloud isn't configured (the
/// local row is still written).
async fn record_play_session(
    ctx: &WorkflowCtx<'_>,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    duration_secs: i64,
) -> bool {
    let (device_id, device_name) = rclone::device_identity(ctx.app);
    if device_id.is_empty() {
        // No device identity (poisoned config) — skip rather than write a row
        // with a blank device id that the cross-device fold can't attribute.
        return false;
    }
    let seed = session_id_seed(ctx.game_name, started_at);
    let session = crate::library::PlaySession {
        session_id: format!("{device_id}:{}", seed.timestamp_millis()),
        device_id,
        device_name,
        game_name: ctx.game_name.to_string(),
        started_at,
        ended_at,
        duration_secs,
    };
    match ctx
        .app
        .state::<SharedLibrary>()
        .insert_session(&session)
        .await
    {
        Ok(true) => {}
        // Already recorded (a forced-close fallback won the race, or a retry) —
        // don't re-push playtime; signal the caller to skip the aggregate bump.
        Ok(false) => return false,
        Err(e) => {
            tracing::warn!(error = %e, game = ctx.game_name, "failed to record play session");
            return false;
        }
    }
    let minutes = (duration_secs / 60) as i32;
    // These write independent remote files (the device blob vs the history blob)
    // and only the first takes the control-plane lock, so run them concurrently —
    // on a high-latency remote their round-trips would otherwise stack.
    let ended = ended_at.to_rfc3339();
    tokio::join!(
        rclone::record_session(ctx.app, ctx.game_name, minutes, &ended),
        rclone::sync_play_history(ctx.app),
    );
    true
}

/// Record the LOCAL side of a session Spool's own workflow never got to finish:
/// the SteamOS Game-Mode forced-close path, where Steam SIGKILLs Spool before
/// [`phase_play`] reaches [`record_play_session`]. Called from the plugin
/// server's game-stop backup ([`crate::plugin_server`]) with the start time from
/// the active-session record ([`crate::session`]) — which is exactly the
/// [`session_id_seed`] the in-process path uses, so the two dedupe.
///
/// Writes the `play_sessions` row, bumps local playtime, and pushes BOTH the
/// history blob and this session's cross-device playtime — all gated on the row
/// being new, exactly as the in-process [`record_play_session`] does. Pushing
/// playtime here (not in the caller, after the backup) is what makes the device
/// blob's additive playtime land exactly once per session: whoever inserts the
/// row pushes the playtime, and `INSERT OR IGNORE` lets only one path win, so a
/// forced-close that the in-process workflow already partly recorded, a Decky
/// retry, or a double-fire can't double-count or drop it (#1/#2). The caller is
/// left with only the backup-completion side effects (marker delete + backup
/// stamp), which are idempotent.
///
/// Returns `Some(minutes)` when a **new** row landed, or `None` when the session
/// was already recorded (the in-process workflow beat us to it via
/// [`session_id_seed`], a retry) or there's no device identity.
///
/// Duration is `(ended_at - started_at) - suspended_secs`, mirroring the
/// in-process path: `suspended_secs` is the suspend total the watcher
/// checkpointed into the session record on each resume (see [`crate::session`]),
/// so a session spanning a Deck sleep isn't counted as play time even though the
/// in-memory tally died with the force-killed workflow.
#[cfg_attr(windows, allow(dead_code))] // only reached via the unix-gated plugin server
pub async fn record_session_headless(
    library: &crate::library::Library,
    cfg: &crate::config::ConfigData,
    game_name: &str,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    suspended_secs: i64,
) -> Option<i32> {
    // Trimmed to match the `_from_config` rclone helpers; device_id is a UUID
    // (config::ensure_device_identity) so this equals the untrimmed value
    // device_identity() hands the in-process path, keeping the session_ids equal.
    let device_id = cfg.device_id.trim().to_string();
    let device_name = cfg.device_name.trim().to_string();
    if device_id.is_empty() {
        // No device identity — skip rather than write a row the cross-device
        // fold can't attribute. Matches `record_play_session`.
        return None;
    }
    let duration_secs = ((ended_at - started_at).num_seconds() - suspended_secs.max(0)).max(0);
    let session = crate::library::PlaySession {
        session_id: format!("{device_id}:{}", started_at.timestamp_millis()),
        device_id,
        device_name,
        game_name: game_name.to_string(),
        started_at,
        ended_at,
        duration_secs,
    };
    match library.insert_session(&session).await {
        // New row — record playtime locally; caller adds the device-blob delta.
        Ok(true) => {}
        // Already recorded (the in-process path, a retry, a double-fire). Don't
        // bump playtime again, and signal the caller not to push the device blob.
        Ok(false) => return None,
        Err(e) => {
            tracing::warn!(error = %e, game = game_name, "forced-close: failed to record play session");
            return None;
        }
    }
    let minutes = (duration_secs / 60) as i32;
    if let Err(e) = library.recompute_playtime(game_name).await {
        tracing::warn!(error = %e, game = game_name, "forced-close: failed to recompute playtime");
    }
    // Push cross-device playtime now, gated on the row insert above — same
    // contract as the in-process path. The caller only does the backup-completion
    // side effects (marker delete + backup stamp). Independent remote files, so
    // run concurrently (mirrors record_play_session).
    let ended = ended_at.to_rfc3339();
    tokio::join!(
        rclone::record_session_from_config(cfg, game_name, minutes, &ended),
        rclone::sync_play_history_from_config(cfg, library),
    );
    tracing::info!(
        game = game_name,
        duration_secs,
        "forced-close: recorded play session"
    );
    Some(minutes)
}

/// Phase 3: back up saves after the session (skipped when ludusavi didn't
/// recognise the game). Returns whether the local backup succeeded but the
/// cloud upload (`--cloud-sync`) failed — the workflow still finishes (the save
/// is safe on disk) but the caller warns the user rather than claiming a clean
/// sync.
async fn phase_backup(
    ctx: &WorkflowCtx<'_>,
    no_saves: bool,
    timing: &SessionTiming,
) -> AppResult<bool> {
    let session_minutes = timing.minutes;
    let mut cloud_upload_failed = false;
    // Re-derive the offline split NOW rather than trusting the launch-time
    // value: the user may have gone offline while the game was running, and
    // a session that ends in offline mode must skip the upload and land on
    // `local-newer` (so `go_online` knows to push it). One read for the whole
    // phase so the skip/upload/baseline/badge steps below can't disagree.
    let (cloud_configured, cloud_suspended) = ctx.cloud_split();
    if !no_saves {
        emit_phase(
            ctx.app,
            ctx.game_id,
            "backing-up",
            Some("Backing up saves…"),
            cloud_configured,
            Some(session_minutes),
            false,
        );
        os_toast_if_hidden(
            ctx.app,
            "Backing up saves",
            &format!("{} — session ended", ctx.game_name),
        );

        // Take the machine-wide backup lock before touching ludusavi or the
        // remote, and hold it across the local backup + cloud upload so the
        // pair is atomic versus another Spool process (e.g. the Decky
        // forced-close backup via the headless server racing this same
        // session). If it
        // stays contended past the timeout, defer rather than run unlocked: a
        // concurrent ludusavi/rclone write could corrupt the backup or clobber
        // the remote. Nothing's been written yet, the live save is safe on
        // disk, and the lock being held means another process is *already*
        // backing this up — so we flag the save unsynced (local-newer badge +
        // leave the PendingBackup marker so peers keep warning) and let the next
        // launch reconcile. Playtime was already pushed at record time
        // (phase_launch), so there's nothing to record here.
        let _backup_lock = match crate::proc_lock::acquire_backup(std::time::Duration::from_secs(
            180,
        ))
        .await
        {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!(game_id = %ctx.game_id, error = %e, "post-session backup deferred — backup lock held by another process");
                if ctx
                    .app
                    .state::<SharedLibrary>()
                    .set_sync_badge(ctx.game_id, "local-newer")
                    .await
                    .unwrap_or(false)
                {
                    let _ = ctx.app.emit("library:changed", &ctx.game_id.to_string());
                }
                return Ok(true);
            }
        };

        // Phase 3 prelude — canonicalise save paths for Proton games. The
        // restore phase steered a foreign-origin (e.g. Windows) save into the
        // local Proton prefix; without matching backup redirects ludusavi would
        // now record the *local prefix* paths, flipping the backup from Windows
        // paths to Linux paths and breaking the next restore on Windows. Mirror
        // the restore redirects (inverted) so the backup stays portable. Cleared
        // after the backup so they never affect an unrelated operation.
        let mut backup_redirects_set = false;
        if let Some(prefix) = ctx.wine_prefix() {
            let backup_dir = ludusavi_config::backup_dir();
            if let Some(origin) =
                redirects::read_backup_origin_async(&backup_dir, ctx.game_name).await
            {
                let local_win_user = redirects::local_windows_username();
                match redirects::apply_redirects_for_backup(
                    &origin,
                    Some(prefix),
                    ctx.game_folder(),
                    local_win_user.as_deref(),
                ) {
                    Ok(n) if n > 0 => {
                        backup_redirects_set = true;
                        tracing::info!(
                            game_name = ctx.game_name,
                            redirects = n,
                            "applied backup redirects — storing canonical save paths"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(game_name = ctx.game_name, error = %e, "failed to apply backup redirects");
                    }
                }
            }
        }

        tracing::info!(game_name = ctx.game_name, "ludusavi backup (local)");
        let backup_outcome = ctx
            .ludusavi_client
            .backup_local(
                ctx.ludusavi_exe,
                &ctx.config_dir,
                ctx.game_name,
                ctx.wine_prefix(),
            )
            .await;

        // Clear backup redirects regenerated fresh next session — matches the
        // restore phase's clean-up so stale entries can never linger.
        if backup_redirects_set {
            let _ = ludusavi_config::set_redirects(&[]);
        }

        match backup_outcome {
            Ok(out) => {
                if let Some(overall) = &out.overall {
                    if overall.total_games > 0 {
                        let library = ctx.app.state::<SharedLibrary>();
                        persist_backup_stats(
                            ctx.ludusavi_client,
                            ctx.ludusavi_exe,
                            &ctx.config_dir,
                            &library,
                            ctx.game_id,
                            ctx.game_name,
                            overall.total_bytes,
                        )
                        .await?;
                        let _ = ctx.app.emit("library:changed", &ctx.game_id.to_string());
                    }
                }

                // Read the freshly-written local tip once; reused below for both
                // the skip decision and the baseline advance. (The cloud upload
                // doesn't touch the local mapping.yaml, so this stays valid
                // across it.)
                let backup_dir = ludusavi_config::backup_dir();
                let local_tip =
                    redirects::read_local_backup_tip_async(&backup_dir, ctx.game_name).await;

                // Skip the cloud upload when this session produced no save
                // changes and the remote already holds the current revision.
                // ludusavi's scan reports `new + different == 0` when the live
                // save is byte-identical to the latest backup (no new tip was
                // written) — the same signal ludusavi-playnite uses to avoid
                // redundant backups. But "unchanged this session" doesn't on its
                // own mean "already uploaded": a *prior* upload could have failed
                // and left the remote behind. So we also require the cloud-sync
                // baseline (last tip confirmed on the remote) to match the local
                // tip before skipping — otherwise we still owe the cloud an
                // upload even though nothing changed this time. Only meaningful
                // when a remote is configured.
                let skip_upload = cloud_configured && !out.saves_changed() && {
                    // A failed baseline read is not a reason to abort the real
                    // upload — treat an unreadable/absent baseline as "not
                    // current" and upload to be safe.
                    let baseline = ctx
                        .app
                        .state::<SharedLibrary>()
                        .find(ctx.game_id)
                        .await
                        .ok()
                        .flatten()
                        .and_then(|e| e.cloud_sync_baseline);
                    match (local_tip.as_ref(), baseline) {
                        (Some(tip), Some(base)) => tip.name == base,
                        _ => false,
                    }
                };
                if skip_upload {
                    tracing::info!(
                        game_name = ctx.game_name,
                        "saves unchanged and already on the cloud — skipping upload"
                    );
                }

                // The local revision is written. Now mirror it to the cloud as
                // a separate, observable step so the splash can show a live
                // "uploading" spinner instead of jumping straight from the local
                // backup to "done" — the combined `backup --cloud-sync` call
                // blocked silently through the upload, which made the cloud step
                // look skipped. We just played, so local is authoritative: a
                // forced `cloud upload` overwrites the remote (the same
                // resolution the old combined path applied on a cloud conflict),
                // so a remote that advanced under us still fast-forwards cleanly.
                if cloud_configured && !skip_upload {
                    emit_phase(
                        ctx.app,
                        ctx.game_id,
                        "uploading",
                        Some("Uploading saves to your cloud remote…"),
                        true,
                        Some(session_minutes),
                        false,
                    );
                    tracing::info!(game_name = ctx.game_name, "ludusavi cloud upload");
                    match ctx
                        .ludusavi_client
                        .cloud_resolve(
                            ctx.ludusavi_exe,
                            &ctx.config_dir,
                            crate::ludusavi::CloudOp::Upload,
                            ctx.game_name,
                        )
                        .await
                    {
                        Ok(up) => {
                            // ludusavi reports a sync failure as a non-fatal
                            // field on an otherwise-successful op (the local
                            // snapshot still landed). Surface it — silently
                            // swallowing this is what made a dead rclone path /
                            // bad WebDAV creds look like "synced" while nothing
                            // reached the remote.
                            if up
                                .errors
                                .as_ref()
                                .and_then(|e| e.cloud_sync_failed.as_ref())
                                .is_some()
                            {
                                cloud_upload_failed = true;
                                tracing::warn!(
                                    game_name = ctx.game_name,
                                    "post-session cloud upload failed — saves backed up locally but not uploaded"
                                );
                            }
                        }
                        Err(e) => {
                            cloud_upload_failed = true;
                            tracing::warn!(game_name = ctx.game_name, error = %e, "post-session cloud upload failed");
                        }
                    }
                }
                // Playtime/last-played were already pushed at record time
                // (phase_launch). Here we only finish the backup: when the
                // upload reached the cloud, clear the session marker and stamp
                // this device as the latest backer. When it failed, leave the
                // PendingBackup marker so peers keep warning.
                if !cloud_upload_failed {
                    rclone::complete_session_backup(ctx.app, ctx.game_name).await;
                }

                // Advance the cloud-sync baseline to the freshly-written tip,
                // but only when the upload actually reached the cloud — otherwise
                // local and cloud genuinely differ and the next launch should
                // re-evaluate rather than assume we're synced.
                if cloud_configured && !cloud_upload_failed {
                    if let Some(tip) = local_tip.as_ref() {
                        let _ = set_cloud_baseline(ctx.app, ctx.game_id, &tip.name).await;
                    }
                }

                // Set the badge to match the real cloud state: "synced" only
                // when the upload reached the remote, otherwise "local-newer"
                // so the user sees the local save hasn't been backed up to the
                // cloud yet (a flaky network / unreachable remote — or offline
                // mode, where the configured remote is deliberately suspended
                // and `go_online` will pick the badge up to upload).
                let target_badge = if cloud_upload_failed || cloud_suspended {
                    "local-newer"
                } else {
                    "synced"
                };
                if ctx
                    .app
                    .state::<SharedLibrary>()
                    .set_sync_badge(ctx.game_id, target_badge)
                    .await
                    .unwrap_or(false)
                {
                    let _ = ctx.app.emit("library:changed", &ctx.game_id.to_string());
                }
            }
            Err(e) => {
                // Don't fail the workflow — the user already played the game
                // successfully and getting a red toast for a flaky network
                // call would be misleading. Surface it in the log instead.
                tracing::warn!(game_id = %ctx.game_id, error = %e, "post-session backup failed");
                // Playtime was already pushed at record time (phase_launch); the
                // PendingBackup marker stays so peers/next-launch reconcile.
            }
        }
    } else {
        // Ludusavi doesn't recognise this game — no backup will ever clear the
        // PendingBackup marker. Delete it now so other devices aren't
        // permanently blocked from launching this game. (Playtime was already
        // pushed at record time in phase_launch.)
        rclone::delete_session_marker(ctx.app, ctx.game_name).await;
    }
    Ok(cloud_upload_failed)
}

/// Completion: flag the Game-Mode session record as backed up and emit the
/// terminal `done` phase + native toast. When the cloud upload failed the
/// `done` phase carries a warning so the frontend shows a sticky toast instead
/// of a clean "synced".
fn finish(ctx: &WorkflowCtx<'_>, no_saves: bool, cloud_upload_failed: bool, session_minutes: i32) {
    let (cloud_configured, _) = ctx.cloud_split();
    // Game Mode: reconcile the active-session record for THIS game. On full
    // success (saves reached the cloud, or no cloud configured) the record has
    // done its job — clear it so a later "Back up now" / game-stop can't act on a
    // stale, already-synced session (#280). When the cloud upload failed, keep
    // the record but flag the local backup done so the Decky forced-close
    // fallback is skipped while peers/next-launch reconcile. Both are guarded on
    // the record's own session id (and game name) so a newer session that started
    // since this one can't be clobbered (#273). No-op off Game Mode (no record).
    if let Some(rec) = crate::session::read() {
        if rec.game == ctx.game_name {
            if cloud_upload_failed {
                crate::session::mark_backed_up_if(&rec.session_id);
            } else {
                crate::session::clear_if(&rec.session_id);
            }
        }
    }

    // Final completion ping — the most useful native toast since the
    // user may have closed the game and walked away from the PC.
    if no_saves {
        // Ludusavi doesn't track saves for this game — don't claim a backup happened.
        emit_phase(
            ctx.app,
            ctx.game_id,
            "done",
            None,
            cloud_configured,
            Some(session_minutes),
            false,
        );
        os_toast_if_hidden(
            ctx.app,
            "Session complete",
            &format!("{} — no save data tracked", ctx.game_name),
        );
    } else if cloud_upload_failed {
        let warning =
            "Saves backed up locally, but cloud upload failed. Check your cloud save settings.";
        emit_phase(
            ctx.app,
            ctx.game_id,
            "done",
            Some(warning),
            cloud_configured,
            Some(session_minutes),
            true,
        );
        os_toast_if_hidden(
            ctx.app,
            "Cloud upload failed",
            &format!(
                "{} — saves are safe locally but didn't reach the cloud",
                ctx.game_name
            ),
        );
    } else {
        emit_phase(
            ctx.app,
            ctx.game_id,
            "done",
            None,
            cloud_configured,
            Some(session_minutes),
            false,
        );
        os_toast_if_hidden(
            ctx.app,
            "Saves backed up",
            &format!("{} — session complete", ctx.game_name),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tip(name: &str, secs: i64) -> redirects::BackupTip {
        redirects::BackupTip {
            name: name.to_string(),
            when: chrono::DateTime::from_timestamp(secs, 0).unwrap(),
        }
    }

    #[tokio::test]
    async fn record_session_headless_records_once_and_doesnt_double_count() {
        // The Game-Mode forced-close backup (the plugin server's game-stop
        // endpoint) may fire more than once for one session — e.g. a Decky
        // retry. It must record the session exactly once and bump playtime
        // exactly once. Cloud is unconfigured, so the
        // rclone pushes inside the helper no-op (no network in this test).
        let lib = crate::library::Library::open_in_memory().await.unwrap();
        let mut game = crate::library::GameEntry {
            id: "a".to_string(),
            game_name: "Hades".to_string(),
            ..Default::default()
        };
        game.playtime_minutes = 0;
        lib.insert(game).await.unwrap();

        let cfg = crate::config::ConfigData {
            device_id: "deck".to_string(),
            device_name: "Deck".to_string(),
            ..Default::default()
        };
        let start = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = start + chrono::Duration::minutes(45);

        record_session_headless(&lib, &cfg, "Hades", start, end, 0).await;
        assert_eq!(lib.list_sessions(Some("Hades")).await.unwrap().len(), 1);
        assert_eq!(lib.find("a").await.unwrap().unwrap().playtime_minutes, 45);

        // Same session start ⇒ same session_id ⇒ INSERT OR IGNORE no-op, and
        // playtime is NOT bumped a second time.
        record_session_headless(&lib, &cfg, "Hades", start, end, 0).await;
        assert_eq!(lib.list_sessions(Some("Hades")).await.unwrap().len(), 1);
        assert_eq!(lib.find("a").await.unwrap().unwrap().playtime_minutes, 45);
    }

    #[tokio::test]
    async fn record_session_headless_subtracts_suspend_time() {
        // A 45-min wall-clock session that spent 15 min suspended counts as 30
        // min of play — sleep time isn't play time. Mirrors the in-process path.
        let lib = crate::library::Library::open_in_memory().await.unwrap();
        let game = crate::library::GameEntry {
            id: "a".to_string(),
            game_name: "Hades".to_string(),
            ..Default::default()
        };
        lib.insert(game).await.unwrap();

        let cfg = crate::config::ConfigData {
            device_id: "deck".to_string(),
            device_name: "Deck".to_string(),
            ..Default::default()
        };
        let start = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = start + chrono::Duration::minutes(45);

        record_session_headless(&lib, &cfg, "Hades", start, end, 15 * 60).await;
        let sessions = lib.list_sessions(Some("Hades")).await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].duration_secs, 30 * 60);
        assert_eq!(lib.find("a").await.unwrap().unwrap().playtime_minutes, 30);
    }

    #[tokio::test]
    async fn record_session_headless_dedupes_against_in_process_row() {
        // The in-process path records first; its session_id is keyed on the SAME
        // active-session start (via session_id_seed). A forced-close backup that
        // fires before the workflow finished must NOT write a second row or
        // double playtime — it dedupes and returns None.
        let lib = crate::library::Library::open_in_memory().await.unwrap();
        let game = crate::library::GameEntry {
            id: "a".to_string(),
            game_name: "Hades".to_string(),
            ..Default::default()
        };
        lib.insert(game).await.unwrap();
        let cfg = crate::config::ConfigData {
            device_id: "deck".to_string(),
            device_name: "Deck".to_string(),
            ..Default::default()
        };
        let start = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = start + chrono::Duration::minutes(45);

        // Simulate the in-process row already present, keyed on the shared seed
        // (device_id:started_at_millis), plus its derived playtime.
        let pre = crate::library::PlaySession {
            session_id: format!("deck:{}", start.timestamp_millis()),
            device_id: "deck".to_string(),
            device_name: "Deck".to_string(),
            game_name: "Hades".to_string(),
            started_at: start,
            ended_at: end,
            duration_secs: 45 * 60,
        };
        assert!(lib.insert_session(&pre).await.unwrap());
        lib.recompute_playtime("Hades").await.unwrap();

        // Forced-close fallback fires for the same session.
        let res = record_session_headless(&lib, &cfg, "Hades", start, end, 0).await;
        assert_eq!(
            res, None,
            "deduped — no new row, caller skips device-blob push"
        );
        assert_eq!(lib.list_sessions(Some("Hades")).await.unwrap().len(), 1);
        assert_eq!(
            lib.find("a").await.unwrap().unwrap().playtime_minutes,
            45,
            "playtime must not be double-bumped"
        );
    }

    #[tokio::test]
    async fn record_session_headless_skips_blank_device_id() {
        // No device identity ⇒ no row (the cross-device fold couldn't attribute
        // it). Matches `record_play_session`.
        let lib = crate::library::Library::open_in_memory().await.unwrap();
        let game = crate::library::GameEntry {
            id: "a".to_string(),
            game_name: "Hades".to_string(),
            ..Default::default()
        };
        lib.insert(game).await.unwrap();

        let cfg = crate::config::ConfigData::default(); // device_id empty
        let start = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        record_session_headless(
            &lib,
            &cfg,
            "Hades",
            start,
            start + chrono::Duration::minutes(45),
            0,
        )
        .await;
        assert_eq!(lib.list_sessions(Some("Hades")).await.unwrap().len(), 0);
        assert_eq!(lib.find("a").await.unwrap().unwrap().playtime_minutes, 0);
    }

    fn active_record(
        game: &str,
        started_at: DateTime<Utc>,
        backed_up: bool,
    ) -> crate::session::ActiveSession {
        crate::session::ActiveSession {
            game: game.to_string(),
            steam_appid: 0x8000_0001,
            session_id: format!("x-{}", started_at.timestamp_millis()),
            started_at,
            backed_up,
            suspended_secs: 0,
        }
    }

    #[test]
    fn seed_adopts_live_record_only_when_this_process_wrote_it() {
        // Attached launch (wrote_start = true): the fresh record's start is the
        // seed, so the in-process and forced-close paths derive the same
        // session_id and dedupe.
        let rec_start = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let fallback = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            seed_from_record(
                true,
                Some(active_record("Hades", rec_start, false)),
                "Hades",
                fallback
            ),
            rec_start,
        );
    }

    #[test]
    fn seed_ignores_record_on_desktop_launch() {
        // Desktop launch (wrote_start = false) must NOT adopt a stale record left
        // by a past attached session — else a fresh session keyed on the old
        // start would be deduped away. (#4) Falls through to its own start.
        let stale_start = chrono::DateTime::parse_from_rfc3339("2026-06-01T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let fallback = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            seed_from_record(
                false,
                Some(active_record("Hades", stale_start, false)),
                "Hades",
                fallback
            ),
            fallback,
            "desktop launch ignores the active-session record",
        );
    }

    #[test]
    fn seed_ignores_backed_up_or_other_game_record() {
        let rec_start = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let fallback = chrono::DateTime::parse_from_rfc3339("2026-06-06T10:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // backed_up record (a reconciled / failed-upload leftover) is ignored.
        assert_eq!(
            seed_from_record(
                true,
                Some(active_record("Hades", rec_start, true)),
                "Hades",
                fallback
            ),
            fallback,
        );
        // a record for a different game is ignored.
        assert_eq!(
            seed_from_record(
                true,
                Some(active_record("Celeste", rec_start, false)),
                "Hades",
                fallback
            ),
            fallback,
        );
        // no record at all → fallback.
        assert_eq!(seed_from_record(true, None, "Hades", fallback), fallback);
    }

    #[test]
    fn ff_download_when_local_equals_base() {
        // Local unchanged since last sync, cloud advanced → pull cloud.
        let local = tip("A", 100);
        let cloud = CloudTip::Present(tip("B", 200));
        assert_eq!(
            decide_cloud_sync(Some("A"), Some(&local), &cloud),
            CloudSyncDecision::FastForwardDownload
        );
    }

    #[test]
    fn ff_upload_when_cloud_equals_base() {
        // Cloud unchanged since last sync, local advanced → push local.
        let local = tip("B", 200);
        let cloud = CloudTip::Present(tip("A", 100));
        assert_eq!(
            decide_cloud_sync(Some("A"), Some(&local), &cloud),
            CloudSyncDecision::FastForwardUpload
        );
    }

    #[test]
    fn diverged_when_both_moved_past_base() {
        // Neither side matches the baseline → both changed → real conflict.
        let local = tip("B", 200);
        let cloud = CloudTip::Present(tip("C", 210));
        assert_eq!(
            decide_cloud_sync(Some("A"), Some(&local), &cloud),
            CloudSyncDecision::Diverged
        );
    }

    #[test]
    fn in_sync_when_tips_match() {
        let local = tip("A", 100);
        let cloud = CloudTip::Present(tip("A", 100));
        assert_eq!(
            decide_cloud_sync(Some("A"), Some(&local), &cloud),
            CloudSyncDecision::InSync
        );
    }

    #[test]
    fn no_baseline_uses_timestamp_heuristic() {
        let older = tip("A", 100);
        let newer = tip("B", 200);
        // Cloud newer → download.
        assert_eq!(
            decide_cloud_sync(None, Some(&older), &CloudTip::Present(tip("B", 200))),
            CloudSyncDecision::FastForwardDownload
        );
        // Local newer → upload.
        assert_eq!(
            decide_cloud_sync(None, Some(&newer), &CloudTip::Present(tip("A", 100))),
            CloudSyncDecision::FastForwardUpload
        );
        // Equal timestamps, different names → can't tell → prompt.
        let a = tip("A", 100);
        assert_eq!(
            decide_cloud_sync(None, Some(&a), &CloudTip::Present(tip("B", 100))),
            CloudSyncDecision::Diverged
        );
    }

    #[test]
    fn unreadable_cloud_tip_is_conservative() {
        // ludusavi flagged a conflict but we couldn't read the cloud (transient
        // / unreachable) — don't clobber it, prompt instead.
        let local = tip("A", 100);
        assert_eq!(
            decide_cloud_sync(Some("A"), Some(&local), &CloudTip::Unknown),
            CloudSyncDecision::Diverged
        );
    }

    #[test]
    fn absent_cloud_with_local_is_first_upload() {
        // Remote reachable but this game has no cloud backup yet → push, don't
        // prompt (issue #338).
        let local = tip("A", 100);
        assert_eq!(
            decide_cloud_sync(Some("A"), Some(&local), &CloudTip::Absent),
            CloudSyncDecision::FastForwardUpload
        );
        // Even without a baseline.
        assert_eq!(
            decide_cloud_sync(None, Some(&local), &CloudTip::Absent),
            CloudSyncDecision::FastForwardUpload
        );
    }

    #[test]
    fn nothing_on_either_side_is_in_sync() {
        // No local backup and a genuinely empty cloud → nothing to reconcile.
        assert_eq!(
            decide_cloud_sync(None, None, &CloudTip::Absent),
            CloudSyncDecision::InSync
        );
    }

    #[test]
    fn missing_local_tip_pulls_cloud() {
        let cloud = CloudTip::Present(tip("B", 200));
        assert_eq!(
            decide_cloud_sync(None, None, &cloud),
            CloudSyncDecision::FastForwardDownload
        );
    }

    #[test]
    fn backup_tip_parser_picks_latest_child() {
        let yaml = r#"
name: TestGame
drives:
  drive-C: "C:"
backups:
  - name: backup-1
    when: "2026-05-01T10:00:00Z"
    os: windows
    files: {}
    children:
      - name: backup-1-diff-1
        when: "2026-05-02T10:00:00Z"
        os: windows
        files: {}
  - name: backup-2
    when: "2026-05-03T10:00:00Z"
    os: windows
    files: {}
"#;
        let tip = redirects::read_backup_tip_from_str(yaml).unwrap();
        assert_eq!(tip.name, "backup-2");
    }
}

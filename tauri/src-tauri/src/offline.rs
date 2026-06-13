//! Offline mode — the "go offline / go online" switch (Settings → Cloud sync).
//!
//! Going offline PREPARES the device while the network is still up, then flips
//! `ConfigData::offline_mode`:
//!   1. pulls every game's cloud saves down into the local backup store (the
//!      same per-game pull as the "Sync now" button),
//!   2. freshens ludusavi's manifest cache,
//!   3. on Linux, pre-downloads the umu runtime (Steam Linux Runtime container
//!      plus UMU-Proton when no Proton is pinned) so Proton launches work
//!      with no network.
//!
//! While the flag is set, everything network-shaped pauses rather than
//! fails: ludusavi runs without `--cloud-sync`, the rclone control plane
//! no-ops (`resolve_remote` returns `None`), umu skips its runtime-update
//! check, and the metadata backfill is skipped. Offline sessions end on the
//! `local-newer` badge.
//!
//! Going online flips the flag back, re-probes the remote, and reconciles:
//! each game whose local backup tip moved past its cloud baseline is pushed
//! up (the fast-forward upload a launch would do), a cloud that moved instead
//! is pulled, and true divergences are reported for the existing conflict
//! UI to resolve. One caveat is inherent: while a device is offline it can't
//! write session markers, so peers get no "unsynced session elsewhere"
//! warning until it comes back and uploads.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use crate::ludusavi::LudusaviClient;
use crate::runner::{CloudSyncDecision, PullOutcome};
use crate::{ludusavi_config, paths, rclone, redirects, runner};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};

/// Rejects a second go-offline/go-online while one is still running (the UI
/// disables the toggle, but a stuck click or a Decky/UI race shouldn't stack
/// two preparation sweeps).
static TRANSITION_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// RAII release for [`TRANSITION_IN_FLIGHT`].
struct TransitionGuard;

impl TransitionGuard {
    fn acquire() -> AppResult<Self> {
        if TRANSITION_IN_FLIGHT.swap(true, Ordering::SeqCst) {
            return Err(AppError::Other(
                "An offline-mode switch is already in progress.".into(),
            ));
        }
        Ok(Self)
    }
}

impl Drop for TransitionGuard {
    fn drop(&mut self) {
        TRANSITION_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

/// Progress event payload for the Settings UI, emitted as `offline:prep`
/// during both transitions. `current`/`total` are per-stage counters (0/0 for
/// stages without one, e.g. the runtime download).
#[derive(Debug, Clone, Serialize)]
struct PrepProgress<'a> {
    stage: &'a str,
    detail: String,
    current: usize,
    total: usize,
}

fn emit_progress(app: &AppHandle, stage: &str, detail: String, current: usize, total: usize) {
    let _ = app.emit(
        "offline:prep",
        PrepProgress {
            stage,
            detail,
            current,
            total,
        },
    );
}

/// One per-game problem from a prepare/reconcile sweep, surfaced in the
/// report so the user knows which games aren't covered.
#[derive(Debug, Clone, Serialize)]
pub struct GameIssue {
    pub game_name: String,
    pub error: String,
}

/// What `go_offline` did. Mirrored in `types.ts`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct GoOfflineReport {
    /// Games whose saves were pulled down (cloud was ahead).
    pub pulled: Vec<String>,
    /// Games already matching the cloud (nothing to do).
    pub up_to_date: usize,
    /// Games whose local saves were already ahead of the cloud — left as-is.
    pub local_newer: Vec<String>,
    /// Games with a true local-vs-cloud divergence — their saves were NOT
    /// refreshed; resolve via the usual conflict flow.
    pub conflicts: Vec<String>,
    /// Games whose pull failed outright (network blip, game running, …).
    pub errors: Vec<GameIssue>,
    /// Whether the ludusavi manifest cache was freshened.
    pub manifest_refreshed: bool,
    /// Linux Proton runtime warm-up: "ready", "skipped" (no Proton games /
    /// not Linux), or "failed: <reason>".
    pub proton_runtime: String,
    /// False when no cloud remote is configured (the save pull was skipped —
    /// there is nothing to pull; the flag still flips so launches stop
    /// probing the network).
    pub cloud_configured: bool,
}

/// What `go_online` did. Mirrored in `types.ts`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct GoOnlineReport {
    /// Whether the remote answered the re-probe. When false the reconcile was
    /// skipped — the flag is back off, so the next launch / "Sync now" will
    /// reconcile once the network is really there.
    pub reachable: bool,
    /// Games whose offline saves were uploaded to the cloud.
    pub uploaded: Vec<String>,
    /// Games where the cloud had moved ahead instead — pulled down.
    pub pulled: Vec<String>,
    /// Games where both sides moved — left for the conflict UI.
    pub conflicts: Vec<String>,
    /// Games whose reconcile failed outright.
    pub errors: Vec<GameIssue>,
}

/// Resolve the ludusavi sidecar + config dir, shared by both commands.
fn ludusavi_paths() -> AppResult<(PathBuf, PathBuf)> {
    let exe = paths::resolve_ludusavi_path()
        .ok_or_else(|| AppError::Other("Ludusavi sidecar not found — reinstall Spool.".into()))?;
    Ok((exe, paths::ludusavi_config_dir()))
}

/// Flip `offline_mode` in the shared config and persist it. The save must
/// succeed — a flag that only flipped in memory would diverge from what the
/// attached `--run` / headless processes read from disk.
fn set_offline_flag(app: &AppHandle, value: bool) -> AppResult<()> {
    let cfg = app.state::<SharedConfig>();
    let mut g = cfg.lock().map_err(|_| AppError::LockPoisoned)?;
    if g.data.offline_mode == value {
        return Ok(());
    }
    g.data.offline_mode = value;
    g.save()
}

/// Prepare for offline play, then turn offline mode on. See the module docs
/// for the steps. Per-game problems never abort the sweep — they're collected
/// into the report so one flaky game can't block going offline.
#[tauri::command]
pub async fn go_offline(app: AppHandle) -> AppResult<GoOfflineReport> {
    let _guard = TransitionGuard::acquire()?;
    let (ludusavi_exe, config_dir) = ludusavi_paths()?;
    let library = app.state::<SharedLibrary>();
    let ludusavi_client = app.state::<LudusaviClient>();
    let mut report = GoOfflineReport {
        cloud_configured: ludusavi_config::cloud_remote_is_configured(),
        ..Default::default()
    };

    let entries = library.list().await?;

    // ── 1. Pull every game's cloud saves down (runs BEFORE the flag flips —
    //       pulls are network ops and would no-op afterwards) ───────────────
    if report.cloud_configured {
        let total = entries.len();
        for (i, entry) in entries.iter().enumerate() {
            emit_progress(
                &app,
                "saves",
                format!("Syncing saves — {}", entry.game_name),
                i + 1,
                total,
            );
            match runner::pull_cloud_saves_core(
                &ludusavi_client,
                &ludusavi_exe,
                &config_dir,
                &library,
                &entry.id,
            )
            .await
            {
                Ok(res) => match res.outcome {
                    PullOutcome::Pulled => report.pulled.push(entry.game_name.clone()),
                    PullOutcome::UpToDate | PullOutcome::Unconfigured => report.up_to_date += 1,
                    PullOutcome::LocalNewer => report.local_newer.push(entry.game_name.clone()),
                },
                Err(e) => {
                    let msg = e.to_string();
                    // pull_cloud_saves_core reports a true divergence as a
                    // "cloud sync conflict" error — split those out so the UI
                    // can name them distinctly from transport failures.
                    if msg.to_lowercase().contains("conflict") {
                        report.conflicts.push(entry.game_name.clone());
                    } else {
                        report.errors.push(GameIssue {
                            game_name: entry.game_name.clone(),
                            error: msg,
                        });
                    }
                }
            }
        }
    }

    // ── 2. Freshen the ludusavi manifest cache (best-effort) ───────────────
    emit_progress(
        &app,
        "manifest",
        "Updating save-location manifest…".into(),
        0,
        0,
    );
    match ludusavi_client
        .manifest_update(&ludusavi_exe, &config_dir)
        .await
    {
        Ok(()) => report.manifest_refreshed = true,
        Err(e) => {
            tracing::warn!(error = %e, "offline prep: manifest update failed (cached copy will be used)")
        }
    }

    // ── 3. Linux: pre-download the umu runtime for Proton games ────────────
    report.proton_runtime = "skipped".into();
    if cfg!(target_os = "linux")
        && entries
            .iter()
            .any(|e| crate::proton::exe_needs_proton(&e.exe_path))
    {
        emit_progress(
            &app,
            "proton",
            "Downloading the Proton runtime (first time can take a while)…".into(),
            0,
            0,
        );
        let (umu_run_path, default_proton_path) = {
            let cfg = app.state::<SharedConfig>();
            let g = cfg.lock().map_err(|_| AppError::LockPoisoned)?;
            (
                g.data.launch.umu_run_path.clone(),
                g.data.launch.default_proton_path.clone(),
            )
        };
        report.proton_runtime =
            match crate::proton::warm_offline_runtime(&umu_run_path, &default_proton_path).await {
                Ok(()) => "ready".into(),
                Err(e) => format!("failed: {e}"),
            };
    }

    // ── 4. Flip the flag + repaint the sync status ──────────────────────────
    set_offline_flag(&app, true)?;
    rclone::publish_offline_mode_status(&app);
    let _ = app.emit("library:changed", &());

    tracing::info!(
        pulled = report.pulled.len(),
        up_to_date = report.up_to_date,
        local_newer = report.local_newer.len(),
        conflicts = report.conflicts.len(),
        errors = report.errors.len(),
        proton = %report.proton_runtime,
        "offline mode ON"
    );
    Ok(report)
}

/// Turn offline mode off, re-probe the remote, and reconcile what was played
/// while offline. See the module docs. The flag flips FIRST — even if the
/// network still isn't there, staying flagged-offline would also block the
/// per-launch reconcile that mops up after a failed sweep here.
#[tauri::command]
pub async fn go_online(app: AppHandle) -> AppResult<GoOnlineReport> {
    let _guard = TransitionGuard::acquire()?;
    let mut report = GoOnlineReport::default();

    set_offline_flag(&app, false)?;

    // Re-probe (also repaints the chrome icon via sync:status-changed).
    emit_progress(&app, "probe", "Checking the cloud remote…".into(), 0, 0);
    rclone::poll_once(&app).await;
    report.reachable = matches!(
        app.state::<rclone::SyncStatusState>()
            .snapshot()
            .reachability,
        rclone::SyncReachability::Online
    );
    if !report.reachable {
        // Unconfigured or unreachable — nothing to reconcile against. The
        // next launch / manual sync reconciles per game once it can.
        let _ = app.emit("library:changed", &());
        tracing::info!("offline mode OFF (remote not reachable — reconcile deferred)");
        return Ok(report);
    }

    let (ludusavi_exe, config_dir) = ludusavi_paths()?;
    let library = app.state::<SharedLibrary>();
    let ludusavi_client = app.state::<LudusaviClient>();
    let backup_dir = ludusavi_config::backup_dir();

    // Candidates: every game whose local backup tip no longer matches its
    // cloud-sync baseline. That covers offline sessions (workflow backups),
    // manual offline backups, and never-synced games — without trusting the
    // badge alone.
    let entries = library.list().await?;
    let total = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let local_tip = redirects::read_local_backup_tip_async(&backup_dir, &entry.game_name).await;
        let Some(tip) = local_tip.as_ref() else {
            continue; // no local backups at all — nothing this device could owe
        };
        if entry.cloud_sync_baseline.as_deref() == Some(tip.name.as_str()) {
            continue; // local tip is the last-synced tip — already reconciled
        }

        emit_progress(
            &app,
            "reconcile",
            format!("Syncing saves — {}", entry.game_name),
            i + 1,
            total,
        );

        let cloud_tip = runner::fetch_cloud_backup_tip(&entry.game_name).await;
        let decision = runner::decide_cloud_sync(
            entry.cloud_sync_baseline.as_deref(),
            local_tip.as_ref(),
            &cloud_tip,
        );
        tracing::info!(game_name = %entry.game_name, ?decision, "go_online reconcile");
        match decision {
            CloudSyncDecision::InSync => {
                let _ = runner::set_baseline_in(&library, &entry.id, &tip.name).await;
                runner::mark_synced_badge(&library, &entry.id).await;
            }
            CloudSyncDecision::FastForwardUpload => {
                // Serialise against any other Spool process's ludusavi/rclone
                // work, like every backup/upload pair (see proc_lock.rs).
                let upload = async {
                    let _lock =
                        crate::proc_lock::acquire_backup(std::time::Duration::from_secs(180))
                            .await?;
                    ludusavi_client
                        .cloud_resolve(
                            &ludusavi_exe,
                            &config_dir,
                            crate::ludusavi::CloudOp::Upload,
                            &entry.game_name,
                        )
                        .await
                }
                .await;
                match upload {
                    Ok(out)
                        if out
                            .errors
                            .as_ref()
                            .is_none_or(|e| e.cloud_sync_failed.is_none()) =>
                    {
                        let _ = runner::set_baseline_in(&library, &entry.id, &tip.name).await;
                        runner::mark_synced_badge(&library, &entry.id).await;
                        // We're the latest backer now — clear any stale
                        // session marker + record it for peers.
                        rclone::complete_session_backup(&app, &entry.game_name).await;
                        report.uploaded.push(entry.game_name.clone());
                    }
                    Ok(_) => report.errors.push(GameIssue {
                        game_name: entry.game_name.clone(),
                        error: "cloud upload failed".into(),
                    }),
                    Err(e) => report.errors.push(GameIssue {
                        game_name: entry.game_name.clone(),
                        error: e.to_string(),
                    }),
                }
            }
            CloudSyncDecision::FastForwardDownload => {
                // Cloud moved while we were offline and local didn't — the
                // normal pull handles it (download + restore + baseline).
                match runner::pull_cloud_saves_core(
                    &ludusavi_client,
                    &ludusavi_exe,
                    &config_dir,
                    &library,
                    &entry.id,
                )
                .await
                {
                    Ok(_) => report.pulled.push(entry.game_name.clone()),
                    Err(e) => report.errors.push(GameIssue {
                        game_name: entry.game_name.clone(),
                        error: e.to_string(),
                    }),
                }
            }
            CloudSyncDecision::Diverged => report.conflicts.push(entry.game_name.clone()),
        }
    }

    // Fold cross-device state (playtime history, device blobs, custom saves)
    // now that the control plane is live again — the same sweep startup runs.
    rclone::spawn_startup_fold(app.clone());

    let _ = app.emit("library:changed", &());
    tracing::info!(
        uploaded = report.uploaded.len(),
        pulled = report.pulled.len(),
        conflicts = report.conflicts.len(),
        errors = report.errors.len(),
        "offline mode OFF"
    );
    Ok(report)
}

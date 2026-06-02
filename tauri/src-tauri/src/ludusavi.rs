//! Ludusavi CLI integration — subprocess invocation, JSON DTO parsing,
//! manifest cache, and the Add Game search/enrich flow.
//!
//! The manifest is ~9 MB of JSON covering ~80,000 games. It's lazy-loaded on
//! first use, parsed into a `HashMap`, and held in an `Arc` so subsequent
//! enrichment is a cheap clone of the pointer rather than the data.
//!
//! Save backup/restore (used by the Run workflow) will live here too once
//! that slice is built; for now this module focuses on Add Game.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::State;
use tokio::process::Command;
use tokio::sync::RwLock;

/// Hard ceiling on a single `ludusavi {restore,backup} --api --cloud-sync`
/// invocation. These shell out to rclone for the cloud step, which can hang
/// indefinitely on a dropped/flaky network (common on a handheld) — leaving
/// the Game-Mode launch frozen on "Restoring saves…" forever. We kill the
/// stuck child after this window and surface a clean error instead.
///
/// This is the backstop; the first line of defence is the fast-fail rclone
/// timeout flags injected by [`crate::ludusavi_config::ensure_rclone_timeouts`],
/// which make `--cloud-sync` give up in seconds so ludusavi can still complete
/// the local restore.
const RUN_API_TIMEOUT: Duration = Duration::from_secs(60);

/// Direction of a cloud-conflict resolution — which copy of the save wins.
/// Maps onto the `cloud upload` / `cloud download` ludusavi subcommands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudOp {
    /// Keep this device's saves: push local backups up, overwriting the cloud.
    Upload,
    /// Keep the cloud's saves: pull cloud backups down, overwriting local.
    Download,
}

impl CloudOp {
    /// The ludusavi `cloud` subcommand name for this direction.
    fn subcommand(self) -> &'static str {
        match self {
            CloudOp::Upload => "upload",
            CloudOp::Download => "download",
        }
    }

    /// Parse the frontend's side token. `"local"` keeps this device (upload);
    /// `"cloud"` keeps the cloud (download).
    pub fn from_side(side: &str) -> AppResult<Self> {
        match side {
            "local" => Ok(CloudOp::Upload),
            "cloud" => Ok(CloudOp::Download),
            other => Err(AppError::Other(format!(
                "invalid conflict resolution side: {other:?} (expected \"local\" or \"cloud\")"
            ))),
        }
    }
}

// ── DTOs: `ludusavi {restore,backup} --api` output ──────────────────────────

/// Parsed `--api` output from `ludusavi restore` / `ludusavi backup`.
/// All fields default to empty so a partial / unexpected response still
/// deserializes — only the few signals the workflow cares about matter.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ApiOutput {
    pub errors: Option<ApiErrors>,
    pub overall: Option<ApiOverall>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ApiErrors {
    #[serde(rename = "unknownGames")]
    pub unknown_games: Vec<String>,
    #[serde(rename = "cloudConflict")]
    pub cloud_conflict: Option<serde_json::Value>,
    #[serde(rename = "cloudSyncFailed")]
    pub cloud_sync_failed: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ApiOverall {
    #[serde(rename = "totalGames")]
    pub total_games: i32,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    #[serde(rename = "processedGames")]
    pub processed_games: i32,
}

// ── DTOs: `ludusavi find --api` output ──────────────────────────────────────

#[derive(Debug, Deserialize)]
struct FindOutput {
    #[serde(default)]
    games: HashMap<String, FindHit>,
}

#[derive(Debug, Deserialize)]
struct FindHit {
    #[serde(default)]
    score: f64,
}

// ── DTOs: `ludusavi backups --api` output ───────────────────────────────────

/// Parsed `ludusavi backups --api` output. Maps each game name to the list of
/// backups ludusavi actually has on disk for it. This is the authoritative
/// source for "how many save revisions exist" and "when was the last one" —
/// it reflects ludusavi's real backup store, including backups Spool didn't
/// make itself.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct BackupsOutput {
    games: HashMap<String, GameBackups>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct GameBackups {
    backups: Vec<ApiBackup>,
}

#[derive(Debug, Deserialize)]
struct ApiBackup {
    /// ludusavi's unique id for the backup — this is exactly the token
    /// `restore --backup <name>` accepts. The most-recent full is reported
    /// as `"."`; older fulls carry a folder-derived name.
    #[serde(default)]
    name: String,
    when: DateTime<Utc>,
}

/// Reduce a `ludusavi backups` response to authoritative stats: the total
/// revision count and the most recent backup timestamp. ludusavi keys results
/// by its own canonical game name, so we flatten whatever it returned rather
/// than matching names — callers query a single game at a time.
fn reduce_backups(out: BackupsOutput) -> BackupStats {
    let backups: Vec<ApiBackup> = out.games.into_values().flat_map(|g| g.backups).collect();
    BackupStats {
        count: backups.len() as i32,
        last_backed_up_at: backups.iter().map(|b| b.when).max(),
    }
}

/// Authoritative save-backup stats for a game, derived from `ludusavi backups`.
#[derive(Debug, Clone, Default)]
pub struct BackupStats {
    /// Number of backups (revisions) ludusavi currently retains for the game.
    pub count: i32,
    /// Timestamp of the most recent backup, if any.
    pub last_backed_up_at: Option<DateTime<Utc>>,
}

/// A single restorable save revision, surfaced to the UI so the user can roll
/// back to an earlier save. `name` is ludusavi's backup id (the token passed to
/// `restore --backup`); `is_current` marks the tip (the revision a normal
/// pre-launch restore would land).
#[derive(Debug, Clone, Serialize)]
pub struct SaveRevision {
    pub name: String,
    pub when: DateTime<Utc>,
    pub is_current: bool,
}

/// Flatten a `ludusavi backups` response into a newest-first revision list.
/// The single newest entry (by `when`) is flagged `is_current` — that's the
/// tip a normal restore would land, so the UI can mark it and disable rollback
/// to it. Since retention runs with `differential: 0`, every entry is a full,
/// independently-restorable backup.
fn revisions_from(out: BackupsOutput) -> Vec<SaveRevision> {
    let mut revs: Vec<SaveRevision> = out
        .games
        .into_values()
        .flat_map(|g| g.backups)
        .map(|b| SaveRevision {
            name: b.name,
            when: b.when,
            is_current: false,
        })
        .collect();
    // Newest first.
    revs.sort_by_key(|r| std::cmp::Reverse(r.when));
    if let Some(tip) = revs.first_mut() {
        tip.is_current = true;
    }
    revs
}

// ── DTOs: `ludusavi manifest show --api` output ─────────────────────────────

/// One game's entry in the ludusavi manifest. All fields are best-effort;
/// missing data simply degrades the candidate's enrichment rather than
/// failing the search.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ManifestEntry {
    pub files: HashMap<String, ManifestFileEntry>,
    #[serde(rename = "installDir")]
    pub install_dir: HashMap<String, serde_json::Value>,
    pub steam: Option<StoreRef>,
    pub gog: Option<StoreRef>,
    pub id: Option<ManifestIds>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ManifestFileEntry {
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct StoreRef {
    pub id: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ManifestIds {
    pub lutris: Option<String>,
    #[serde(default)]
    pub gog: Vec<u64>,
}

// ── Public types: search results sent to the frontend ───────────────────────

/// Enriched search candidate — combines a `find --api` hit's score with the
/// manifest's metadata (store ids, save paths, cloud sync flag, …).
#[derive(Debug, Clone, Serialize)]
pub struct SearchCandidate {
    pub name: String,
    /// Fuzzy match score, 0.0–1.0 from ludusavi.
    pub score: f64,
    /// Primary save path (the first manifest entry tagged "save"), in
    /// human-readable form. None when the game has no save info or wasn't
    /// in the manifest.
    pub save_path: Option<String>,
    /// All save paths from the manifest, in display form.
    pub save_paths: Vec<String>,
    pub steam_id: Option<u64>,
    pub gog_id: Option<u64>,
    pub lutris_slug: Option<String>,
    /// The folder name ludusavi expects ("Hades" for `D:\Games\Hades\`).
    /// The first of `manifest_install_dirs`, kept for existing consumers.
    pub manifest_install_dir: Option<String>,
    /// Every install-folder name ludusavi lists for this game (the manifest's
    /// `installDir` keys). Used to locate the game's real install root among
    /// the picked exe's ancestor folders.
    pub manifest_install_dirs: Vec<String>,
    /// The picked exe's ancestor directory that matches one of
    /// `manifest_install_dirs` — the detected install root, even when the exe
    /// sits under `Binaries/Win64`. Only set by `search_by_exe` (which knows
    /// the exe path); None for manual name searches. The Add flow defaults the
    /// install folder to this when present.
    pub install_root: Option<String>,
}

// ── Client ──────────────────────────────────────────────────────────────────

/// Ludusavi integration handle. Stateless apart from the lazy manifest
/// cache — methods take the resolved sidecar path as a parameter.
type ManifestCache = Arc<RwLock<Option<Arc<HashMap<String, ManifestEntry>>>>>;

pub struct LudusaviClient {
    /// Lazy-loaded full manifest. `Arc<HashMap>` so handing it out is cheap;
    /// inner `RwLock` so the first load is concurrent-safe.
    manifest: ManifestCache,
}

impl LudusaviClient {
    pub fn new() -> Self {
        Self {
            manifest: Arc::new(RwLock::new(None)),
        }
    }

    /// Searches the ludusavi manifest for games matching `query`. Combines
    /// `find --api --fuzzy --multiple` (for ranking) with the cached manifest
    /// (for metadata enrichment) and returns the merged result sorted by
    /// descending score.
    pub async fn search(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        query: &str,
    ) -> AppResult<Vec<SearchCandidate>> {
        self.search_multi(ludusavi_exe, config_dir, &[query.to_string()])
            .await
    }

    /// Like [`search`], but runs several queries and merges their hits into
    /// one ranked candidate list. A game matched by more than one query
    /// keeps its best score, and the manifest is loaded once for all of
    /// them. Used by the Add Game flow to look up both the exe filename and
    /// its parent folder name.
    pub async fn search_multi(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        queries: &[String],
    ) -> AppResult<Vec<SearchCandidate>> {
        let manifest = self.manifest_or_load(ludusavi_exe, config_dir).await?;

        // Merge hits keyed by game name, keeping the highest score when a
        // game matches more than one query.
        let mut best: HashMap<String, f64> = HashMap::new();
        for query in queries {
            if query.trim().is_empty() {
                continue;
            }
            let hits = run_find(ludusavi_exe, config_dir, query).await?;
            for (name, hit) in hits.games {
                best.entry(name)
                    .and_modify(|s| {
                        if hit.score > *s {
                            *s = hit.score;
                        }
                    })
                    .or_insert(hit.score);
            }
        }

        let mut candidates: Vec<SearchCandidate> = best
            .into_iter()
            .map(|(name, score)| {
                let entry = manifest.get(&name);
                enrich(name, score, entry)
            })
            .collect();

        // Descending score; ludusavi already returns ranked, but it sorts
        // by name when scores tie. Cap at 20 — UI won't scroll forever.
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(20);
        Ok(candidates)
    }

    /// Runs `ludusavi restore --api --cloud-sync --force <name>` and parses
    /// the JSON output. Empty stdout (which ludusavi sometimes produces for
    /// unknown-game errors) deserialises to an empty `ApiOutput` rather
    /// than failing.
    pub async fn restore(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        game_name: &str,
    ) -> AppResult<ApiOutput> {
        run_api(ludusavi_exe, config_dir, &["restore", "--api", "--cloud-sync", "--force", game_name]).await
    }

    /// Runs `ludusavi restore --api --backup <id> --force <name>` — a restore
    /// of a *specific* revision. Deliberately omits `--cloud-sync`: this is a
    /// local rollback, and the caller pins the result by following up with a
    /// (cloud-syncing) backup so the rolled-back state becomes the new tip.
    /// Pulling the cloud here would re-land the newest revision and defeat the
    /// rollback.
    pub async fn restore_backup(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        game_name: &str,
        backup_name: &str,
    ) -> AppResult<ApiOutput> {
        run_api(
            ludusavi_exe,
            config_dir,
            &["restore", "--api", "--backup", backup_name, "--force", game_name],
        )
        .await
    }

    /// Runs `ludusavi backup --api --cloud-sync --force <name>` and optionally
    /// passes `--wine-prefix <prefix>` for Proton games so ludusavi finds saves
    /// inside the prefix's drive_c tree. The single call both writes the local
    /// revision and mirrors it to the cloud remote.
    pub async fn backup(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        game_name: &str,
        wine_prefix: Option<&Path>,
    ) -> AppResult<ApiOutput> {
        self.run_backup(ludusavi_exe, config_dir, game_name, wine_prefix, true).await
    }

    /// Runs `ludusavi backup --api --force <name>` *without* `--cloud-sync`, so
    /// it only writes the local revision and never touches the cloud remote.
    /// The play workflow uses this to split the post-session backup into two
    /// observable steps — local write, then a separate cloud upload — so the
    /// splash can show real progress for each instead of one combined call that
    /// blocks silently while it uploads.
    pub async fn backup_local(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        game_name: &str,
        wine_prefix: Option<&Path>,
    ) -> AppResult<ApiOutput> {
        self.run_backup(ludusavi_exe, config_dir, game_name, wine_prefix, false).await
    }

    /// Shared backup invocation. `cloud_sync` toggles the `--cloud-sync` flag
    /// that mirrors the freshly-written revision to the configured remote.
    async fn run_backup(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        game_name: &str,
        wine_prefix: Option<&Path>,
        cloud_sync: bool,
    ) -> AppResult<ApiOutput> {
        let mut args = vec!["backup", "--api"];
        if cloud_sync {
            args.push("--cloud-sync");
        }
        args.push("--force");
        args.push(game_name);
        let prefix_str;
        if let Some(pfx) = wine_prefix {
            prefix_str = pfx.to_string_lossy().into_owned();
            args.push("--wine-prefix");
            args.push(&prefix_str);
        }
        run_api(ludusavi_exe, config_dir, &args).await
    }

    /// Resolve a cloud-sync conflict in one direction, overwriting the losing
    /// side, then parse the `--api` envelope.
    ///
    ///   [`CloudOp::Upload`]   → `cloud upload --api --force <name>`
    ///                           local backups overwrite the cloud (keep this
    ///                           device's saves).
    ///   [`CloudOp::Download`] → `cloud download --api --force <name>`
    ///                           cloud backups overwrite local (keep the cloud's
    ///                           saves).
    ///
    /// `--force` is what makes this non-interactive: it skips ludusavi's own
    /// conflict guard (the very guard that produced the `cloudConflict` we're
    /// resolving) and unconditionally mirrors the chosen side. Restricted to the
    /// single `game_name` so we never touch unrelated games in the library.
    pub async fn cloud_resolve(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        op: CloudOp,
        game_name: &str,
    ) -> AppResult<ApiOutput> {
        run_api(
            ludusavi_exe,
            config_dir,
            &["cloud", op.subcommand(), "--api", "--force", game_name],
        )
        .await
    }

    /// Runs `ludusavi backups --api <name>` and reduces it to authoritative
    /// stats: the real revision count and the latest backup timestamp. Unlike
    /// the old Spool-maintained counters, this reflects ludusavi's actual
    /// backup store, so it stays correct across externally-made backups,
    /// pruned revisions, and freshly-added/migrated library entries.
    pub async fn list_backups(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        game_name: &str,
    ) -> AppResult<BackupStats> {
        let out = run_backups(ludusavi_exe, config_dir, game_name).await?;
        Ok(reduce_backups(out))
    }

    /// Runs `ludusavi backups --api <name>` and returns the full, newest-first
    /// revision list (rather than reducing to a count). Backs the in-app
    /// "restore an earlier save" picker. Reflects ludusavi's local backup
    /// store, so cloud-only revisions this device hasn't pulled aren't listed.
    pub async fn list_revisions(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        game_name: &str,
    ) -> AppResult<Vec<SaveRevision>> {
        let out = run_backups(ludusavi_exe, config_dir, game_name).await?;
        Ok(revisions_from(out))
    }

    async fn manifest_or_load(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
    ) -> AppResult<Arc<HashMap<String, ManifestEntry>>> {
        // Read-side fast path.
        {
            let guard = self.manifest.read().await;
            if let Some(m) = guard.as_ref() {
                return Ok(Arc::clone(m));
            }
        }
        // Slow path: load the manifest from ludusavi. Cheap to hold the
        // write lock since this only happens once per session.
        let manifest = load_manifest(ludusavi_exe, config_dir).await?;
        let arc = Arc::new(manifest);
        let mut guard = self.manifest.write().await;
        *guard = Some(Arc::clone(&arc));
        Ok(arc)
    }
}

impl Default for LudusaviClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── Subprocess helpers ──────────────────────────────────────────────────────

/// Builds a `Command` pre-loaded with `--config <config_dir>` so every
/// ludusavi invocation uses Spool's owned config (backup path, cloud remote,
/// redirects) rather than the user's personal ludusavi config.
///
/// On Windows also sets `CREATE_NO_WINDOW` to avoid a console flash.
fn hidden_command(exe: &Path, config_dir: &Path) -> Command {
    let mut cmd = Command::new(exe);
    cmd.args(["--config", &config_dir.to_string_lossy()]);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW — winbase.h. Avoids a winapi dep for one constant.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

async fn run_find(ludusavi_exe: &Path, config_dir: &Path, query: &str) -> AppResult<FindOutput> {
    let output = hidden_command(ludusavi_exe, config_dir)
        .args(["find", "--api", "--fuzzy", "--multiple", query])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("failed to spawn ludusavi: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() && stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(format!(
            "ludusavi find failed: {}",
            stderr.trim()
        )));
    }
    if stdout.trim().is_empty() {
        // ludusavi exits non-zero on no matches; surface an empty set.
        return Ok(FindOutput { games: HashMap::new() });
    }
    crate::util::parse_json(stdout.as_bytes(), "ludusavi find output")
}

/// Runs `ludusavi backups --api <name>` and parses the result. A game with no
/// backups (or unknown to ludusavi) produces empty/non-zero output, which we
/// treat as "no backups" rather than an error — the detail card handles a zero
/// count gracefully.
async fn run_backups(
    ludusavi_exe: &Path,
    config_dir: &Path,
    game_name: &str,
) -> AppResult<BackupsOutput> {
    let output = hidden_command(ludusavi_exe, config_dir)
        .args(["backups", "--api", game_name])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("failed to spawn ludusavi backups: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        // ludusavi can exit non-zero with no output for an unknown game.
        return Ok(BackupsOutput::default());
    }
    crate::util::parse_json(stdout.as_bytes(), "ludusavi backups output")
}

/// Generic runner for ludusavi subcommands that emit the `--api` envelope
/// shared by `restore` and `backup`. Treats empty stdout as a successful
/// no-op (ludusavi sometimes exits non-zero with no output when the game
/// isn't in its manifest, which we surface as "no saves to handle").
async fn run_api(ludusavi_exe: &Path, config_dir: &Path, args: &[&str]) -> AppResult<ApiOutput> {
    // The subcommand ("restore" / "backup") for log/error messages — the first
    // non-flag arg, since global flags like `--no-manifest-update` precede it.
    let op = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .copied()
        .unwrap_or("operation");
    // Run ludusavi as a BLOCKING std::process call on a spawn_blocking thread
    // rather than via tokio::process.
    //
    // tokio::process reaps child exits via the runtime's SIGCHLD/IO driver,
    // which only makes progress while the runtime is actively driven. In the
    // standalone SteamOS Game-Mode attached launch the process is near-idle
    // (just a splash window — no main window, no pollers), so the driver wasn't
    // ticking: ludusavi had already exited in ~1.5s but `cmd.output().await`
    // didn't observe it for ~40s, leaving the launch stuck on "Restoring saves".
    // A synchronous `waitpid` via std::process doesn't depend on the async
    // runtime at all, so the wait returns the instant ludusavi exits.
    let exe = ludusavi_exe.to_path_buf();
    let cfg = config_dir.to_path_buf();
    let owned_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let started = std::time::Instant::now();

    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("--config").arg(&cfg);
    cmd.args(&owned_args);
    crate::capture_stdio!(cmd);

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => return Err(AppError::Other(format!("failed to spawn ludusavi: {e}"))),
    };

    let mut stdout_pipe = child.stdout.take().unwrap();
    let mut stderr_pipe = child.stderr.take().unwrap();

    let child_arc = Arc::new(std::sync::Mutex::new(Some(child)));
    let child_clone = child_arc.clone();

    let join = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();
        
        let _ = stdout_pipe.read_to_end(&mut stdout_bytes);
        let _ = stderr_pipe.read_to_end(&mut stderr_bytes);

        let mut child_opt = child_clone.lock().unwrap();
        let status = if let Some(mut c) = child_opt.take() {
            c.wait()
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "ludusavi timed out",
            ));
        };
        
        status.map(|status| std::process::Output {
            status,
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        })
    });

    let output = match tokio::time::timeout(RUN_API_TIMEOUT, join).await {
        Ok(Ok(Ok(out))) => out,
        Ok(Ok(Err(e))) => return Err(AppError::Other(format!("failed to execute ludusavi: {e}"))),
        Ok(Err(e)) => return Err(AppError::Other(format!("ludusavi task panicked: {e}"))),
        Err(_) => {
            let mut lock = child_arc.lock().unwrap();
            if let Some(mut c) = lock.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            return Err(AppError::Other(format!(
                "ludusavi {op} timed out after {}s — the network or cloud sync may be unavailable.",
                RUN_API_TIMEOUT.as_secs()
            )));
        }
    };
    // Record how long the restore/backup subprocess took + its exit code. Cheap
    // (only fires on the run-workflow path) and makes a slow or failing
    // cloud-sync visible in debug.log without re-instrumenting.
    tracing::info!(
        op,
        elapsed_ms = started.elapsed().as_millis() as u64,
        exit = output.status.code().unwrap_or(-1),
        "ludusavi {op} finished"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() && stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(format!(
            "ludusavi execution failed: {}",
            stderr.trim()
        )));
    }
    if stdout.trim().is_empty() {
        return Ok(ApiOutput::default());
    }
    crate::util::parse_json(stdout.as_bytes(), "ludusavi output")
}

async fn load_manifest(ludusavi_exe: &Path, config_dir: &Path) -> AppResult<HashMap<String, ManifestEntry>> {
    let output = hidden_command(ludusavi_exe, config_dir)
        .args(["manifest", "show", "--api"])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("failed to spawn ludusavi manifest: {e}")))?;
    if !output.status.success() {
        return Err(AppError::Other(format!(
            "ludusavi manifest exited {}",
            output.status.code().unwrap_or(-1)
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| AppError::Other(format!("ludusavi manifest output not utf-8: {e}")))?;
    crate::util::parse_json(stdout.as_bytes(), "ludusavi manifest")
}

// ── Enrichment + helpers ────────────────────────────────────────────────────

fn enrich(
    name: String,
    score: f64,
    entry: Option<&ManifestEntry>,
) -> SearchCandidate {
    let Some(entry) = entry else {
        return SearchCandidate {
            name,
            score,
            save_path: None,
            save_paths: Vec::new(),
            steam_id: None,
            gog_id: None,
            lutris_slug: None,
            manifest_install_dir: None,
            manifest_install_dirs: Vec::new(),
            install_root: None,
        };
    };

    // Save paths: every `files` entry whose tags include "save".
    let mut save_paths: Vec<String> = entry
        .files
        .iter()
        .filter(|(_, fe)| fe.tags.iter().any(|t| t == "save"))
        .map(|(path, _)| prettify_save_template(path))
        .collect();
    save_paths.sort(); // stable order for tests/UI

    let save_path = save_paths.first().cloned();
    // All install-folder names ludusavi knows for this game, sorted for a
    // stable primary pick. Most games list exactly one.
    let mut manifest_install_dirs: Vec<String> = entry.install_dir.keys().cloned().collect();
    manifest_install_dirs.sort();
    let manifest_install_dir = manifest_install_dirs.first().cloned();
    let steam_id = entry.steam.as_ref().map(|s| s.id);
    let gog_id = entry
        .gog
        .as_ref()
        .map(|g| g.id)
        .or_else(|| entry.id.as_ref().and_then(|i| i.gog.first().copied()));
    let lutris_slug = entry.id.as_ref().and_then(|i| i.lutris.clone());

    SearchCandidate {
        name,
        score,
        save_path,
        save_paths,
        steam_id,
        gog_id,
        lutris_slug,
        manifest_install_dir,
        manifest_install_dirs,
        // Filled in by `search_by_exe`, which has the exe path to match against.
        install_root: None,
    }
}

/// Translates ludusavi's manifest path tokens to OS-familiar equivalents
/// for display. Unknown tokens are passed through unchanged.
fn prettify_save_template(template: &str) -> String {
    template
        .replace("<winAppData>", "%APPDATA%")
        .replace("<winLocalAppData>", "%LOCALAPPDATA%")
        .replace("<winDocuments>", "%USERPROFILE%/Documents")
        .replace("<winPublic>", "%PUBLIC%")
        .replace("<home>", "%USERPROFILE%")
}

/// Best-effort guess at a ludusavi search query from an exe filename.
/// `nightreign.exe` → `"Nightreign"`, `elden_ring.exe` → `"Elden Ring"`.
pub fn infer_name_from_exe(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    clean_query(stem)
}

/// Normalises a raw filename/folder fragment into a search query:
/// underscores and dashes become spaces, then title-case.
fn clean_query(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    title_case(cleaned.trim())
}

/// Folder names that are structural rather than a game's title. Skipped when
/// walking an exe's ancestors to build search queries — engines bury the real
/// binary under these (Unreal's `Binaries/Win64`, a bare `bin`, Steam's
/// `steamapps/common`), so the title lives in a folder further up.
const GENERIC_DIR_NAMES: &[&str] = &[
    "bin",
    "binaries",
    "win",
    "win32",
    "win64",
    "windows",
    "x64",
    "x86",
    "x86_64",
    "game",
    "games",
    "app",
    "application",
    "build",
    "data",
    "content",
    "release",
    "retail",
    "redist",
    "system",
    "common",
    "steamapps",
    "engine",
];

fn is_generic_dir(name: &str) -> bool {
    GENERIC_DIR_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

/// How far up an exe's ancestors to walk when building search queries.
const MAX_ANCESTOR_LEVELS: usize = 4;

/// Builds ordered, de-duplicated ludusavi search queries from an exe path:
/// the filename stem first, then each ancestor folder name walking upward
/// (skipping structural folders like `bin` / `Binaries/Win64`). Walks at most
/// [`MAX_ANCESTOR_LEVELS`] up so a deeply nested path doesn't fan out forever.
fn infer_queries_from_path(path: &Path) -> Vec<String> {
    let mut queries: Vec<String> = Vec::new();
    let mut push = |q: String| {
        if !q.is_empty() && !queries.contains(&q) {
            queries.push(q);
        }
    };
    push(infer_name_from_exe(path));

    // `ancestors()` yields the path itself first; `skip(1)` starts at the
    // immediate parent directory.
    for ancestor in path.ancestors().skip(1).take(MAX_ANCESTOR_LEVELS) {
        let Some(folder) = ancestor.file_name().and_then(|s| s.to_str()) else {
            continue; // reached a root / prefix component
        };
        // Skip structural folders and drive-letter / single-char fragments
        // (e.g. a Windows `D:`), which only add noise to the search.
        if is_generic_dir(folder) || normalize_dir(folder).len() < 2 {
            continue;
        }
        push(clean_query(folder));
    }
    queries
}

/// Lowercases and strips every non-alphanumeric char so `"Elden Ring"`,
/// `"ELDEN RING"`, and a folder literally named `EldenRing` all compare equal.
fn normalize_dir(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Given an exe path and a game's known install-folder names (the manifest's
/// `installDir` keys), finds the ancestor directory whose name matches one of
/// them — the game's install root, even when the exe is buried under
/// `Binaries/Win64`. Returns the outermost match (the top of the game's tree),
/// or None when nothing matches, in which case the caller keeps the exe's
/// parent directory.
fn resolve_install_root(exe_path: &Path, install_dirs: &[String]) -> Option<String> {
    if install_dirs.is_empty() {
        return None;
    }
    let wanted: Vec<String> = install_dirs.iter().map(|d| normalize_dir(d)).collect();
    let mut best: Option<&Path> = None;
    for ancestor in exe_path.ancestors().skip(1) {
        let Some(folder) = ancestor.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let folder_norm = normalize_dir(folder);
        if !folder_norm.is_empty() && wanted.contains(&folder_norm) {
            best = Some(ancestor); // keep climbing — prefer the outermost match
        }
    }
    best.and_then(|p| p.to_str()).map(str::to_string)
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ── PE VERSIONINFO name extraction ─────────────────────────────────────────

/// `ProductName` / `FileDescription` strings that indicate a game engine or
/// generic application rather than a game's own title. When the PE-derived
/// name matches one of these (case-insensitive, after stripping trademark
/// symbols), we skip it rather than feeding junk into the ludusavi query.
const JUNK_PRODUCT_NAMES: &[&str] = &[
    // Engines
    "unreal engine",
    "unity",
    "unity technologies",
    "gamemaker",
    "gamemaker studio",
    "godot",
    "godot engine",
    "cryengine",
    "rpg maker",
    "rpgmaker",
    "renpy",
    "ren'py",
    "construct",
    "monogame",
    "defold",
    // Generic/installer strings
    "application",
    "setup",
    "installer",
    "uninstaller",
    "uninstall",
    "launcher",
    "redistributable",
    // Runtime/middleware vendors (ProductName = company, not title)
    "microsoft",
    "visual c++",
    "directx",
    "electronic arts",
    "ea games",
    "ubisoft",
    "activision",
    "bethesda",
    "2k games",
    "rockstar games",
    "square enix",
    "denuvo",
    "eac",
    "easy anti-cheat",
    "bink",
    "bink video",
    "miles sound system",
    "steam",
    "steamworks",
];

fn is_junk_product_name(s: &str) -> bool {
    let lower = s.to_lowercase();
    JUNK_PRODUCT_NAMES.iter().any(|&j| lower == j)
}

/// Strips trademark/copyright symbols and collapses whitespace.
fn sanitize_pe_string(raw: &str) -> String {
    raw.chars()
        .filter(|&c| c != '™' && c != '®' && c != '©')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads the embedded PE VERSIONINFO resource from `path` and returns the
/// best name string: `ProductName` when it isn't a generic engine/junk value,
/// otherwise `FileDescription`. Returns `None` when the file can't be parsed
/// as a PE, has no version resource, or neither field yields a useful name.
///
/// This is pure byte-level PE parsing (via `pelite`) — it works on any host
/// OS, so Linux + Proton installations of Windows games are covered too.
///
/// Prefer English (lang_id 0x0409) when present; otherwise use the first
/// available language from the translation table.
pub fn read_exe_product_name(path: &Path) -> Option<String> {
    // Memory-map rather than read the whole file — packed/protected game exes
    // can be hundreds of MB, and PE parsing only touches the headers + resource
    // section, so the OS pages in just what's read.
    let map = pelite::FileMap::open(path).ok()?;
    let bytes = map.as_ref();
    // Try PE32+ (64-bit) first, fall back to PE32 (32-bit).
    pe_product_name_64(bytes).or_else(|| pe_product_name_32(bytes))
}

fn pe_product_name_64(bytes: &[u8]) -> Option<String> {
    use pelite::pe64::Pe;
    let pe = pelite::pe64::PeFile::from_bytes(bytes).ok()?;
    let vi = pe.resources().ok()?.version_info().ok()?;
    pick_name_from_version_info(&vi)
}

fn pe_product_name_32(bytes: &[u8]) -> Option<String> {
    use pelite::pe32::Pe;
    let pe = pelite::pe32::PeFile::from_bytes(bytes).ok()?;
    let vi = pe.resources().ok()?.version_info().ok()?;
    pick_name_from_version_info(&vi)
}

fn pick_name_from_version_info(
    vi: &pelite::resources::version_info::VersionInfo<'_>,
) -> Option<String> {
    let translations = vi.translation();
    // Prefer English (0x0409); fall back to the first available language.
    let lang = translations
        .iter()
        .find(|l| l.lang_id == 0x0409)
        .or_else(|| translations.first())
        .copied()?;

    // ProductName first; skip if it's a junk/engine string.
    // title_case normalises all-caps values (e.g. "PRAGMATA" → "Pragmata")
    // so they score well against the mixed-case names in the ludusavi manifest.
    let product = vi
        .value(lang, "ProductName")
        .map(|s| title_case(&sanitize_pe_string(&s)))
        .filter(|s| !s.is_empty() && !is_junk_product_name(s));

    if product.is_some() {
        return product;
    }

    // FileDescription as the fallback.
    vi.value(lang, "FileDescription")
        .map(|s| title_case(&sanitize_pe_string(&s)))
        .filter(|s| !s.is_empty() && !is_junk_product_name(s))
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn search_games(
    config: State<'_, SharedConfig>,
    ludusavi: State<'_, LudusaviClient>,
    query: String,
) -> AppResult<Vec<SearchCandidate>> {
    let ludusavi_exe = ludusavi_path_or_err(&config)?;
    let config_dir = crate::paths::ludusavi_config_dir();
    ludusavi.search(&ludusavi_exe, &config_dir, query.trim()).await
}

/// Opens ludusavi in GUI mode against Spool's owned config dir so the user
/// can configure the cloud remote, rclone, or inspect the backup state —
/// all within the Spool-managed config rather than their personal one.
#[tauri::command]
pub async fn open_ludusavi_gui(config: State<'_, SharedConfig>) -> AppResult<()> {
    let ludusavi_exe = ludusavi_path_or_err(&config)?;
    let config_dir = crate::paths::ludusavi_config_dir();
    Command::new(&ludusavi_exe)
        .args(["--config", &config_dir.to_string_lossy(), "gui"])
        .spawn()
        .map_err(|e| AppError::Other(format!("failed to spawn ludusavi gui: {e}")))?;
    Ok(())
}

/// Configure ludusavi's owned config to use a WebDAV cloud remote by delegating
/// to `ludusavi cloud set webdav`.
///
/// This is the only correct mechanism: ludusavi creates the backing rclone
/// remote (obscuring the password into rclone.conf) and references it by id in
/// its `config.yaml`. Writing the YAML by hand leaves the password unset and the
/// remote unusable — which is the bug this replaces.
///
/// `provider` is one of ludusavi's WebDAV vendors: `other`, `nextcloud`,
/// `owncloud`, `sharepoint`, `sharepoint-ntlm` (empty → `other`).
///
/// `obscure_password` pre-obscures the password before handing it to ludusavi.
/// This should almost always be `false`: ludusavi/rclone already obscure the
/// password at rest in rclone.conf and reveal it back to plaintext on the wire,
/// so the server receives exactly what the caller passed here. Spool's own
/// self-hosted store validates the basic-auth password verbatim against the
/// account API key (`/internal/webdav-auth` does a plain equality check, no
/// deobfuscation), so it too wants `false`. Only set `true` for a server that
/// itself rclone-reveals the incoming password before validating it.
pub async fn apply_webdav_remote(
    url: &str,
    username: &str,
    password: &str,
    provider: &str,
    obscure_password: bool,
) -> AppResult<()> {
    let ludusavi_exe = crate::paths::resolve_ludusavi_path().ok_or_else(|| {
        AppError::Other("Ludusavi sidecar not found — reinstall Spool.".to_string())
    })?;

    // ludusavi shells out to rclone to obscure the password, so the owned config
    // must point at a usable rclone before we invoke `cloud set`.
    let rclone_exe = crate::paths::resolve_rclone_path().ok_or_else(|| {
        AppError::Other("rclone sidecar not found — reinstall Spool.".to_string())
    })?;
    crate::ludusavi_config::set_cloud(None, None, None, Some(&rclone_exe.to_string_lossy()), None)?;

    let password = if obscure_password {
        let out = Command::new(&rclone_exe)
            .arg("obscure")
            .arg(password)
            .output()
            .await
            .map_err(|e| AppError::Other(format!("failed to run rclone obscure: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(AppError::Other(format!("rclone obscure failed: {}", stderr.trim())));
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        password.to_string()
    };

    let config_dir = crate::paths::ludusavi_config_dir();
    let provider = if provider.trim().is_empty() { "other" } else { provider.trim() };
    let output = Command::new(&ludusavi_exe)
        .args([
            "--config",
            &config_dir.to_string_lossy(),
            "cloud",
            "set",
            "webdav",
            "--url",
            url,
            "--username",
            username,
            "--password",
            &password,
            "--provider",
            provider,
        ])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("failed to run ludusavi cloud set webdav: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(format!(
            "ludusavi cloud set webdav failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

/// Manually configure a WebDAV cloud remote from the settings form. Persists the
/// active provider + connection (sans password) so the UI reflects it.
#[tauri::command]
pub async fn set_cloud_webdav(
    config: State<'_, SharedConfig>,
    url: String,
    username: String,
    password: String,
    provider: String,
) -> AppResult<()> {
    // Manual WebDAV (Nextcloud, ownCloud, …) expects the plaintext password.
    apply_webdav_remote(&url, &username, &password, &provider, false).await?;
    let mut cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
    cfg.data.cloud.provider = "webdav".to_string();
    cfg.data.cloud.webdav_url = url;
    cfg.data.cloud.webdav_username = username;
    cfg.save()?;
    Ok(())
}

/// Identifies a game from its exe path using three query sources, merged and
/// ranked by ludusavi's fuzzy scorer:
///
/// 1. **PE VERSIONINFO** (`ProductName` / `FileDescription`) — highest signal;
///    a generic launcher like `start_protected_game.exe` may embed the real
///    title. Read in a blocking thread so the async executor isn't stalled.
/// 2. **Exe filename stem** — `elden_ring.exe` → "Elden Ring".
/// 3. **Ancestor folder names** — walks up to four levels, skipping structural
///    ones like `Binaries/Win64`, so the title folder is found even when the
///    exe is buried.
///
/// Each matched candidate is then checked against the exe's ancestor folders
/// using the manifest's `installDir` names to auto-detect the install root.
#[tauri::command]
pub async fn search_by_exe(
    config: State<'_, SharedConfig>,
    ludusavi: State<'_, LudusaviClient>,
    exe_path: String,
) -> AppResult<Vec<SearchCandidate>> {
    let ludusavi_exe = ludusavi_path_or_err(&config)?;
    let config_dir = crate::paths::ludusavi_config_dir();
    let path = Path::new(&exe_path);

    // Read PE metadata off the async executor — file IO can block.
    let exe_path_clone = exe_path.clone();
    let pe_name = tokio::task::spawn_blocking(move || {
        read_exe_product_name(Path::new(&exe_path_clone))
    })
    .await
    .ok()
    .flatten();

    // Build the query list: PE name first (highest confidence), then the
    // path-based heuristics, de-duped throughout.
    let mut queries: Vec<String> = Vec::new();
    if let Some(name) = pe_name {
        queries.push(name);
    }
    for q in infer_queries_from_path(path) {
        if !queries.contains(&q) {
            queries.push(q);
        }
    }
    if queries.is_empty() {
        return Ok(Vec::new());
    }

    let mut candidates = ludusavi
        .search_multi(&ludusavi_exe, &config_dir, &queries)
        .await?;
    // Detect each candidate's install root from the exe's ancestor folders so
    // the Add flow can default the install directory to it.
    for cand in &mut candidates {
        cand.install_root = resolve_install_root(path, &cand.manifest_install_dirs);
    }
    Ok(candidates)
}

fn ludusavi_path_or_err(_config: &State<'_, SharedConfig>) -> AppResult<PathBuf> {
    crate::paths::resolve_ludusavi_path().ok_or_else(|| {
        AppError::Other("Ludusavi sidecar not found — reinstall Spool.".to_string())
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_name_basic() {
        assert_eq!(infer_name_from_exe(Path::new("nightreign.exe")), "Nightreign");
        assert_eq!(
            infer_name_from_exe(Path::new("D:/Games/EldenRing/elden_ring.exe")),
            "Elden Ring"
        );
        assert_eq!(
            infer_name_from_exe(Path::new("hades-2.exe")),
            "Hades 2"
        );
        assert_eq!(infer_name_from_exe(Path::new("")), "");
        // All-caps exe names normalise to title case so fuzzy scores are good.
        assert_eq!(infer_name_from_exe(Path::new("PRAGMATA.exe")), "Pragmata");
        assert_eq!(infer_name_from_exe(Path::new("DOOM.exe")), "Doom");
    }

    #[test]
    fn infer_queries_includes_exe_and_ancestor_folders() {
        // Exe stem first, then the parent folder that carries the title.
        let q = infer_queries_from_path(Path::new("D:/Games/Hollow_Knight/start.exe"));
        assert_eq!(q, vec!["Start".to_string(), "Hollow Knight".to_string()]);
    }

    #[test]
    fn infer_queries_skips_structural_folders() {
        // Unreal layout: the title is two folders above the exe, under
        // Binaries/Win64 — both of which are skipped.
        let q = infer_queries_from_path(Path::new(
            "C:/Games/Elden Ring/Game/Binaries/Win64/eldenring-Win64-Shipping.exe",
        ));
        // "Game", "Binaries", "Win64" are all generic and skipped; "Elden
        // Ring" survives. The exe stem is included too.
        assert!(q.contains(&"Elden Ring".to_string()));
        assert!(!q.contains(&"Win64".to_string()));
        assert!(!q.contains(&"Binaries".to_string()));
        assert!(!q.contains(&"Game".to_string()));
    }

    #[test]
    fn infer_queries_dedups_when_exe_matches_folder() {
        // hollow_knight.exe inside a Hollow Knight folder → one query.
        let q = infer_queries_from_path(Path::new("D:/hollow_knight/hollow_knight.exe"));
        assert_eq!(q, vec!["Hollow Knight".to_string()]);
    }

    #[test]
    fn resolve_install_root_matches_outermost_ancestor() {
        // The exe is buried under Binaries/Win64; the install root is the
        // ancestor folder named like the manifest's installDir.
        let root = resolve_install_root(
            Path::new("C:/Games/EldenRing/Game/Binaries/Win64/start.exe"),
            &["ELDEN RING".to_string()],
        );
        assert_eq!(root.as_deref(), Some("C:/Games/EldenRing"));
    }

    #[test]
    fn resolve_install_root_none_without_match_or_dirs() {
        // No install-dir names → None (caller keeps the exe's parent).
        assert!(resolve_install_root(Path::new("C:/Games/Foo/foo.exe"), &[]).is_none());
        // Install-dir name that no ancestor matches → None.
        assert!(resolve_install_root(
            Path::new("C:/Games/Foo/foo.exe"),
            &["Totally Different".to_string()]
        )
        .is_none());
    }

    #[test]
    fn normalize_dir_ignores_case_and_punctuation() {
        assert_eq!(normalize_dir("Elden Ring"), "eldenring");
        assert_eq!(normalize_dir("ELDEN_RING"), "eldenring");
        assert_eq!(normalize_dir("EldenRing"), "eldenring");
    }

    #[test]
    fn sanitize_pe_string_strips_trademark_symbols() {
        assert_eq!(sanitize_pe_string("ELDEN RING™"), "ELDEN RING");
        assert_eq!(sanitize_pe_string("Half-Life® 2"), "Half-Life 2");
        assert_eq!(sanitize_pe_string("Doom © 2023"), "Doom 2023");
        // Multiple spaces collapsed.
        assert_eq!(sanitize_pe_string("  Hades  "), "Hades");
    }

    #[test]
    fn pe_name_normalised_to_title_case() {
        // Simulates pick_name_from_version_info's sanitize → title_case pipeline.
        assert_eq!(title_case(&sanitize_pe_string("PRAGMATA")), "Pragmata");
        assert_eq!(title_case(&sanitize_pe_string("ELDEN RING™")), "Elden Ring");
        assert_eq!(title_case(&sanitize_pe_string("DOOM")), "Doom");
        // Already title-cased values pass through unchanged.
        assert_eq!(title_case(&sanitize_pe_string("Hades")), "Hades");
    }

    #[test]
    fn is_junk_product_name_matches_case_insensitively() {
        assert!(is_junk_product_name("Unreal Engine"));
        assert!(is_junk_product_name("UNITY"));
        assert!(is_junk_product_name("unity technologies"));
        assert!(is_junk_product_name("Easy Anti-Cheat"));
        // Real game titles should not be filtered.
        assert!(!is_junk_product_name("ELDEN RING"));
        assert!(!is_junk_product_name("Hades"));
        assert!(!is_junk_product_name("Hollow Knight"));
    }

    #[test]
    fn reduce_backups_counts_and_picks_latest() {
        // Two backups for one game; latest `when` should win, count = 2.
        let json = r#"{
            "games": {
                "Hades": {
                    "backups": [
                        { "name": "ftp", "when": "2024-01-02T10:00:00Z", "locked": false },
                        { "name": ".", "when": "2024-03-15T18:30:00Z", "locked": false }
                    ]
                }
            }
        }"#;
        let out: BackupsOutput = serde_json::from_str(json).unwrap();
        let stats = reduce_backups(out);
        assert_eq!(stats.count, 2);
        assert_eq!(
            stats.last_backed_up_at.unwrap().to_rfc3339(),
            "2024-03-15T18:30:00+00:00"
        );
    }

    #[test]
    fn reduce_backups_empty_is_zero() {
        let out: BackupsOutput = serde_json::from_str(r#"{"games":{}}"#).unwrap();
        let stats = reduce_backups(out);
        assert_eq!(stats.count, 0);
        assert!(stats.last_backed_up_at.is_none());
    }

    #[test]
    fn revisions_are_newest_first_with_tip_flagged() {
        // Names survive parsing; the newest `when` is flagged is_current.
        let json = r#"{
            "games": {
                "Hades": {
                    "backups": [
                        { "name": "backup-1", "when": "2024-01-02T10:00:00Z", "locked": false },
                        { "name": ".", "when": "2024-03-15T18:30:00Z", "locked": false },
                        { "name": "backup-2", "when": "2024-02-01T09:00:00Z", "locked": false }
                    ]
                }
            }
        }"#;
        let out: BackupsOutput = serde_json::from_str(json).unwrap();
        let revs = revisions_from(out);
        assert_eq!(revs.len(), 3);
        // Newest first.
        assert_eq!(revs[0].name, ".");
        assert!(revs[0].is_current);
        assert_eq!(revs[1].name, "backup-2");
        assert!(!revs[1].is_current);
        assert_eq!(revs[2].name, "backup-1");
        assert!(!revs[2].is_current);
    }

    #[test]
    fn revisions_empty_is_empty() {
        let out: BackupsOutput = serde_json::from_str(r#"{"games":{}}"#).unwrap();
        assert!(revisions_from(out).is_empty());
    }

    #[test]
    fn cloud_op_maps_side_to_subcommand() {
        assert_eq!(CloudOp::from_side("local").unwrap(), CloudOp::Upload);
        assert_eq!(CloudOp::from_side("cloud").unwrap(), CloudOp::Download);
        assert_eq!(CloudOp::Upload.subcommand(), "upload");
        assert_eq!(CloudOp::Download.subcommand(), "download");
        assert!(CloudOp::from_side("nonsense").is_err());
        assert!(CloudOp::from_side("").is_err());
    }

    #[test]
    fn prettify_translates_known_tokens() {
        assert_eq!(
            prettify_save_template("<winAppData>/Hades/save"),
            "%APPDATA%/Hades/save"
        );
        assert_eq!(
            prettify_save_template("<winLocalAppData>/Foo/Saved"),
            "%LOCALAPPDATA%/Foo/Saved"
        );
        // Unknown tokens pass through.
        assert_eq!(
            prettify_save_template("<base>/saves"),
            "<base>/saves"
        );
    }

    #[test]
    fn enrich_falls_back_when_no_manifest_entry() {
        let c = enrich("Unknown Game".to_string(), 0.7, None);
        assert_eq!(c.name, "Unknown Game");
        assert_eq!(c.score, 0.7);
        assert!(c.save_path.is_none());
        assert!(c.save_paths.is_empty());
        assert!(c.steam_id.is_none());
    }

    #[test]
    fn enrich_picks_up_steam_and_install_dir() {
        let entry = ManifestEntry {
            steam: Some(StoreRef { id: 1145360 }),
            install_dir: {
                let mut m = HashMap::new();
                m.insert("Hades".to_string(), serde_json::Value::Null);
                m
            },
            files: {
                let mut m = HashMap::new();
                m.insert(
                    "<winAppData>/Hades/save".to_string(),
                    ManifestFileEntry { tags: vec!["save".to_string()] },
                );
                m
            },
            ..Default::default()
        };
        let c = enrich("Hades".to_string(), 0.95, Some(&entry));
        assert_eq!(c.steam_id, Some(1145360));
        assert_eq!(c.manifest_install_dir.as_deref(), Some("Hades"));
        assert_eq!(c.manifest_install_dirs, vec!["Hades".to_string()]);
        // install_root is only resolved by search_by_exe, which has the path.
        assert!(c.install_root.is_none());
        assert_eq!(c.save_path.as_deref(), Some("%APPDATA%/Hades/save"));
    }
}

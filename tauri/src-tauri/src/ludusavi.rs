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
    pub manifest_install_dir: Option<String>,
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
        let hits = run_find(ludusavi_exe, config_dir, query).await?;
        let manifest = self.manifest_or_load(ludusavi_exe, config_dir).await?;

        let mut candidates: Vec<SearchCandidate> = hits
            .games
            .into_iter()
            .map(|(name, hit)| {
                let entry = manifest.get(&name);
                enrich(name, hit.score, entry)
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
    /// inside the prefix's drive_c tree.
    pub async fn backup(
        &self,
        ludusavi_exe: &Path,
        config_dir: &Path,
        game_name: &str,
        wine_prefix: Option<&Path>,
    ) -> AppResult<ApiOutput> {
        let mut args = vec!["backup", "--api", "--cloud-sync", "--force", game_name];
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
    let manifest_install_dir = entry.install_dir.keys().next().cloned();
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
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let cleaned: String = stem
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    title_case(cleaned.trim())
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

/// Identifies a game from its exe path by inferring a search query from
/// the filename and running [`search_games`] under the hood. Used by the
/// Add Game flow's "identifying…" state.
#[tauri::command]
pub async fn search_by_exe(
    config: State<'_, SharedConfig>,
    ludusavi: State<'_, LudusaviClient>,
    exe_path: String,
) -> AppResult<Vec<SearchCandidate>> {
    let ludusavi_exe = ludusavi_path_or_err(&config)?;
    let config_dir = crate::paths::ludusavi_config_dir();
    let query = infer_name_from_exe(Path::new(&exe_path));
    if query.is_empty() {
        return Ok(Vec::new());
    }
    ludusavi.search(&ludusavi_exe, &config_dir, &query).await
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
        assert_eq!(c.save_path.as_deref(), Some("%APPDATA%/Hades/save"));
    }
}

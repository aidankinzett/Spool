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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;
use tokio::process::Command;
use tokio::sync::RwLock;

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
    pub cloud: Option<CloudInfo>,
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
pub struct CloudInfo {
    pub steam: bool,
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
    pub has_cloud_save: bool,
    /// The folder name ludusavi expects ("Hades" for `D:\Games\Hades\`).
    pub manifest_install_dir: Option<String>,
}

// ── Client ──────────────────────────────────────────────────────────────────

/// Ludusavi integration handle. Stateless apart from the lazy manifest
/// cache — methods take the ludusavi exe path as a parameter so changes to
/// `Config.ludusavi_path` are picked up immediately on the next call.
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
        query: &str,
    ) -> AppResult<Vec<SearchCandidate>> {
        let hits = run_find(ludusavi_exe, query).await?;
        let manifest = self.manifest_or_load(ludusavi_exe).await?;

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
        game_name: &str,
    ) -> AppResult<ApiOutput> {
        run_api(ludusavi_exe, &["restore", "--api", "--cloud-sync", "--force", game_name]).await
    }

    /// Runs `ludusavi backup --api --cloud-sync --force <name>` and parses
    /// the JSON output. Same forgiving behaviour as `restore`.
    pub async fn backup(
        &self,
        ludusavi_exe: &Path,
        game_name: &str,
    ) -> AppResult<ApiOutput> {
        run_api(ludusavi_exe, &["backup", "--api", "--cloud-sync", "--force", game_name]).await
    }

    async fn manifest_or_load(
        &self,
        ludusavi_exe: &Path,
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
        let manifest = load_manifest(ludusavi_exe).await?;
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

/// Builds a `Command` that won't flash a console window on Windows.
/// ludusavi.exe is a console subsystem binary, so without `CREATE_NO_WINDOW`
/// every invocation pops up a cmd window for a fraction of a second (or
/// longer, for restore/backup) on top of the Spool UI.
fn hidden_command(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW — winbase.h. Avoids a winapi dep for one constant.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

async fn run_find(ludusavi_exe: &Path, query: &str) -> AppResult<FindOutput> {
    let output = hidden_command(ludusavi_exe)
        .args(["find", "--api", "--fuzzy", "--multiple", query])
        .output()
        .await
        .map_err(|e| AppError::Other(format!("failed to spawn ludusavi: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        // ludusavi exits non-zero on no matches; surface an empty set.
        return Ok(FindOutput { games: HashMap::new() });
    }
    serde_json::from_str(&stdout)
        .map_err(|e| AppError::Other(format!("failed to parse ludusavi find output: {e}")))
}

/// Generic runner for ludusavi subcommands that emit the `--api` envelope
/// shared by `restore` and `backup`. Treats empty stdout as a successful
/// no-op (ludusavi sometimes exits non-zero with no output when the game
/// isn't in its manifest, which we surface as "no saves to handle").
async fn run_api(ludusavi_exe: &Path, args: &[&str]) -> AppResult<ApiOutput> {
    let output = hidden_command(ludusavi_exe)
        .args(args)
        .output()
        .await
        .map_err(|e| AppError::Other(format!("failed to spawn ludusavi: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(ApiOutput::default());
    }
    serde_json::from_str(&stdout)
        .map_err(|e| AppError::Other(format!("failed to parse ludusavi output: {e}")))
}

async fn load_manifest(ludusavi_exe: &Path) -> AppResult<HashMap<String, ManifestEntry>> {
    let output = hidden_command(ludusavi_exe)
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
    serde_json::from_str(&stdout)
        .map_err(|e| AppError::Other(format!("failed to parse ludusavi manifest: {e}")))
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
            has_cloud_save: false,
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
    let has_cloud_save = entry.cloud.as_ref().map(|c| c.steam).unwrap_or(false);

    SearchCandidate {
        name,
        score,
        save_path,
        save_paths,
        steam_id,
        gog_id,
        lutris_slug,
        has_cloud_save,
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
    ludusavi.search(&ludusavi_exe, query.trim()).await
}

/// Opens the configured ludusavi executable in GUI mode (`ludusavi gui`).
/// Used by toast CTAs that ask the user to resolve a state in ludusavi
/// (cloud-sync conflict, missing manifest entry, etc.).
#[tauri::command]
pub async fn open_ludusavi_gui(config: State<'_, SharedConfig>) -> AppResult<()> {
    let ludusavi_exe = ludusavi_path_or_err(&config)?;
    Command::new(&ludusavi_exe)
        .arg("gui")
        .spawn()
        .map_err(|e| AppError::Other(format!("failed to spawn ludusavi gui: {e}")))?;
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
    let query = infer_name_from_exe(Path::new(&exe_path));
    if query.is_empty() {
        return Ok(Vec::new());
    }
    ludusavi.search(&ludusavi_exe, &query).await
}

fn ludusavi_path_or_err(config: &State<'_, SharedConfig>) -> AppResult<PathBuf> {
    let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
    let path = cfg.data.ludusavi_path.clone();
    drop(cfg);
    if path.is_empty() {
        return Err(AppError::Other(
            "Ludusavi is not configured. Set its path in Settings.".to_string(),
        ));
    }
    let pb = PathBuf::from(&path);
    if !pb.is_file() {
        return Err(AppError::Other(format!(
            "Ludusavi not found at {}",
            path
        )));
    }
    Ok(pb)
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
        assert!(!c.has_cloud_save);
    }

    #[test]
    fn enrich_picks_up_steam_and_cloud() {
        let entry = ManifestEntry {
            steam: Some(StoreRef { id: 1145360 }),
            cloud: Some(CloudInfo { steam: true }),
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
        assert!(c.has_cloud_save);
        assert_eq!(c.manifest_install_dir.as_deref(), Some("Hades"));
        assert_eq!(c.save_path.as_deref(), Some("%APPDATA%/Hades/save"));
    }
}

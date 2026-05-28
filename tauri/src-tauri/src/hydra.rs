//! Hydra source feed aggregator.
//!
//! Hydra-format JSON catalogues (`{name, downloads: [{title, uris,
//! uploadDate, fileSize}, ...]}`) are community-maintained lists of
//! game downloads. The user configures one or more URLs in Settings;
//! Spool fetches them on demand, merges into a single list, and the
//! Browse Games window aggregates everything across feeds.
//!
//! Each entry carries one or more `uris` — magnet links, direct URLs,
//! whatever the source supports. The download orchestrator (built in
//! Phase 4) picks the best uri based on the user's configured
//! backends (TorBox for magnets, direct fetch for HTTP URLs).
//!
//! The C# implementation is the spec. Same JSON shape, same merging
//! rules, same error handling (per-feed failures log + continue).

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const FEED_TIMEOUT: Duration = Duration::from_secs(60);

/// One downloadable game in a Hydra source. Fields match the spec
/// from the Hydra ecosystem. `source_name` is filled in by us at
/// merge time so the UI can show "elamigos.json · 38.4 GB · 4 days
/// ago".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydraEntry {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub uris: Vec<String>,
    /// ISO 8601 string. Some feeds use yyyy-mm-dd, some include time.
    /// Source JSON uses camelCase `uploadDate`; we accept that on
    /// deserialize but serialize back as snake_case so the frontend's
    /// TypeScript types (and `entry.upload_date` access) line up.
    #[serde(default, alias = "uploadDate")]
    pub upload_date: String,
    /// Free-form human size string ("38.4 GB"). We pass through as-is.
    /// Same camelCase-in / snake_case-out story as `upload_date`.
    #[serde(default, alias = "fileSize")]
    pub file_size: String,
    /// Filled in by the merger — not present in the source JSON.
    #[serde(default)]
    pub source_name: String,
    /// Filled in by the merger — the URL the entry came from.
    #[serde(default)]
    pub source_url: String,
}

/// One whole source file. `name` is shown in the Browse sidebar;
/// `downloads` is what we flatten.
#[derive(Debug, Clone, Deserialize)]
pub struct HydraSource {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub downloads: Vec<HydraEntry>,
}

/// Per-feed fetch outcome reported to the UI so it can show
/// "elamigos.json — 1842 entries" or "repacks.json — failed: TLS
/// error".
#[derive(Debug, Clone, Serialize)]
pub struct FeedStatus {
    pub url: String,
    pub name: Option<String>,
    pub entry_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowseFetchResult {
    pub entries: Vec<HydraEntry>,
    pub feeds: Vec<FeedStatus>,
}

/// Fetches one Hydra source JSON. Best-effort: any failure resolves
/// to `Err` and the caller logs + skips that feed.
async fn fetch_one(app: &AppHandle, url: &str) -> AppResult<HydraSource> {
    let client = (*app.state::<reqwest::Client>()).clone();
    let resp = client
        .get(url)
        .timeout(FEED_TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("fetch {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "fetch {}: HTTP {}",
            url,
            resp.status()
        )));
    }
    resp.json::<HydraSource>()
        .await
        .map_err(|e| AppError::Other(format!("parse {url}: {e}")))
}

/// Reads the configured download_sources list. Returns an empty
/// vec when none are configured, never an error.
fn source_urls(app: &AppHandle) -> Vec<String> {
    let cfg = app.state::<SharedConfig>();
    let g = match cfg.lock() {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };
    g.data
        .download_sources
        .iter()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect()
}

/// Fetches every configured source in parallel, flattens results into
/// one entry list, and reports per-feed status. Empty configured
/// list resolves to an empty result (not an error) so the Browse UI
/// can render the "No sources configured" state cleanly.
pub async fn fetch_all(app: &AppHandle) -> BrowseFetchResult {
    let urls = source_urls(app);
    if urls.is_empty() {
        return BrowseFetchResult {
            entries: Vec::new(),
            feeds: Vec::new(),
        };
    }

    // Run all feed fetches concurrently — typical user has 2-3 feeds
    // and they're independent.
    let futures: Vec<_> = urls
        .iter()
        .map(|url| {
            let app = app.clone();
            let url = url.clone();
            async move {
                let result = fetch_one(&app, &url).await;
                (url, result)
            }
        })
        .collect();

    let results = futures_util::future::join_all(futures).await;

    let mut entries = Vec::new();
    let mut feeds = Vec::new();
    for (url, result) in results {
        match result {
            Ok(source) => {
                let count = source.downloads.len();
                feeds.push(FeedStatus {
                    url: url.clone(),
                    name: Some(source.name.clone()),
                    entry_count: count,
                    error: None,
                });
                let source_name = source.name;
                for mut entry in source.downloads {
                    entry.source_name = source_name.clone();
                    entry.source_url = url.clone();
                    entries.push(entry);
                }
            }
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "hydra: feed fetch failed");
                feeds.push(FeedStatus {
                    url,
                    name: None,
                    entry_count: 0,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    BrowseFetchResult { entries, feeds }
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn hydra_fetch_all(app: AppHandle) -> BrowseFetchResult {
    fetch_all(&app).await
}

/// Add a feed URL to config.download_sources. Returns the full
/// updated list. No-op if the URL is already present.
#[tauri::command]
pub fn hydra_add_source(app: AppHandle, url: String) -> AppResult<Vec<String>> {
    let trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::Other("URL cannot be empty".into()));
    }
    let cfg = app.state::<SharedConfig>();
    let mut g = cfg.lock().map_err(|_| AppError::LockPoisoned)?;
    if !g.data.download_sources.iter().any(|u| u == &trimmed) {
        g.data.download_sources.push(trimmed);
        g.save()?;
    }
    Ok(g.data.download_sources.clone())
}

/// Remove a feed URL from config.download_sources. Returns the
/// updated list.
#[tauri::command]
pub fn hydra_remove_source(app: AppHandle, url: String) -> AppResult<Vec<String>> {
    let cfg = app.state::<SharedConfig>();
    let mut g = cfg.lock().map_err(|_| AppError::LockPoisoned)?;
    let before = g.data.download_sources.len();
    g.data.download_sources.retain(|u| u != &url);
    if g.data.download_sources.len() != before {
        g.save()?;
    }
    Ok(g.data.download_sources.clone())
}

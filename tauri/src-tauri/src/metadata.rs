//! Steam Store metadata enrichment — description, developer, publisher,
//! genres, release date.
//!
//! Spool resolves a canonical Steam app id (`GameEntry.steam_id`) from
//! the ludusavi manifest at add-time. The public Steam Store
//! `appdetails` endpoint takes that id and returns rich store metadata
//! with **no API key** required:
//!
//! ```text
//! https://store.steampowered.com/api/appdetails?appids=<steam_id>&l=english
//! ```
//!
//! The fields map 1:1 onto the (previously unpopulated) metadata fields
//! already present on `GameEntry` and already rendered by
//! `GameDetail.svelte`. We only fill fields the user hasn't set — a
//! manual edit is never clobbered.
//!
//! The endpoint is rate-limited (~200 requests / 5 min), so the startup
//! backfill (`metadata_backfill.rs`) throttles between calls. The
//! add-game path fires a single request and is best-effort.

use crate::error::{AppError, AppResult};
use crate::library::{GameEntry, SharedLibrary};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const APPDETAILS: &str = "https://store.steampowered.com/api/appdetails";

/// Steam's public store search endpoint. Takes a free-text term and returns
/// matching store entries (apps, bundles, …) with their ids — no API key.
/// Used to resolve a Steam app id from a game name for an untracked add.
const STORE_SEARCH: &str = "https://store.steampowered.com/api/storesearch/";

/// Stateless Steam Store client. The HTTP client is held inline so the
/// TLS setup cost is paid once per process (mirrors `SteamGridDbClient`).
pub struct MetadataClient {
    http: reqwest::Client,
}

impl MetadataClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent("Spool/0.1 (https://github.com/aidankinzett/spool)")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Borrow the inner HTTP client (used by the backfill task, which
    /// drives `fetch_steam_metadata` directly without the library
    /// save/emit wrapper).
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }
}

impl Default for MetadataClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── DTOs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AppDetailsEntry {
    success: bool,
    data: Option<AppData>,
}

/// Top-level shape of the store-search response: `{ "total": N, "items": [...] }`.
#[derive(Debug, Deserialize)]
struct StoreSearchResponse {
    #[serde(default)]
    items: Vec<StoreSearchItem>,
}

#[derive(Debug, Deserialize)]
struct StoreSearchItem {
    id: u64,
    #[serde(rename = "type", default)]
    item_type: String,
}

#[derive(Debug, Deserialize)]
struct AppData {
    #[serde(default)]
    short_description: String,
    #[serde(default)]
    developers: Vec<String>,
    #[serde(default)]
    publishers: Vec<String>,
    #[serde(default)]
    genres: Vec<Genre>,
    #[serde(default)]
    release_date: Option<ReleaseDate>,
}

#[derive(Debug, Deserialize)]
struct Genre {
    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseDate {
    #[serde(default)]
    date: String,
}

/// Normalised metadata ready to fold into a `GameEntry`.
#[derive(Debug, Default, Clone)]
pub struct GameMetadata {
    pub description: String,
    pub developer: String,
    pub publisher: String,
    pub genres: Vec<String>,
    pub release_date: Option<chrono::DateTime<Utc>>,
}

impl GameMetadata {
    /// True when nothing useful came back — avoids a needless save/emit.
    fn is_empty(&self) -> bool {
        self.description.is_empty()
            && self.developer.is_empty()
            && self.publisher.is_empty()
            && self.genres.is_empty()
            && self.release_date.is_none()
    }
}

// ── Public entry point ──────────────────────────────────────────────────────

/// Fetches Steam Store metadata for `game_id`, fills any empty metadata
/// fields on the library entry, saves, and emits `library:changed`.
///
/// No-op (returns false) when the entry has no `steam_id`, when the
/// endpoint returns nothing, or when every target field is already
/// populated. Best-effort by design.
pub async fn fetch_and_save_metadata(app: &AppHandle, game_id: &str) -> AppResult<bool> {
    let library = app.state::<SharedLibrary>();
    let client = app.state::<MetadataClient>();

    // Snapshot the steam_id — bail before any network if there's nothing
    // to look up.
    let steam_id = library
        .find(game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?
        .steam_id;
    let Some(steam_id) = steam_id else {
        return Ok(false);
    };

    // Ok here means Steam responded — even Ok(None) (it has no metadata for this
    // appid). Either way we mark the entry fetched so the startup backfill stops
    // re-requesting it forever; only a network *error* (the `?`) leaves the
    // marker unset so the next boot retries.
    let meta = fetch_steam_metadata(&client.http, steam_id).await?;

    // Re-read the entry (it may have changed during the network call), apply only
    // the empty fields, set the fetched marker, and persist just those fields.
    let applied = match library.find(game_id).await? {
        Some(mut entry) => {
            let changed = meta
                .as_ref()
                .map(|m| apply_to_entry(&mut entry, m))
                .unwrap_or(false);
            entry.metadata_fetched = true;
            library
                .update_fields(game_id, &metadata_fields(&entry))
                .await?;
            changed
        }
        None => {
            tracing::warn!(game_id, "metadata fetched but library entry gone; skipping");
            false
        }
    };

    if applied {
        if let Err(e) = app.emit("library:changed", &game_id.to_string()) {
            tracing::warn!(error = %e, "failed to emit library:changed after metadata fetch");
        }
    }
    Ok(applied)
}

/// Tauri command: manually (re-)fetch Steam Store metadata for a game.
#[tauri::command]
pub async fn fetch_metadata(app: AppHandle, game_id: String) -> AppResult<bool> {
    fetch_and_save_metadata(&app, &game_id).await
}

// ── Internals ───────────────────────────────────────────────────────────────

/// Resolves a game name to a Steam app id via Steam's public store search
/// endpoint (no API key). Returns the top matching app's id, or `None` when
/// nothing matches or the name is blank.
///
/// Used to enrich an *untracked* add — one whose name ludusavi didn't resolve
/// to a Steam id — so it still gets Steam-CDN artwork and Steam-Store metadata.
/// Best-effort: a miss leaves `steam_id` unset and art falls back to a
/// SteamGridDB name lookup. The resolved id only drives art/metadata; it does
/// **not** enable save tracking (that's keyed off the manifest / save paths).
pub async fn resolve_steam_appid(http: &reqwest::Client, name: &str) -> AppResult<Option<u64>> {
    let term = name.trim();
    if term.is_empty() {
        return Ok(None);
    }
    let resp = http
        .get(STORE_SEARCH)
        .query(&[("term", term), ("cc", "us"), ("l", "english")])
        .send()
        .await
        .map_err(|e| AppError::Other(format!("steam store search failed: {e}")))?;
    if !resp.status().is_success() {
        tracing::warn!(name = term, status = %resp.status(), "steam store search non-2xx");
        return Ok(None);
    }
    let body: StoreSearchResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("steam store search json: {e}")))?;
    // The list can include bundles/subscriptions/DLC, whose ids are not Steam
    // app ids — persisting one as steam_id would yield no appdetails metadata
    // and 404 the Steam-CDN art. Only an `app` result carries a usable app id.
    let appid = body
        .items
        .iter()
        .find(|i| i.item_type == "app")
        .map(|i| i.id);
    Ok(appid)
}

/// Hits the Steam Store `appdetails` endpoint for `steam_id` and parses
/// the response into normalised metadata. Returns `None` when Steam
/// reports `success: false` (unknown/region-locked app) or the payload
/// has no usable fields.
pub async fn fetch_steam_metadata(
    http: &reqwest::Client,
    steam_id: u64,
) -> AppResult<Option<GameMetadata>> {
    let resp = http
        .get(APPDETAILS)
        .query(&[
            ("appids", steam_id.to_string()),
            ("l", "english".to_string()),
        ])
        .send()
        .await
        .map_err(|e| AppError::Other(format!("steam appdetails failed: {e}")))?;
    if !resp.status().is_success() {
        // 429 (rate limited) and friends land here — surface as a
        // soft None so callers log-and-continue rather than abort a
        // whole backfill run.
        tracing::warn!(steam_id, status = %resp.status(), "steam appdetails non-2xx");
        return Ok(None);
    }

    // Top-level shape is { "<appid>": { success, data } }.
    let body: HashMap<String, AppDetailsEntry> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("steam appdetails json: {e}")))?;
    let Some(entry) = body.get(&steam_id.to_string()) else {
        return Ok(None);
    };
    if !entry.success {
        return Ok(None);
    }
    let Some(data) = &entry.data else {
        return Ok(None);
    };

    let meta = GameMetadata {
        description: strip_html(&data.short_description),
        developer: data.developers.join(", "),
        publisher: data.publishers.join(", "),
        genres: data
            .genres
            .iter()
            .map(|g| g.description.trim().to_string())
            .filter(|g| !g.is_empty())
            .collect(),
        release_date: data
            .release_date
            .as_ref()
            .and_then(|r| parse_release_date(&r.date)),
    };

    if meta.is_empty() {
        Ok(None)
    } else {
        Ok(Some(meta))
    }
}

/// Folds `meta` into `entry`, only touching fields the user/library
/// hasn't already populated. Returns true if anything changed.
pub fn apply_to_entry(entry: &mut GameEntry, meta: &GameMetadata) -> bool {
    let mut changed = false;
    if entry.description.is_empty() && !meta.description.is_empty() {
        entry.description = meta.description.clone();
        changed = true;
    }
    if entry.developer.is_empty() && !meta.developer.is_empty() {
        entry.developer = meta.developer.clone();
        changed = true;
    }
    if entry.publisher.is_empty() && !meta.publisher.is_empty() {
        entry.publisher = meta.publisher.clone();
        changed = true;
    }
    if entry.genres.is_empty() && !meta.genres.is_empty() {
        entry.genres = meta.genres.clone();
        changed = true;
    }
    if entry.release_date.is_none() && meta.release_date.is_some() {
        entry.release_date = meta.release_date;
        changed = true;
    }
    changed
}

/// The JSON field tuples for the metadata [`apply_to_entry`] fills, used to
/// persist just those fields atomically via `Library::update_fields` (so a
/// concurrent playtime/backup write isn't clobbered).
pub fn metadata_fields(entry: &GameEntry) -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::json;
    vec![
        ("description", json!(entry.description)),
        ("developer", json!(entry.developer)),
        ("publisher", json!(entry.publisher)),
        ("genres", json!(entry.genres)),
        (
            "release_date",
            serde_json::to_value(entry.release_date).unwrap_or(serde_json::Value::Null),
        ),
        ("metadata_fetched", json!(entry.metadata_fetched)),
    ]
}

/// Strips a small set of HTML tags/entities Steam sometimes leaves in
/// `short_description`. Not a full HTML parser — just enough to keep the
/// detail screen clean.
fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

/// Steam's `release_date.date` is a localised free-text string. We try
/// the common English layouts; anything else leaves the field unset
/// rather than guessing wrong.
fn parse_release_date(s: &str) -> Option<chrono::DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // "25 Feb, 2022", "Feb 25, 2022", "25 February 2022", "February 2022".
    const FORMATS: &[&str] = &[
        "%d %b, %Y",
        "%b %d, %Y",
        "%d %B, %Y",
        "%B %d, %Y",
        "%d %b %Y",
        "%d %B %Y",
    ];
    for fmt in FORMATS {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return d.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc());
        }
    }
    // Month-year only, e.g. "Feb 2022" — pin to the first of the month.
    for fmt in &["%b %Y", "%B %Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(&format!("1 {s}"), &format!("%d {fmt}")) {
            return d.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_entities() {
        assert_eq!(strip_html("<b>Hello</b> &amp; bye"), "Hello & bye");
        assert_eq!(strip_html("plain text"), "plain text");
        assert_eq!(strip_html("a &lt;tag&gt; b"), "a <tag> b");
    }

    #[test]
    fn parses_common_release_dates() {
        assert!(parse_release_date("25 Feb, 2022").is_some());
        assert!(parse_release_date("Feb 25, 2022").is_some());
        assert!(parse_release_date("25 February 2022").is_some());
        assert!(parse_release_date("Feb 2022").is_some());
        assert!(parse_release_date("Coming soon").is_none());
        assert!(parse_release_date("").is_none());
    }

    #[test]
    fn apply_fills_only_empty_fields() {
        let mut entry = GameEntry {
            developer: "Existing Dev".to_string(),
            ..GameEntry::default()
        };
        let meta = GameMetadata {
            description: "A game".to_string(),
            developer: "Steam Dev".to_string(),
            publisher: "Steam Pub".to_string(),
            genres: vec!["Action".to_string()],
            release_date: None,
        };
        let changed = apply_to_entry(&mut entry, &meta);
        assert!(changed);
        // Pre-existing field is preserved.
        assert_eq!(entry.developer, "Existing Dev");
        // Empty fields get filled.
        assert_eq!(entry.description, "A game");
        assert_eq!(entry.publisher, "Steam Pub");
        assert_eq!(entry.genres, vec!["Action".to_string()]);
    }

    #[test]
    fn apply_noop_when_all_populated() {
        let mut entry = GameEntry {
            description: "d".to_string(),
            developer: "dev".to_string(),
            publisher: "pub".to_string(),
            genres: vec!["g".to_string()],
            release_date: Some(Utc::now()),
            ..GameEntry::default()
        };
        let meta = GameMetadata {
            description: "new".to_string(),
            developer: "new".to_string(),
            publisher: "new".to_string(),
            genres: vec!["new".to_string()],
            release_date: Some(Utc::now()),
        };
        assert!(!apply_to_entry(&mut entry, &meta));
    }
}

//! SteamGridDB integration — cover art lookup and download.
//!
//! Lookup strategy: prefer Steam ID (canonical, accurate) and fall back
//! to name autocomplete when no Steam ID is known. Downloads land in
//! `%LOCALAPPDATA%\Spool\covers\<safe_name>.<ext>` and the matching
//! `GameEntry.cover_image_path` is updated in place; the library file
//! is saved atomically and `library:changed` is emitted so any open
//! window can refresh.
//!
//! v1 only fetches the portrait cover (600×900). Hero / wide grid / logo
//! will land alongside the "Add to Steam" slice that needs them all.

use crate::config::SharedConfig;
use crate::error::{AppError, AppResult};
use crate::library::SharedLibrary;
use crate::paths;
use serde::Deserialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const BASE: &str = "https://www.steamgriddb.com/api/v2";

/// Stateless SteamGridDB client. The HTTP client is held inline so we
/// only pay the TLS setup cost once per process. The API key comes from
/// `Config` at call time (no caching) so changes in Settings take effect
/// immediately.
pub struct SteamGridDbClient {
    http: reqwest::Client,
}

impl SteamGridDbClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent("Spool/0.1 (https://github.com/aidankinzett/spool)")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl Default for SteamGridDbClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── DTOs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: serde::de::DeserializeOwned"))]
struct SgdbResponse<T> {
    success: bool,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct SgdbGame {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct Grid {
    url: String,
    #[serde(default)]
    mime: String,
}

// ── Public entry point used by add_game (spawned in the background) ─────────

/// Fetches the portrait cover for `game_entry_id`, saves it to disk,
/// updates the library entry's `cover_image_path`, and emits
/// `library:changed` so listeners refresh.
///
/// Returns the saved path (or None if cover lookup yielded nothing or
/// SteamGridDB is disabled in settings).
pub async fn fetch_and_save_cover(
    app: &AppHandle,
    game_entry_id: &str,
) -> AppResult<Option<String>> {
    let config = app.state::<SharedConfig>();
    let library = app.state::<SharedLibrary>();
    let client = app.state::<SteamGridDbClient>();

    // 1. Bail early if SteamGridDB isn't configured.
    let (api_key, enabled) = {
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        (
            cfg.data.steamgriddb_api_key.clone(),
            cfg.data.steamgriddb_enabled,
        )
    };
    if !enabled || api_key.is_empty() {
        return Ok(None);
    }

    // 2. Snapshot lookup info from the library entry — drop the lock fast.
    let (name, safe_name, steam_id) = {
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(game_entry_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_entry_id}")))?;
        (
            entry.game_name.clone(),
            entry.safe_name.clone(),
            entry.steam_id,
        )
    };

    // 3. Resolve to a SteamGridDB game id.
    let Some(sgdb_id) = resolve_game_id(&client.http, &api_key, steam_id, &name).await? else {
        return Ok(None);
    };

    // 4. Pick the first portrait grid result.
    let grids = fetch_portrait_grids(&client.http, &api_key, sgdb_id).await?;
    let Some(grid) = grids.into_iter().next() else {
        return Ok(None);
    };

    // 5. Download to disk.
    let ext = mime_to_ext(&grid.mime).unwrap_or_else(|| url_ext(&grid.url).unwrap_or("png"));
    let dir = paths::covers_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{safe_name}.{ext}"));
    let bytes = client
        .http
        .get(&grid.url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("sgdb image fetch failed: {e}")))?
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("sgdb image body failed: {e}")))?;
    std::fs::write(&path, &bytes)?;
    let path_str = path.to_string_lossy().to_string();

    // 6. Extract a vibrant accent colour from the cover. Best-effort —
    //    failure here just leaves the brand `spool` default on the UI.
    //    Image decode + histogram is sync CPU work; per
    //    `m07-concurrency` we punt to `spawn_blocking` so the async
    //    runtime stays responsive while it's chewing on a 1 MB image.
    let accent = {
        let p = path.clone();
        tokio::task::spawn_blocking(move || extract_vibrant_color(&p))
            .await
            .ok()
            .flatten()
    };

    // 7. Update library entry + persist.
    {
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_entry_id) {
            entry.cover_image_path = Some(path_str.clone());
            if let Some(a) = accent.clone() {
                entry.accent_color = Some(a);
            }
        }
        lib.save()?;
    }

    // 7. Notify the UI.
    if let Err(e) = app.emit("library:changed", &game_entry_id.to_string()) {
        tracing::warn!(error = %e, "failed to emit library:changed after cover download");
    }

    Ok(Some(path_str))
}

// ── Accent colour extraction ────────────────────────────────────────────────

/// Picks a "design-intent-y" accent colour from a cover image. The most
/// common colour in a typical cover is the background black; we want the
/// vibrant fill (the oxide-amber on Nightreign, the arcane purple on
/// Hades II). Algorithm:
///
///   1. Downsample to 32×32 for speed
///   2. Bucket pixels into ~32k colour bins (5 bits per channel)
///   3. Score each bin by saturation × proximity-to-mid-lightness ×
///      sqrt(frequency) — favours saturated mid-tones common enough to be
///      meaningful but not just background fill
///   4. Return the top bin as `#rrggbb`, or None if nothing passes
///      minimum saturation / lightness filters
///
/// Pure Rust, no extra crates beyond `image`.
pub fn extract_vibrant_color(path: &std::path::Path) -> Option<String> {
    use std::collections::HashMap;

    let img = image::open(path).ok()?.to_rgb8();
    let resized = image::imageops::resize(
        &img,
        32,
        32,
        image::imageops::FilterType::Lanczos3,
    );

    let mut buckets: HashMap<(u8, u8, u8), u32> = HashMap::new();
    for px in resized.pixels() {
        // 5 bits per channel ⇒ 32×32×32 = 32k buckets
        let key = (px[0] & 0xF8, px[1] & 0xF8, px[2] & 0xF8);
        *buckets.entry(key).or_insert(0) += 1;
    }

    let best = buckets
        .iter()
        .filter_map(|(&(r, g, b), &count)| {
            if count < 3 {
                return None;
            }
            let (_, s, l) = rgb_to_hsl(r, g, b);
            if s < 0.25 || l < 0.18 || l > 0.85 {
                return None;
            }
            // Peak score at lightness ~0.55 (slightly above middle —
            // covers tend to be moody, accent should pop a bit brighter).
            let lightness_weight = (1.0 - (l - 0.55).abs() * 2.0).max(0.1);
            let score = s * lightness_weight * (count as f32).sqrt();
            Some((r, g, b, score))
        })
        .max_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));

    best.map(|(r, g, b, _)| format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Standard RGB → HSL conversion. Returns (hue 0–360, sat 0–1, lum 0–1).
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

// ── Multi-art bundle for Add-to-Steam ───────────────────────────────────────

/// Fetches hero / wide-grid / logo from SteamGridDB and writes them
/// straight into Steam's grid dir with the filenames Steam expects:
///
///   `<app_id>_hero.<ext>`   — hero banner
///   `<app_id>.<ext>`        — wide grid (920×430)
///   `<app_id>_logo.<ext>`   — logo (transparent PNG)
///
/// Portrait cover is handled separately by `fetch_and_save_cover` (called
/// at add-time and reused by `place_grid_art` in `steam.rs`).
///
/// Best-effort throughout — silently skips any kind that doesn't resolve.
/// Returns the list of kinds that landed, suitable for surfacing in a
/// success toast.
pub async fn fetch_steam_grid_bundle(
    app: &AppHandle,
    steam_id: Option<u64>,
    game_name: &str,
    grid_dir: &std::path::Path,
    app_id: u32,
) -> AppResult<Vec<String>> {
    let config = app.state::<SharedConfig>();
    let client = app.state::<SteamGridDbClient>();

    let (api_key, enabled) = {
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        (cfg.data.steamgriddb_api_key.clone(), cfg.data.steamgriddb_enabled)
    };
    if !enabled || api_key.is_empty() {
        return Ok(Vec::new());
    }

    let Some(sgdb_id) = resolve_game_id(&client.http, &api_key, steam_id, game_name).await? else {
        return Ok(Vec::new());
    };

    std::fs::create_dir_all(grid_dir)?;
    let mut placed = Vec::new();

    let kinds: [(&str, &str); 3] = [
        ("hero", "_hero"),
        ("grid", ""),
        ("logo", "_logo"),
    ];
    for (kind, suffix) in kinds {
        if let Ok(Some(asset)) = fetch_first_art(&client.http, &api_key, sgdb_id, kind).await {
            let ext = mime_to_ext(&asset.mime).unwrap_or_else(|| url_ext(&asset.url).unwrap_or("png"));
            let dest = grid_dir.join(format!("{app_id}{suffix}.{ext}"));
            if let Ok(bytes) = download_bytes(&client.http, &asset.url).await {
                if std::fs::write(&dest, &bytes).is_ok() {
                    placed.push(kind.to_string());
                }
            }
        }
    }

    Ok(placed)
}

/// Fetches the first asset of `kind` (hero / grid / logo) for a game.
/// `kind` is one of "heroes", "grids", "logos" — actually we accept the
/// short forms "hero" / "grid" / "logo" and map.
async fn fetch_first_art(
    http: &reqwest::Client,
    api_key: &str,
    sgdb_id: u64,
    kind: &str,
) -> AppResult<Option<Asset>> {
    let endpoint = match kind {
        "hero" => format!("{BASE}/heroes/game/{sgdb_id}"),
        // Wide grid — explicit dimensions so we don't grab the portrait.
        "grid" => format!("{BASE}/grids/game/{sgdb_id}?dimensions=920x430"),
        "logo" => format!("{BASE}/logos/game/{sgdb_id}"),
        _ => return Ok(None),
    };
    let resp = http
        .get(&endpoint)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("sgdb {kind} fetch: {e}")))?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let body: SgdbResponse<Vec<Asset>> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("sgdb {kind} parse: {e}")))?;
    Ok(body.data.and_then(|v| v.into_iter().next()))
}

async fn download_bytes(http: &reqwest::Client, url: &str) -> AppResult<Vec<u8>> {
    let resp = http
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("download failed: {e}")))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("download body: {e}")))?;
    Ok(bytes.to_vec())
}

/// Common shape for hero / grid / logo entries — they all return at least
/// `url` and `mime`, sometimes more fields we don't need.
#[derive(Debug, Deserialize)]
struct Asset {
    url: String,
    #[serde(default)]
    mime: String,
}

// ── Tauri commands ──────────────────────────────────────────────────────────

/// Manual cover refresh for an existing game (re-runs lookup + download).
#[tauri::command]
pub async fn fetch_cover(app: AppHandle, game_id: String) -> AppResult<Option<String>> {
    fetch_and_save_cover(&app, &game_id).await
}

// ── Internals ───────────────────────────────────────────────────────────────

/// Returns a SteamGridDB game id. Tries Steam ID first; falls back to
/// name autocomplete. None when nothing matches at all.
async fn resolve_game_id(
    http: &reqwest::Client,
    api_key: &str,
    steam_id: Option<u64>,
    name: &str,
) -> AppResult<Option<u64>> {
    if let Some(sid) = steam_id {
        let url = format!("{BASE}/games/steam/{sid}");
        let resp = http
            .get(&url)
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|e| AppError::Other(format!("sgdb steam lookup failed: {e}")))?;
        if resp.status().is_success() {
            let body: SgdbResponse<SgdbGame> = resp
                .json()
                .await
                .map_err(|e| AppError::Other(format!("sgdb json (steam): {e}")))?;
            if body.success {
                if let Some(g) = body.data {
                    return Ok(Some(g.id));
                }
            }
        }
        // Steam ID lookup failed — fall through to name search.
    }

    // Autocomplete by name — Url::path_segments_mut() handles percent-encoding.
    let mut url = reqwest::Url::parse(&format!("{BASE}/search/autocomplete/"))
        .map_err(|e| AppError::Other(format!("sgdb url parse: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| AppError::Other("sgdb url cannot have path segments".into()))?
        .pop_if_empty()
        .push(name);

    let resp = http
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("sgdb search failed: {e}")))?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let body: SgdbResponse<Vec<SgdbGame>> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("sgdb json (search): {e}")))?;
    if !body.success {
        return Ok(None);
    }
    Ok(body.data.and_then(|v| v.into_iter().next()).map(|g| g.id))
}

async fn fetch_portrait_grids(
    http: &reqwest::Client,
    api_key: &str,
    sgdb_game_id: u64,
) -> AppResult<Vec<Grid>> {
    let url = format!("{BASE}/grids/game/{sgdb_game_id}?dimensions=600x900");
    let resp = http
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("sgdb grids failed: {e}")))?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let body: SgdbResponse<Vec<Grid>> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("sgdb json (grids): {e}")))?;
    Ok(body.data.unwrap_or_default())
}

pub(crate) fn mime_to_ext(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

fn url_ext(url: &str) -> Option<&'static str> {
    // Best-effort extension sniff from the URL path.
    let path = url.split('?').next().unwrap_or(url);
    let ext = path.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_translates_known_types() {
        assert_eq!(mime_to_ext("image/png"), Some("png"));
        assert_eq!(mime_to_ext("image/jpeg"), Some("jpg"));
        assert_eq!(mime_to_ext("image/webp"), Some("webp"));
        assert_eq!(mime_to_ext("image/avif"), None);
    }

    #[test]
    fn url_ext_sniffs_path() {
        assert_eq!(url_ext("https://cdn.example.com/a/b/cover.png"), Some("png"));
        assert_eq!(
            url_ext("https://cdn.example.com/cover.jpg?v=1"),
            Some("jpg")
        );
        assert_eq!(url_ext("https://cdn.example.com/cover"), None);
    }
}

//! SteamGridDB integration — cover art lookup and download.
//!
//! Lookup strategy: prefer Steam ID (canonical, accurate) and fall back
//! to name autocomplete when no Steam ID is known. Downloads land in
//! `%LOCALAPPDATA%\Spool\covers\<safe_name>.<ext>` and the matching
//! `GameEntry.cover_image_path` is updated in place; the library file
//! is saved atomically and `library:changed` is emitted so any open
//! window can refresh.
//!
//! Fetches both the portrait cover (600×900) and the hero banner (1920×620)
//! at add-time. Wide grid / logo are only fetched by the "Add to Steam" flow.

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
    let client = app.state::<SteamGridDbClient>();
    let Some((sgdb_id, safe_name, api_key)) = resolve_entry_sgdb_id(app, game_entry_id).await?
    else {
        return Ok(None);
    };

    let Some(path_str) = download_cover(&client.http, &api_key, sgdb_id, &safe_name).await? else {
        return Ok(None);
    };
    let accent = extract_accent_blocking(&path_str).await;

    apply_art(app, game_entry_id, Some(&path_str), None, accent.as_deref())?;
    Ok(Some(path_str))
}

/// Fetches both the cover and the hero for `game_entry_id` from a single
/// SteamGridDB game-id lookup, then writes them (plus the cover's accent
/// colour) in one library save + `library:changed` emit. Used by add_game so
/// the two assets don't each pay for their own id resolution. Each download is
/// best-effort — one failing doesn't discard the other.
pub async fn fetch_and_save_cover_and_hero(
    app: &AppHandle,
    game_entry_id: &str,
) -> AppResult<()> {
    let client = app.state::<SteamGridDbClient>();
    let Some((sgdb_id, safe_name, api_key)) = resolve_entry_sgdb_id(app, game_entry_id).await?
    else {
        return Ok(());
    };

    let cover = download_cover(&client.http, &api_key, sgdb_id, &safe_name)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(game_entry_id, error = %e, "cover download failed");
            None
        });
    let hero = download_hero(&client.http, &api_key, sgdb_id, &safe_name)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(game_entry_id, error = %e, "hero download failed");
            None
        });
    if cover.is_none() && hero.is_none() {
        return Ok(());
    }

    let accent = match &cover {
        Some(p) => extract_accent_blocking(p).await,
        None => None,
    };
    apply_art(
        app,
        game_entry_id,
        cover.as_deref(),
        hero.as_deref(),
        accent.as_deref(),
    )?;
    Ok(())
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
            if s < 0.25 || !(0.18..=0.85).contains(&l) {
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

/// Fetches the hero banner (1920×620 landscape) for `game_entry_id`, saves it
/// to `%LOCALAPPDATA%\Spool\heroes\<safe_name>.<ext>`, updates
/// `GameEntry.hero_image_path`, and emits `library:changed`. Mirrors
/// `fetch_and_save_cover` but uses the `/heroes` SteamGridDB endpoint.
///
/// Returns the saved path, or None if the game has no hero art or SteamGridDB
/// is disabled.
pub async fn fetch_and_save_hero(
    app: &AppHandle,
    game_entry_id: &str,
) -> AppResult<Option<String>> {
    let client = app.state::<SteamGridDbClient>();
    let Some((sgdb_id, safe_name, api_key)) = resolve_entry_sgdb_id(app, game_entry_id).await?
    else {
        return Ok(None);
    };
    let Some(path_str) = download_hero(&client.http, &api_key, sgdb_id, &safe_name).await? else {
        return Ok(None);
    };
    apply_art(app, game_entry_id, None, Some(&path_str), None)?;
    Ok(Some(path_str))
}

// ── Multi-art bundle for Add-to-Steam ───────────────────────────────────────

/// Fetches hero / wide-grid / logo / icon from SteamGridDB and writes them
/// straight into Steam's grid dir with the filenames Steam expects:
///
///   `<app_id>_hero.<ext>`   — hero banner
///   `<app_id>.<ext>`        — wide grid (920×430)
///   `<app_id>_logo.<ext>`   — logo (transparent PNG)
///   `<app_id>_icon.<ext>`   — icon
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

    let kinds: [(&str, &str); 4] = [
        ("hero", "_hero"),
        ("grid", ""),
        ("logo", "_logo"),
        ("icon", "_icon"),
    ];
    for (kind, suffix) in kinds {
        let asset = match fetch_first_art(&client.http, &api_key, sgdb_id, kind).await {
            Ok(Some(a)) => a,
            Ok(None) => {
                tracing::debug!(kind, "fetch_steam_grid_bundle: no {kind} art on SteamGridDB");
                continue;
            }
            Err(e) => {
                tracing::warn!(kind, %e, "fetch_steam_grid_bundle: {kind} API request failed");
                continue;
            }
        };
        let ext = mime_to_ext(&asset.mime).unwrap_or_else(|| url_ext(&asset.url).unwrap_or("png"));
        let dest = grid_dir.join(format!("{app_id}{suffix}.{ext}"));
        let bytes = match download_bytes(&client.http, &asset.url).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(kind, %e, "fetch_steam_grid_bundle: {kind} download failed");
                continue;
            }
        };
        match std::fs::write(&dest, &bytes) {
            Ok(()) => {
                tracing::debug!(kind, dest = %dest.display(), "fetch_steam_grid_bundle: {kind} placed");
                placed.push(kind.to_string());
            }
            Err(e) => {
                tracing::warn!(kind, dest = %dest.display(), %e, "fetch_steam_grid_bundle: {kind} write failed");
            }
        }
    }

    Ok(placed)
}

/// One downloaded art asset: raw bytes plus its mime (for the caller to set a
/// content-type / pick a Steam image type).
#[cfg(unix)]
pub struct ArtBytes {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// Tauri-free variant of the wide-art fetch used by the headless plugin
/// server: resolves the SteamGridDB id then downloads a single `kind`
/// ("hero" / "grid" / "logo" / "icon") and returns the bytes in memory rather
/// than writing them into Steam's grid dir. The desktop path
/// ([`fetch_steam_grid_bundle`]) still writes files; this one hands bytes to
/// the Decky UI which applies them live via `SetCustomArtworkForApp`.
///
/// `Ok(None)` means SteamGridDB is disabled/unconfigured, the game didn't
/// resolve, or that kind has no art — all non-errors the caller treats as
/// "skip this asset".
#[cfg(unix)]
pub async fn fetch_art_bytes(
    http: &reqwest::Client,
    api_key: &str,
    steam_id: Option<u64>,
    game_name: &str,
    kind: &str,
) -> AppResult<Option<ArtBytes>> {
    if api_key.is_empty() {
        return Ok(None);
    }
    let Some(sgdb_id) = resolve_game_id(http, api_key, steam_id, game_name).await? else {
        return Ok(None);
    };
    let Some(asset) = fetch_first_art(http, api_key, sgdb_id, kind).await? else {
        return Ok(None);
    };
    let bytes = download_bytes(http, &asset.url).await?;
    Ok(Some(ArtBytes { bytes, mime: asset.mime }))
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
        // Wide grid — filter to landscape dimensions only so we don't grab
        // the portrait (600x900). Both common sizes are accepted.
        "grid" => format!("{BASE}/grids/game/{sgdb_id}?dimensions=920x430,460x215"),
        "logo" => format!("{BASE}/logos/game/{sgdb_id}"),
        "icon" => format!("{BASE}/icons/game/{sgdb_id}"),
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
    let bytes = http
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("download failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("download non-2xx: {e}")))?
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("download body: {e}")))?;
    Ok(bytes.to_vec())
}

// ── Shared art-fetch helpers (cover + hero) ─────────────────────────────────

/// Shared preamble for the cover/hero fetchers: bail if SteamGridDB is
/// disabled, snapshot the entry's lookup fields, and resolve its SteamGridDB
/// game id. Returns `(sgdb_id, safe_name, api_key)`, or None to skip (disabled,
/// no API key, or nothing matched).
async fn resolve_entry_sgdb_id(
    app: &AppHandle,
    game_entry_id: &str,
) -> AppResult<Option<(u64, String, String)>> {
    let config = app.state::<SharedConfig>();
    let library = app.state::<SharedLibrary>();
    let client = app.state::<SteamGridDbClient>();

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

    match resolve_game_id(&client.http, &api_key, steam_id, &name).await? {
        Some(sgdb_id) => Ok(Some((sgdb_id, safe_name, api_key))),
        None => Ok(None),
    }
}

/// Downloads art at `url` into `<dir>/<safe_name>.<ext>` (ext inferred from the
/// mime, then the URL, defaulting to `png`) and returns the saved path.
async fn save_art_to(
    http: &reqwest::Client,
    dir: std::path::PathBuf,
    safe_name: &str,
    url: &str,
    mime: &str,
) -> AppResult<String> {
    let ext = mime_to_ext(mime).unwrap_or_else(|| url_ext(url).unwrap_or("png"));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{safe_name}.{ext}"));
    let bytes = download_bytes(http, url).await?;
    std::fs::write(&path, &bytes)?;
    Ok(path.to_string_lossy().to_string())
}

/// Downloads the first portrait cover for an already-resolved sgdb id into the
/// covers dir. None when the game has no portrait grid.
async fn download_cover(
    http: &reqwest::Client,
    api_key: &str,
    sgdb_id: u64,
    safe_name: &str,
) -> AppResult<Option<String>> {
    let grids = fetch_portrait_grids(http, api_key, sgdb_id).await?;
    let Some(grid) = grids.into_iter().next() else {
        return Ok(None);
    };
    Ok(Some(
        save_art_to(http, paths::covers_dir(), safe_name, &grid.url, &grid.mime).await?,
    ))
}

/// Downloads the first hero banner for an already-resolved sgdb id into the
/// heroes dir. None when the game has no hero art.
async fn download_hero(
    http: &reqwest::Client,
    api_key: &str,
    sgdb_id: u64,
    safe_name: &str,
) -> AppResult<Option<String>> {
    let Some(asset) = fetch_first_art(http, api_key, sgdb_id, "hero").await? else {
        return Ok(None);
    };
    Ok(Some(
        save_art_to(http, paths::heroes_dir(), safe_name, &asset.url, &asset.mime).await?,
    ))
}

/// Extracts the vibrant accent colour from a saved cover off the async executor
/// (image decode + histogram is sync CPU work). Best-effort — None on failure.
async fn extract_accent_blocking(path: &str) -> Option<String> {
    let p = std::path::PathBuf::from(path);
    tokio::task::spawn_blocking(move || extract_vibrant_color(&p))
        .await
        .ok()
        .flatten()
}

/// Applies any of cover path / hero path / accent to the entry, then persists
/// and emits `library:changed` once. No-ops the write if the entry vanished
/// mid-download.
fn apply_art(
    app: &AppHandle,
    game_entry_id: &str,
    cover: Option<&str>,
    hero: Option<&str>,
    accent: Option<&str>,
) -> AppResult<()> {
    let library = app.state::<SharedLibrary>();
    {
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_entry_id) else {
            tracing::warn!(game_entry_id, "art downloaded but library entry gone; skipping update");
            return Ok(());
        };
        if let Some(c) = cover {
            entry.cover_image_path = Some(c.to_string());
        }
        if let Some(h) = hero {
            entry.hero_image_path = Some(h.to_string());
        }
        if let Some(a) = accent {
            entry.accent_color = Some(a.to_string());
        }
        lib.save()?;
    }
    if let Err(e) = app.emit("library:changed", &game_entry_id.to_string()) {
        tracing::warn!(error = %e, "failed to emit library:changed after art download");
    }
    Ok(())
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

/// Manual hero refresh for an existing game (re-runs lookup + download).
#[tauri::command]
pub async fn fetch_hero(app: AppHandle, game_id: String) -> AppResult<Option<String>> {
    fetch_and_save_hero(&app, &game_id).await
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
    // Include all three common portrait capsule dimensions that SteamGridDB
    // hosts — some games only have 342x482 or 660x930 entries, not 600x900.
    let url = format!("{BASE}/grids/game/{sgdb_game_id}?dimensions=600x900,342x482,660x930");
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

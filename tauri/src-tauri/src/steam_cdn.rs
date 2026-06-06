//! Official Steam library artwork via the public Steam CDN.
//!
//! When ludusavi resolves a game's Steam app id (`GameEntry.steam_id`, taken
//! from the manifest's `steam:` block), Spool can pull canonical store/library
//! art straight from Steam's CDN. These are the same assets Steam's own library
//! renders, served from predictable per-appid URLs, so no SteamGridDB API key
//! or rate-limited lookup is involved. Only Steam games have them; everything
//! else falls back to SteamGridDB.
//!
//! `download_cover` / `download_hero` save into Spool's own `covers/`/`heroes/`
//! dirs (for the library tile + detail page); `fetch` returns raw bytes so the
//! Add-to-Steam bundle can write them into Steam's grid dir under Steam's own
//! filename convention.

use crate::error::{AppError, AppResult};
use crate::paths;
use reqwest::StatusCode;

const CDN: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";

/// A Steam asset Spool can pull from the CDN by appid alone.
#[derive(Clone, Copy)]
pub enum Asset {
    /// Portrait capsule (600×900), high-DPI — the library tile.
    Cover,
    /// Landscape hero banner (1920×620).
    Hero,
    /// Wide store capsule (460×215) — Steam's grid "landscape" tile.
    WideGrid,
    /// Transparent logo.
    Logo,
}

impl Asset {
    /// CDN filename for this asset under `apps/<appid>/`.
    fn filename(self) -> &'static str {
        match self {
            Asset::Cover => "library_600x900_2x.jpg",
            Asset::Hero => "library_hero.jpg",
            Asset::WideGrid => "header.jpg",
            Asset::Logo => "logo.png",
        }
    }

    /// File extension for callers writing the bytes to disk.
    pub fn ext(self) -> &'static str {
        match self {
            Asset::Logo => "png",
            _ => "jpg",
        }
    }
}

fn url(steam_id: u64, asset: Asset) -> String {
    format!("{CDN}/{steam_id}/{}", asset.filename())
}

/// GETs the official asset, returning the body on HTTP 200, `None` when Steam
/// has no such asset for this appid (404/403 — many appids lack a hero or logo,
/// and non-game appids lack everything), and `Err` only on a genuine transport
/// failure so callers can distinguish "no official art" from "network down".
pub async fn fetch(
    http: &reqwest::Client,
    steam_id: u64,
    asset: Asset,
) -> AppResult<Option<Vec<u8>>> {
    let resp = http
        .get(url(steam_id, asset))
        .send()
        .await
        .map_err(|e| AppError::Other(format!("steam cdn request: {e}")))?;
    match resp.status() {
        StatusCode::OK => {
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| AppError::Other(format!("steam cdn body: {e}")))?;
            Ok(Some(bytes.to_vec()))
        }
        StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => Ok(None),
        s => Err(AppError::Other(format!("steam cdn non-2xx: {s}"))),
    }
}

/// Fetches `asset` and writes it to `<dir>/<safe_name>.<ext>`, returning the
/// saved path. `None` when Steam has no such asset (caller falls back).
async fn fetch_to_dir(
    http: &reqwest::Client,
    steam_id: u64,
    asset: Asset,
    dir: std::path::PathBuf,
    safe_name: &str,
) -> AppResult<Option<String>> {
    let Some(bytes) = fetch(http, steam_id, asset).await? else {
        return Ok(None);
    };
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{safe_name}.{}", asset.ext()));
    std::fs::write(&path, &bytes)?;
    Ok(Some(path.to_string_lossy().to_string()))
}

/// Fetches the official portrait cover into the covers dir.
pub async fn download_cover(
    http: &reqwest::Client,
    steam_id: u64,
    safe_name: &str,
) -> AppResult<Option<String>> {
    fetch_to_dir(http, steam_id, Asset::Cover, paths::covers_dir(), safe_name).await
}

/// Fetches the official hero banner into the heroes dir.
pub async fn download_hero(
    http: &reqwest::Client,
    steam_id: u64,
    safe_name: &str,
) -> AppResult<Option<String>> {
    fetch_to_dir(http, steam_id, Asset::Hero, paths::heroes_dir(), safe_name).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_canonical_cdn_urls() {
        assert_eq!(
            url(1145360, Asset::Cover),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/1145360/library_600x900_2x.jpg"
        );
        assert_eq!(
            url(1145360, Asset::Hero),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/1145360/library_hero.jpg"
        );
        assert_eq!(
            url(1145360, Asset::WideGrid),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/1145360/header.jpg"
        );
        assert_eq!(
            url(1145360, Asset::Logo),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/1145360/logo.png"
        );
    }
}

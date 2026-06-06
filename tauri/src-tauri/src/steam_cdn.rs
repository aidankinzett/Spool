//! Official Steam library artwork via the public Steam CDN.
//!
//! When ludusavi resolves a game's Steam app id (`GameEntry.steam_id`, taken
//! from the manifest's `steam:` block), Spool can pull the canonical portrait
//! capsule and hero banner straight from Steam's CDN. These are the same
//! assets Steam's own library renders, served from predictable per-appid URLs,
//! so no SteamGridDB API key or rate-limited lookup is involved. Only Steam
//! games have them; everything else falls back to SteamGridDB.
//!
//! Downloaded files land in the same `covers/` and `heroes/` dirs the
//! SteamGridDB path uses, so the rest of the art pipeline (accent extraction,
//! `set_art`, `library:changed`) is unchanged.

use crate::error::{AppError, AppResult};
use crate::paths;
use reqwest::StatusCode;

const CDN: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";

/// Portrait capsule (600×900). The `_2x` variant is the high-DPI version Steam
/// uses for library tiles.
fn cover_url(steam_id: u64) -> String {
    format!("{CDN}/{steam_id}/library_600x900_2x.jpg")
}

/// Landscape hero banner shown behind the game's detail page.
fn hero_url(steam_id: u64) -> String {
    format!("{CDN}/{steam_id}/library_hero.jpg")
}

/// GETs `url`, returning the body on HTTP 200, `None` when the asset doesn't
/// exist (404/403 — many appids lack a hero, and non-game appids lack both),
/// and `Err` only on a genuine transport failure so the caller can distinguish
/// "no official art" from "network down".
async fn try_fetch(http: &reqwest::Client, url: &str) -> AppResult<Option<Vec<u8>>> {
    let resp = http
        .get(url)
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

/// Writes `bytes` to `<dir>/<safe_name>.jpg` and returns the saved path.
fn save_jpg(dir: std::path::PathBuf, safe_name: &str, bytes: &[u8]) -> AppResult<String> {
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{safe_name}.jpg"));
    std::fs::write(&path, bytes)?;
    Ok(path.to_string_lossy().to_string())
}

/// Fetches the official portrait cover into the covers dir. `None` when Steam
/// has no capsule for this appid (caller should fall back to SteamGridDB).
pub async fn download_cover(
    http: &reqwest::Client,
    steam_id: u64,
    safe_name: &str,
) -> AppResult<Option<String>> {
    let Some(bytes) = try_fetch(http, &cover_url(steam_id)).await? else {
        return Ok(None);
    };
    Ok(Some(save_jpg(paths::covers_dir(), safe_name, &bytes)?))
}

/// Fetches the official hero banner into the heroes dir. `None` when Steam has
/// no hero for this appid.
pub async fn download_hero(
    http: &reqwest::Client,
    steam_id: u64,
    safe_name: &str,
) -> AppResult<Option<String>> {
    let Some(bytes) = try_fetch(http, &hero_url(steam_id)).await? else {
        return Ok(None);
    };
    Ok(Some(save_jpg(paths::heroes_dir(), safe_name, &bytes)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_canonical_cdn_urls() {
        assert_eq!(
            cover_url(1145360),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/1145360/library_600x900_2x.jpg"
        );
        assert_eq!(
            hero_url(1145360),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/1145360/library_hero.jpg"
        );
    }
}

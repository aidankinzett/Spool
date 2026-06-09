use super::PluginState;
use axum::{
    extract::{Path as AxPath, State as AxState},
    http::StatusCode,
    response::Json,
};
use base64::Engine as _;
use serde_json::{json, Value};

/// Fields the UI needs to create a non-Steam shortcut (live, via
/// `SteamClient.Apps.AddShortcut`) and launch it. Mirrors what the desktop
/// `steam::add_to_steam` writes: the shortcut's exe is the stable Spool
/// binary (`spool_executable`, the `$APPIMAGE` path so it survives restarts)
/// and its launch options are `--run "<name>" "<game exe>"`, which the
/// Game-Mode attached `--run` flow consumes. The UI owns the actual shortcut
/// creation so it can use the live API (no Steam restart) and the appid Steam
/// returns.
pub(super) async fn get_steam_launch_info(
    AxState(state): AxState<PluginState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, StatusCode> {
    if !state.library_available {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let entry = state
        .library
        .find(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let spool_exe = crate::paths::spool_executable().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let start_dir = spool_exe
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    Ok(Json(json!({
        "appName": entry.game_name,
        "exe": spool_exe.to_string_lossy(),
        "startDir": start_dir,
        "launchOptions": crate::steam::build_launch_options(&entry.game_name, &entry.exe_path),
    })))
}

/// Returns `{ imageType: "png"|"jpeg", base64: "<data>" }` for the requested
/// art kind (`capsule`, `hero`, `logo`, `header`). Portrait and hero are served
/// from Spool's on-disk art (downloaded at add time, so they work with
/// SteamGridDB disabled); logo and the wide `header` capsule (and a hero with no
/// local copy) are resolved live through `steamgriddb::resolve_art_bytes` — the
/// official Steam CDN first, then SteamGridDB — so they still appear when no
/// SteamGridDB key is configured, matching the desktop "Add to Steam" bundle.
/// WebP images are transcoded to PNG because `SetCustomArtworkForApp` rejects
/// them. Returns 404 when neither source has the art.
pub(super) async fn get_steam_art(
    AxState(state): AxState<PluginState>,
    AxPath((id, kind)): AxPath<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    if !state.library_available {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let entry = state
        .library
        .find(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Portrait/capsule and hero are kept on disk (downloaded during "Add game"
    // / LAN install), so serve them straight from the file — no SteamGridDB
    // round-trip, and they work even when SteamGridDB is disabled.
    let local_path = match kind.as_str() {
        "capsule" => entry.cover_image_path.as_deref(),
        "hero" => entry.hero_image_path.as_deref(),
        _ => None,
    };
    if let Some(path_str) = local_path {
        let path = std::path::PathBuf::from(path_str);
        // Disk read + WebP→PNG decode/re-encode + base64 are blocking/CPU work;
        // keep them off the async runtime (the Decky UI fetches several art kinds
        // per game page). Returns None when the file is missing/unreadable so we
        // fall through to the live resolver below.
        let encoded = tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&path).ok()?;
            let mime = mime_from_path(&path);
            let (image_type, bytes) = transcode_webp_to_png(mime, bytes)?;
            Some((
                image_type,
                base64::engine::general_purpose::STANDARD.encode(&bytes),
            ))
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some((image_type, b64)) = encoded {
            return Ok(Json(json!({ "imageType": image_type, "base64": b64 })));
        }
    }

    // Anything without a local copy (logo, wide `header`, or a hero we never
    // downloaded) is resolved live: the official Steam CDN first (works with no
    // SteamGridDB key, which is the common case on a Deck), then SteamGridDB.
    // Map the plugin's Steam-assetType vocabulary onto the shared resolver's
    // kinds: the wide capsule is a landscape "grid".
    let sgdb_kind = match kind.as_str() {
        "header" => "grid",
        other => other,
    };
    let steam_id = entry.steam_id;

    // SteamGridDB is only the fallback, so resolve its game id lazily — a CDN
    // hit (the common case for a Steam game) then costs no SteamGridDB lookup.
    let config = crate::config::Config::load().unwrap_or_default();
    let api_key = config.data.steamgriddb_api_key;
    let sgdb = if config.data.steamgriddb_enabled && !api_key.is_empty() {
        crate::steamgriddb::SgdbFallback::Lazy {
            api_key: &api_key,
            name: &entry.game_name,
        }
    } else {
        crate::steamgriddb::SgdbFallback::None
    };

    let art = crate::steamgriddb::resolve_art_bytes(&state.http, steam_id, sgdb, sgdb_kind)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Transcode + base64 are CPU-bound — run them off the async runtime too.
    let transcode_res = tokio::task::spawn_blocking(move || {
        let mime = art.mime;
        let (image_type, bytes) = transcode_webp_to_png(&mime, art.bytes)?;
        Some((
            image_type,
            base64::engine::general_purpose::STANDARD.encode(&bytes),
        ))
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (image_type, b64) = transcode_res.ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!({ "imageType": image_type, "base64": b64 })))
}

/// Infers a MIME type string from a file extension (used for cached covers).
fn mime_from_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

/// Transcodes WebP bytes to PNG and returns `("png", transcoded_bytes)`.
/// For non-WebP MIME types, returns the normalised Steam imageType string
/// (`"jpeg"` or `"png"`) with the bytes unchanged.
fn transcode_webp_to_png(mime: &str, bytes: Vec<u8>) -> Option<(&'static str, Vec<u8>)> {
    if mime.contains("webp") {
        match image::load_from_memory(&bytes) {
            Ok(img) => {
                let mut out = std::io::Cursor::new(Vec::new());
                match img.write_to(&mut out, image::ImageFormat::Png) {
                    Ok(()) => Some(("png", out.into_inner())),
                    Err(e) => {
                        tracing::warn!(error = %e, "steam-art: png encoding failed");
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "steam-art: webp→png transcode failed");
                None
            }
        }
    } else {
        let image_type = if mime.contains("jpeg") || mime.contains("jpg") {
            "jpeg"
        } else {
            "png"
        };
        Some((image_type, bytes))
    }
}

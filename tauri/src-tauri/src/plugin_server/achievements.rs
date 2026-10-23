use super::PluginState;
use axum::{
    extract::{Path as AxPath, Query, State as AxState},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub(super) struct RecentQuery {
    /// Unix ms; only unlocks recorded after this are returned.
    #[serde(default)]
    since: i64,
}

/// Achievements unlocked during a play session and recorded after `since`.
/// The Decky plugin polls this while a game runs and shows each as a Steam
/// toast. Empty when achievement notifications are off in Spool's settings.
/// Icons are only passed through when they're http(s) URLs, since the Steam
/// UI can't load Spool's local icon cache.
pub(super) async fn get_recent(
    AxState(state): AxState<PluginState>,
    Query(q): Query<RecentQuery>,
) -> Json<Value> {
    let enabled = crate::config::Config::load()
        .map(|c| c.data.achievement_notifications)
        .unwrap_or(true);
    if !enabled {
        return Json(json!([]));
    }
    let rows = state
        .library
        .live_achievement_unlocks_since(q.since)
        .await
        .unwrap_or_default();
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "game_name": r.game_name,
                "api_name": r.api_name,
                "display_name": r.display_name.as_deref().unwrap_or(&r.api_name),
                "icon": r.icon.as_deref().filter(|i| i.starts_with("http")),
                "detected_at": r.detected_at,
            })
        })
        .collect();
    Json(json!(out))
}

/// A game's achievement list, as the desktop detail pane's `get_game_achievements`
/// returns it: the game's stats files are scanned first, so unlocks earned
/// outside Spool show up, and any new ones are published to other devices.
/// Other devices' unlocks come in through `POST /fold`, which the plugin calls
/// when the game page opens.
///
/// Icons cached from a game's `steam_settings` are local paths, which the Steam
/// UI can't load; they're rewritten to `/achievement-icons/…` on this server.
/// Steam Web API icons are already http(s) URLs and pass through.
pub(super) async fn get_game_achievements(
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
    let (mut achievements, imported) =
        crate::achievements::scan_game_achievements(&state.http, &state.library, entry)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if imported {
        let library = state.library.clone();
        tokio::spawn(async move {
            if let Ok(cfg) = crate::config::Config::load() {
                crate::rclone::sync_achievements_from_config(&cfg.data, &library).await;
            }
        });
    }
    for item in &mut achievements.items {
        item.icon = item.icon.take().and_then(|i| icon_url(&i));
        item.icon_gray = item.icon_gray.take().and_then(|i| icon_url(&i));
    }
    serde_json::to_value(achievements)
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// An icon the Steam UI can load: http(s) URLs as is, and files in the schema
/// cache as a path under `/achievement-icons` (relative to the server base).
/// `None` for anything else.
fn icon_url(icon: &str) -> Option<String> {
    if icon.starts_with("http://") || icon.starts_with("https://") {
        return Some(icon.to_string());
    }
    let rel = std::path::Path::new(icon)
        .strip_prefix(crate::achievements::schema::cache_dir())
        .ok()?;
    let parts: Vec<_> = rel
        .components()
        .map(|c| match c {
            std::path::Component::Normal(p) => p.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    Some(format!("/achievement-icons/{}", parts.join("/")))
}

/// A game's play sessions on every device (oldest first), and this device's
/// id so the UI can tell its own sessions apart. Mirrors the desktop
/// `list_play_sessions` command.
pub(super) async fn get_game_sessions(
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
    let sessions = state
        .library
        .list_sessions(Some(&entry.game_name))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let device_id = crate::config::Config::load()
        .map(|c| c.data.device_id)
        .unwrap_or_default();
    Ok(Json(
        json!({ "device_id": device_id, "sessions": sessions }),
    ))
}

#[cfg(test)]
mod tests {
    use super::icon_url;
    use crate::achievements::schema::cache_dir;

    #[test]
    fn icon_url_passes_http_through() {
        assert_eq!(
            icon_url("https://cdn.example/a.jpg").as_deref(),
            Some("https://cdn.example/a.jpg")
        );
    }

    #[test]
    fn icon_url_maps_cached_files_under_the_route() {
        let p = cache_dir().join("480-icons").join("img_a.png");
        assert_eq!(
            icon_url(&p.to_string_lossy()).as_deref(),
            Some("/achievement-icons/480-icons/img_a.png")
        );
    }

    #[test]
    fn icon_url_rejects_paths_outside_the_cache() {
        assert_eq!(icon_url("/etc/passwd"), None);
        let p = cache_dir().join("..").join("config.json");
        assert_eq!(icon_url(&p.to_string_lossy()), None);
    }
}

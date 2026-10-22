use super::PluginState;
use axum::{
    extract::{Query, State as AxState},
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

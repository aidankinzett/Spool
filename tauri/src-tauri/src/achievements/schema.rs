//! Achievement schema (display names, descriptions, icons) for a Steam app id.
//!
//! Emulator state files carry only API names, so the schema supplies
//! everything shown to the user. Two sources, in order:
//!
//! 1. **Steam Web API** `ISteamUserStats/GetSchemaForGame`, when the user has
//!    set a Steam Web API key (Settings → Achievements). Icons are Steam CDN
//!    URLs. Global unlock percentages come from the keyless
//!    `GetGlobalAchievementPercentagesForApp`.
//! 2. **The game's own `steam_settings/achievements.json`**, which Goldberg /
//!    GSE setups ship with the emulator. Its icons are files beside it; they
//!    are copied into app data because the webview's asset scope only covers
//!    Spool's own folder.
//!
//! The result is cached per app id under `achievement-schemas/` and refreshed
//! after [`MAX_AGE_SECS`]. In offline mode only the cache and the local file
//! are used.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SCHEMA_URL: &str = "https://api.steampowered.com/ISteamUserStats/GetSchemaForGame/v2/";
const PERCENT_URL: &str =
    "https://api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/";

/// A cached schema is refetched after this long (achievements are sometimes
/// added in updates; percentages drift).
const MAX_AGE_SECS: i64 = 14 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaEntry {
    pub api_name: String,
    pub display_name: String,
    pub description: String,
    pub hidden: bool,
    /// Steam CDN URL, or an absolute path under app data for a local schema.
    pub icon: Option<String>,
    pub icon_gray: Option<String>,
    /// Share of Steam players who have unlocked it, 0–100.
    pub global_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schema {
    pub appid: u64,
    /// `"steam"` or `"local"`.
    pub source: String,
    pub fetched_at: i64,
    pub achievements: Vec<SchemaEntry>,
}

impl Schema {
    /// Entry for an API name, matched case-insensitively (some emulator files
    /// don't keep the schema's casing).
    pub fn find(&self, api_name: &str) -> Option<&SchemaEntry> {
        self.achievements
            .iter()
            .find(|e| e.api_name == api_name)
            .or_else(|| {
                self.achievements
                    .iter()
                    .find(|e| e.api_name.eq_ignore_ascii_case(api_name))
            })
    }

    /// Entry whose API name has this CRC32 (SmartSteamEmu).
    pub fn find_crc(&self, crc: u32) -> Option<&SchemaEntry> {
        self.achievements
            .iter()
            .find(|e| crc32fast::hash(e.api_name.as_bytes()) == crc)
    }
}

fn cache_dir() -> PathBuf {
    crate::paths::app_data_dir().join("achievement-schemas")
}

fn cache_file(appid: u64) -> PathBuf {
    cache_dir().join(format!("{appid}.json"))
}

pub fn load_cached(appid: u64) -> Option<Schema> {
    let text = std::fs::read_to_string(cache_file(appid)).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cached(schema: &Schema) {
    let path = cache_file(schema.appid);
    let _ = std::fs::create_dir_all(cache_dir());
    match serde_json::to_vec_pretty(schema) {
        Ok(bytes) => {
            if let Err(e) = crate::paths::write_atomic(&path, &bytes, false) {
                tracing::warn!(error = %e, appid = schema.appid, "achievements: couldn't cache schema");
            }
        }
        Err(e) => tracing::warn!(error = %e, "achievements: couldn't serialise schema"),
    }
}

/// The schema for `appid`: a fresh cache entry if there is one, else a fetch
/// from Steam (needs `api_key`) or the game's `steam_settings/achievements.json`
/// (`local_file`), else a stale cache entry. `None` when nothing is available.
pub async fn ensure(
    http: &reqwest::Client,
    appid: u64,
    local_file: Option<&Path>,
    api_key: &str,
) -> Option<Schema> {
    let cached = load_cached(appid);
    let now = chrono::Utc::now().timestamp();
    let offline = crate::config::offline_mode_enabled();
    let has_key = !api_key.trim().is_empty();
    if let Some(c) = &cached {
        let fresh = now - c.fetched_at < MAX_AGE_SECS;
        // A local schema is upgraded to Steam's as soon as a key is set.
        let upgradable = c.source == "local" && has_key;
        if offline || (fresh && !upgradable) {
            return cached;
        }
    }

    if has_key && !offline {
        match fetch_steam(http, appid, api_key.trim()).await {
            Ok(schema) => {
                save_cached(&schema);
                return Some(schema);
            }
            Err(e) => tracing::warn!(error = %e, appid, "achievements: Steam schema fetch failed"),
        }
    }

    if let Some(file) = local_file {
        let file = file.to_path_buf();
        let local = tokio::task::spawn_blocking(move || read_local(appid, &file))
            .await
            .ok()
            .flatten();
        if let Some(mut schema) = local {
            if !offline {
                if let Ok(pct) = fetch_percentages(http, appid).await {
                    apply_percentages(&mut schema, &pct);
                }
            }
            save_cached(&schema);
            return Some(schema);
        }
    }
    cached
}

fn net(e: reqwest::Error) -> AppError {
    AppError::Other(format!("network error: {e}"))
}

/// Steam's per-language string, or a plain string.
fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(m)) => m
            .get("english")
            .or_else(|| m.values().next())
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64() == Some(1),
        Some(Value::String(s)) => s.trim() == "1" || s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

async fn fetch_steam(http: &reqwest::Client, appid: u64, key: &str) -> AppResult<Schema> {
    let res = http
        .get(SCHEMA_URL)
        .query(&[
            ("key", key),
            ("appid", &appid.to_string()),
            ("l", "english"),
        ])
        .send()
        .await
        .map_err(net)?;
    if !res.status().is_success() {
        return Err(AppError::Other(format!(
            "GetSchemaForGame returned {}",
            res.status()
        )));
    }
    let body: Value = res.json().await.map_err(net)?;
    let list = body
        .pointer("/game/availableGameStats/achievements")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut schema = Schema {
        appid,
        source: "steam".into(),
        fetched_at: chrono::Utc::now().timestamp(),
        achievements: list.iter().filter_map(steam_entry).collect(),
    };
    if !schema.achievements.is_empty() {
        if let Ok(pct) = fetch_percentages(http, appid).await {
            apply_percentages(&mut schema, &pct);
        }
    }
    Ok(schema)
}

fn steam_entry(v: &Value) -> Option<SchemaEntry> {
    let url = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .filter(|s| s.starts_with("http"))
            .map(str::to_string)
    };
    Some(SchemaEntry {
        api_name: v.get("name")?.as_str()?.to_string(),
        display_name: text_of(v.get("displayName")),
        description: text_of(v.get("description")),
        hidden: truthy(v.get("hidden")),
        icon: url("icon"),
        icon_gray: url("icongray"),
        global_percent: None,
    })
}

async fn fetch_percentages(http: &reqwest::Client, appid: u64) -> AppResult<HashMap<String, f64>> {
    let body: Value = http
        .get(PERCENT_URL)
        .query(&[("gameid", appid.to_string())])
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(net)?
        .json()
        .await
        .map_err(net)?;
    let list = body
        .pointer("/achievementpercentages/achievements")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(list
        .iter()
        .filter_map(|a| {
            let name = a.get("name")?.as_str()?.to_string();
            let pct = match a.get("percent")? {
                Value::Number(n) => n.as_f64()?,
                Value::String(s) => s.parse().ok()?,
                _ => return None,
            };
            Some((name, pct))
        })
        .collect())
}

fn apply_percentages(schema: &mut Schema, pct: &HashMap<String, f64>) {
    for e in &mut schema.achievements {
        e.global_percent = pct.get(&e.api_name).copied();
    }
}

/// Parses a `steam_settings/achievements.json` and copies its icons into the
/// cache.
fn read_local(appid: u64, file: &Path) -> Option<Schema> {
    let settings_dir = file.parent()?.to_path_buf();
    let list: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let icon_dir = cache_dir().join(format!("{appid}-icons"));
    let copy_icon = |rel: Option<&Value>| -> Option<String> {
        let rel = rel?.as_str()?.trim();
        if rel.is_empty() {
            return None;
        }
        if rel.starts_with("http") {
            return Some(rel.to_string());
        }
        let src = settings_dir.join(rel);
        let name = src.file_name()?;
        let dst = icon_dir.join(name);
        if !dst.is_file() {
            std::fs::create_dir_all(&icon_dir).ok()?;
            std::fs::copy(&src, &dst).ok()?;
        }
        Some(dst.to_string_lossy().into_owned())
    };
    let achievements = list
        .iter()
        .filter_map(|v| {
            Some(SchemaEntry {
                api_name: v.get("name")?.as_str()?.to_string(),
                display_name: text_of(v.get("displayName")),
                description: text_of(v.get("description")),
                hidden: truthy(v.get("hidden")),
                icon: copy_icon(v.get("icon")),
                icon_gray: copy_icon(v.get("icongray").or_else(|| v.get("icon_gray"))),
                global_percent: None,
            })
        })
        .collect();
    Some(Schema {
        appid,
        source: "local".into(),
        fetched_at: chrono::Utc::now().timestamp(),
        achievements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn steam_entry_maps_fields() {
        let e = steam_entry(&json!({
            "name": "ACH_WIN", "defaultvalue": 0, "displayName": "Winner",
            "hidden": 1, "description": "Win.",
            "icon": "https://cdn/x.jpg", "icongray": "https://cdn/y.jpg"
        }))
        .unwrap();
        assert_eq!(e.api_name, "ACH_WIN");
        assert_eq!(e.display_name, "Winner");
        assert!(e.hidden);
        assert_eq!(e.icon.as_deref(), Some("https://cdn/x.jpg"));
    }

    #[test]
    fn local_schema_handles_language_maps() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("steam_settings");
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::write(
            settings.join("achievements.json"),
            json!([{ "name": "A", "displayName": { "english": "Alpha", "german": "Alfa" },
                     "description": "d", "hidden": "0" }])
            .to_string(),
        )
        .unwrap();
        let s = read_local(1, &settings.join("achievements.json")).unwrap();
        assert_eq!(s.source, "local");
        assert_eq!(s.achievements[0].display_name, "Alpha");
        assert!(!s.achievements[0].hidden);
    }

    #[test]
    fn find_matches_case_and_crc() {
        let s = Schema {
            appid: 1,
            source: "steam".into(),
            fetched_at: 0,
            achievements: vec![SchemaEntry {
                api_name: "Ach_One".into(),
                display_name: "One".into(),
                description: String::new(),
                hidden: false,
                icon: None,
                icon_gray: None,
                global_percent: None,
            }],
        };
        assert!(s.find("ACH_ONE").is_some());
        assert!(s.find_crc(crc32fast::hash(b"Ach_One")).is_some());
        assert!(s.find_crc(0).is_none());
    }
}

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

/// How long to wait after a failed Steam schema fetch before trying again.
const STEAM_RETRY_SECS: u64 = 6 * 60 * 60;

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

pub(crate) fn cache_dir() -> PathBuf {
    crate::paths::app_data_dir().join("achievement-schemas")
}

fn cache_file(appid: u64) -> PathBuf {
    cache_dir().join(format!("{appid}.json"))
}

/// Marker whose mtime records the last failed Steam fetch for `appid` (bad or
/// expired key, network down, or an app Steam has no achievements for).
fn failed_marker(appid: u64) -> PathBuf {
    cache_dir().join(format!("{appid}.steam-failed"))
}

/// True for [`STEAM_RETRY_SECS`] after a failed Steam fetch, so a bad key
/// doesn't send a request on every detail-pane load.
fn steam_backed_off(appid: u64) -> bool {
    std::fs::metadata(failed_marker(appid))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age.as_secs() < STEAM_RETRY_SECS)
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
    let steam_due = !api_key.trim().is_empty() && !offline && !steam_backed_off(appid);
    if let Some(c) = &cached {
        let fresh = now - c.fetched_at < MAX_AGE_SECS;
        // A local schema is upgraded to Steam's as soon as a key is set.
        let upgradable = c.source == "local" && steam_due;
        if offline || (fresh && !upgradable) {
            return cached;
        }
    }

    if steam_due {
        match fetch_steam(http, appid, api_key.trim()).await {
            Ok(schema) => {
                let _ = std::fs::remove_file(failed_marker(appid));
                save_cached(&schema);
                return Some(schema);
            }
            Err(e) => {
                tracing::warn!(error = %e, appid, "achievements: Steam schema fetch failed");
                let _ = std::fs::create_dir_all(cache_dir());
                let _ = std::fs::write(failed_marker(appid), b"");
            }
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

/// The request URL is dropped from the message: it carries the API key as a
/// query parameter, and this text ends up in `debug.log`.
fn net(e: reqwest::Error) -> AppError {
    AppError::Other(format!("network error: {}", e.without_url()))
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
    // Steam answers `{"game":{}}` for apps it has no stats for. Treated as a
    // failure so the game's own steam_settings schema is used instead.
    if schema.achievements.is_empty() {
        return Err(AppError::Other(format!(
            "Steam lists no achievements for app {appid}"
        )));
    }
    if let Ok(pct) = fetch_percentages(http, appid).await {
        apply_percentages(&mut schema, &pct);
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
        // Named after the whole relative path, so same-named files in
        // different subfolders (e.g. colour and gray icons) don't collide.
        let dst = icon_dir.join(rel.replace(['/', '\\'], "_"));
        let src_meta = std::fs::metadata(&src).ok()?;
        let up_to_date = std::fs::metadata(&dst).is_ok_and(|d| d.len() == src_meta.len());
        if !up_to_date {
            std::fs::create_dir_all(&icon_dir).ok()?;
            // Copied beside the target then renamed, so another Spool process
            // reading the icon never sees a partial file.
            let tmp = dst.with_extension(format!("tmp-{}", std::process::id()));
            std::fs::copy(&src, &tmp).ok()?;
            std::fs::rename(&tmp, &dst).ok()?;
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

    #[test]
    fn text_of_prefers_english_then_any_language_then_empty() {
        assert_eq!(text_of(Some(&json!("plain"))), "plain");
        assert_eq!(
            text_of(Some(&json!({ "german": "Hallo", "english": "Hi" }))),
            "Hi"
        );
        assert_eq!(text_of(Some(&json!({ "german": "Hallo" }))), "Hallo");
        assert_eq!(text_of(Some(&json!({ "english": 5 }))), "");
        assert_eq!(text_of(Some(&json!(7))), "");
        assert_eq!(text_of(None), "");
    }

    #[test]
    fn truthy_accepts_bool_one_and_true_strings_only() {
        assert!(truthy(Some(&json!(true))));
        assert!(truthy(Some(&json!(1))));
        assert!(truthy(Some(&json!("1"))));
        assert!(truthy(Some(&json!(" 1 "))));
        assert!(truthy(Some(&json!("TRUE"))));
        assert!(!truthy(Some(&json!(2))));
        assert!(!truthy(Some(&json!("yes"))));
        assert!(!truthy(Some(&json!(null))));
        assert!(!truthy(None));
    }

    #[test]
    fn steam_entry_needs_a_name_and_drops_non_http_icons() {
        assert!(steam_entry(&json!({ "displayName": "No name" })).is_none());
        assert!(steam_entry(&json!({ "name": 5 })).is_none());
        let e =
            steam_entry(&json!({ "name": "A", "icon": "icons/a.png", "icongray": "" })).unwrap();
        assert_eq!(e.icon, None);
        assert_eq!(e.icon_gray, None);
        assert!(!e.hidden);
    }

    #[test]
    fn percentages_overwrite_every_entry_including_clearing_missing_ones() {
        let entry = |name: &str, pct: Option<f64>| SchemaEntry {
            api_name: name.into(),
            display_name: String::new(),
            description: String::new(),
            hidden: false,
            icon: None,
            icon_gray: None,
            global_percent: pct,
        };
        let mut s = Schema {
            appid: 1,
            source: "steam".into(),
            fetched_at: 0,
            achievements: vec![entry("A", Some(99.0)), entry("B", None)],
        };
        let pct = HashMap::from([("B".to_string(), 12.5)]);
        apply_percentages(&mut s, &pct);
        assert_eq!(s.achievements[0].global_percent, None);
        assert_eq!(s.achievements[1].global_percent, Some(12.5));
    }

    #[test]
    fn local_schema_skips_unnamed_entries_and_keeps_http_icons() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("achievements.json");
        std::fs::write(
            &f,
            json!([
                { "displayName": "Nameless" },
                { "name": "A", "icon": "https://cdn/a.jpg", "icon_gray": "https://cdn/g.jpg" },
                { "name": "B", "icon": "missing/file.png" }
            ])
            .to_string(),
        )
        .unwrap();
        let s = read_local(7, &f).unwrap();
        assert_eq!(s.appid, 7);
        assert_eq!(s.achievements.len(), 2);
        assert_eq!(s.achievements[0].icon.as_deref(), Some("https://cdn/a.jpg"));
        assert_eq!(
            s.achievements[0].icon_gray.as_deref(),
            Some("https://cdn/g.jpg")
        );
        assert_eq!(s.achievements[1].icon, None, "unreadable icon file");
    }

    #[test]
    fn local_schema_rejects_non_array_or_unreadable_files() {
        let dir = tempfile::tempdir().unwrap();
        let obj = dir.path().join("obj.json");
        std::fs::write(&obj, r#"{"name":"A"}"#).unwrap();
        assert!(read_local(1, &obj).is_none());
        assert!(read_local(1, &dir.path().join("missing.json")).is_none());
    }

    #[test]
    fn find_prefers_an_exact_match_over_a_case_insensitive_one() {
        let entry = |name: &str, disp: &str| SchemaEntry {
            api_name: name.into(),
            display_name: disp.into(),
            description: String::new(),
            hidden: false,
            icon: None,
            icon_gray: None,
            global_percent: None,
        };
        let s = Schema {
            appid: 1,
            source: "steam".into(),
            fetched_at: 0,
            achievements: vec![entry("ach_a", "lower"), entry("ACH_A", "upper")],
        };
        assert_eq!(s.find("ACH_A").unwrap().display_name, "upper");
        assert_eq!(s.find("Ach_A").unwrap().display_name, "lower");
    }
}

//! Achievement schema (display names, descriptions, icons) for a Steam app id
//! or an Epic game.
//!
//! Emulator state files carry only API names, so the schema supplies
//! everything shown to the user. For a Steam app id, two sources, in order:
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
//! An Epic game's schema comes from **egdata.app**, a community database of
//! the Epic Games Store, which serves a sandbox's achievement definitions
//! without authentication (`/sandboxes/<id>/achievements`). The emulator only
//! knows the game's `AppName`, so the sandbox id is found by reading the
//! 32-hex-digit ids the game exe passes to the EOS SDK (product, sandbox and
//! deployment ids, among others) and trying each one; the id that answers is
//! kept in the cached schema. Only achievements the Epic store displays are
//! listed there.
//!
//! The result is cached per game under `achievement-schemas/` (`<appid>.json`
//! or `epic-<AppName>.json`) and refreshed after [`MAX_AGE_SECS`]. In offline
//! mode only the cache and the local file are used.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SCHEMA_URL: &str = "https://api.steampowered.com/ISteamUserStats/GetSchemaForGame/v2/";
const PERCENT_URL: &str =
    "https://api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/";
const EGDATA_URL: &str = "https://api.egdata.app/sandboxes";

/// At most this many ids from the exe are tried against egdata.
/// Each try is one small request, and the search stops at the first hit.
const MAX_SANDBOX_TRIES: usize = 48;
/// Executables larger than this aren't read for ids.
const MAX_EXE_BYTES: u64 = 512 * 1024 * 1024;

/// A cached schema is refetched after this long (achievements are sometimes
/// added in updates; percentages drift).
const MAX_AGE_SECS: i64 = 14 * 24 * 60 * 60;

/// How long to wait after a failed schema fetch before trying again.
const RETRY_SECS: u64 = 6 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaEntry {
    pub api_name: String,
    pub display_name: String,
    pub description: String,
    pub hidden: bool,
    /// Steam CDN URL, or an absolute path under app data for a local schema.
    pub icon: Option<String>,
    pub icon_gray: Option<String>,
    /// Share of the store's players who have unlocked it, 0–100.
    pub global_percent: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Schema {
    /// The Steam app id; 0 for an Epic schema.
    pub appid: u64,
    /// `"steam"`, `"local"`, or `"epic"`.
    pub source: String,
    pub fetched_at: i64,
    pub achievements: Vec<SchemaEntry>,
    /// The Epic sandbox id egdata answered for, so a refresh needn't re-read
    /// the exe.
    pub sandbox_id: Option<String>,
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

/// The cache key (file stem) for an Epic game, with anything that isn't safe
/// in a file name replaced.
fn epic_key(app_name: &str) -> String {
    let safe: String = app_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("epic-{safe}")
}

fn cache_file(key: &str) -> PathBuf {
    cache_dir().join(format!("{key}.json"))
}

/// Marker whose mtime records the last failed fetch of `key` from `source`
/// (for Steam: bad or expired key, network down, or an app Steam has no
/// achievements for; for egdata: no id in the exe that it knows).
fn failed_marker(key: &str, source: &str) -> PathBuf {
    cache_dir().join(format!("{key}.{source}-failed"))
}

/// True for [`RETRY_SECS`] after a failed fetch, so a bad key or an unknown
/// game doesn't send requests on every detail-pane load.
fn backed_off(marker: &Path) -> bool {
    std::fs::metadata(marker)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age.as_secs() < RETRY_SECS)
}

fn mark_failed(marker: &Path) {
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(marker, b"");
}

fn load_key(key: &str) -> Option<Schema> {
    let text = std::fs::read_to_string(cache_file(key)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn load_cached(appid: u64) -> Option<Schema> {
    load_key(&appid.to_string())
}

pub fn load_cached_epic(app_name: &str) -> Option<Schema> {
    load_key(&epic_key(app_name))
}

fn save_cached(key: &str, schema: &Schema) {
    let path = cache_file(key);
    let _ = std::fs::create_dir_all(cache_dir());
    match serde_json::to_vec_pretty(schema) {
        Ok(bytes) => {
            if let Err(e) = crate::paths::write_atomic(&path, &bytes, false) {
                tracing::warn!(error = %e, key, "achievements: couldn't cache schema");
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
    let key = appid.to_string();
    let marker = failed_marker(&key, "steam");
    let cached = load_key(&key);
    let now = chrono::Utc::now().timestamp();
    let offline = crate::config::offline_mode_enabled();
    let steam_due = !api_key.trim().is_empty() && !offline && !backed_off(&marker);
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
                let _ = std::fs::remove_file(&marker);
                save_cached(&key, &schema);
                return Some(schema);
            }
            Err(e) => {
                tracing::warn!(error = %e, appid, "achievements: Steam schema fetch failed");
                mark_failed(&marker);
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
            save_cached(&key, &schema);
            return Some(schema);
        }
    }
    cached
}

/// The schema for an Epic game: a fresh cache entry if there is one, else a
/// fetch from egdata.app, else a stale cache entry. `list_exes` names the
/// executables to read for candidate sandbox ids; it runs on the blocking
/// pool, and only when no cached id still works.
pub async fn ensure_epic(
    http: &reqwest::Client,
    app_name: &str,
    list_exes: impl FnOnce() -> Vec<PathBuf> + Send + 'static,
) -> Option<Schema> {
    let key = epic_key(app_name);
    let marker = failed_marker(&key, "egdata");
    let cached = load_key(&key);
    let now = chrono::Utc::now().timestamp();
    let offline = crate::config::offline_mode_enabled();
    if let Some(c) = &cached {
        if offline || now - c.fetched_at < MAX_AGE_SECS {
            return cached;
        }
    }
    if offline || backed_off(&marker) {
        return cached;
    }

    // The cached sandbox id is tried first; the exes are only read when there
    // is none or egdata no longer answers for it.
    let mut ids: Vec<String> = cached.iter().filter_map(|c| c.sandbox_id.clone()).collect();
    let mut list_exes = Some(list_exes);
    let mut next = 0;
    let found = loop {
        if next == ids.len() {
            let Some(list) = list_exes.take() else {
                break None;
            };
            let scanned = tokio::task::spawn_blocking(move || sandbox_candidates(&list()))
                .await
                .unwrap_or_default();
            for id in scanned {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            continue;
        }
        match fetch_egdata(http, &ids[next]).await {
            Ok(Some(schema)) => break Some(schema),
            // Not a sandbox id egdata knows; try the next.
            Ok(None) => next += 1,
            Err(e) => {
                tracing::warn!(error = %e, app_name, "achievements: egdata schema fetch failed");
                break None;
            }
        }
    };
    if let Some(schema) = found {
        let _ = std::fs::remove_file(&marker);
        save_cached(&key, &schema);
        return Some(schema);
    }
    tracing::info!(
        app_name,
        tried = next,
        "achievements: no egdata schema found"
    );
    mark_failed(&marker);
    cached
}

/// `Ok(None)` when egdata has no achievements under `sandbox_id`, including
/// a client error for an id that isn't a sandbox id. Rate limiting, server
/// errors and network failures are `Err`, which stops the search.
async fn fetch_egdata(http: &reqwest::Client, sandbox_id: &str) -> AppResult<Option<Schema>> {
    let res = http
        .get(format!("{EGDATA_URL}/{sandbox_id}/achievements"))
        .send()
        .await
        .map_err(net)?;
    let status = res.status();
    if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(AppError::Other(format!("egdata returned {status}")));
    }
    let body: Value = res.json().await.map_err(net)?;
    Ok(egdata_schema(&body, sandbox_id))
}

/// Maps egdata's response (a list of achievement sets, the base game's and
/// one per add-on) onto a schema, base set first.
fn egdata_schema(body: &Value, sandbox_id: &str) -> Option<Schema> {
    let mut sets: Vec<&Value> = body.as_array()?.iter().collect();
    sets.sort_by_key(|s| !s.get("isBase").and_then(Value::as_bool).unwrap_or(false));
    let text = |v: &Value, keys: &[&str]| {
        keys.iter()
            .filter_map(|k| v.get(*k).and_then(Value::as_str))
            .find(|s| !s.trim().is_empty())
            .unwrap_or_default()
            .to_string()
    };
    let url = |v: &Value, k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .filter(|s| s.starts_with("http"))
            .map(str::to_string)
    };
    let achievements: Vec<_> = sets
        .iter()
        .filter_map(|s| s.get("achievements")?.as_array())
        .flatten()
        .filter_map(|v| {
            Some(SchemaEntry {
                api_name: v.get("name")?.as_str()?.to_string(),
                display_name: text(v, &["unlockedDisplayName", "lockedDisplayName"]),
                description: text(v, &["unlockedDescription", "lockedDescription"]),
                hidden: truthy(v.get("hidden")),
                icon: url(v, "unlockedIconLink"),
                icon_gray: url(v, "lockedIconLink"),
                global_percent: v.get("completedPercent").and_then(Value::as_f64),
            })
        })
        .collect();
    if achievements.is_empty() {
        return None;
    }
    Some(Schema {
        appid: 0,
        source: "epic".into(),
        fetched_at: chrono::Utc::now().timestamp(),
        achievements,
        sandbox_id: Some(sandbox_id.to_string()),
    })
}

/// Distinct 32-hex-digit ids found in `exes`, in the order given and then
/// file order, capped at [`MAX_SANDBOX_TRIES`]. Each exe is read in chunks,
/// so a large one isn't held in memory.
fn sandbox_candidates(exes: &[PathBuf]) -> Vec<String> {
    use std::io::Read;
    let mut out: Vec<String> = Vec::new();
    let mut buf = vec![0u8; 1024 * 1024];
    for exe in exes {
        if std::fs::metadata(exe).map_or(true, |m| m.len() > MAX_EXE_BYTES) {
            continue;
        }
        let Ok(mut file) = std::fs::File::open(exe) else {
            continue;
        };
        let mut scanner = HexScanner::default();
        while let Ok(n) = file.read(&mut buf) {
            if n == 0 {
                break;
            }
            scanner.feed(&buf[..n]);
        }
        for id in scanner.finish() {
            if !out.contains(&id) {
                out.push(id);
            }
        }
        if out.len() >= MAX_SANDBOX_TRIES {
            break;
        }
    }
    out.truncate(MAX_SANDBOX_TRIES);
    out
}

/// Lower-case 32-hex-digit runs not touching other letters or digits, stored
/// as ASCII or as UTF-16LE.
#[cfg(test)]
fn hex_ids(bytes: &[u8]) -> Vec<String> {
    let mut scanner = HexScanner::default();
    scanner.feed(bytes);
    scanner.finish()
}

/// One character stream being searched for ids.
#[derive(Default)]
struct HexRun {
    run: Vec<u8>,
    /// Length of the current run of hex digits, which may exceed `run`.
    len: usize,
    /// Whether the character before the current run was neither a letter nor
    /// a digit.
    touching: bool,
}

impl HexRun {
    fn feed(&mut self, c: u8, out: &mut Vec<String>) {
        if matches!(c, b'0'..=b'9' | b'a'..=b'f') {
            if self.len < 32 {
                self.run.push(c);
            }
            self.len += 1;
        } else {
            self.end(!c.is_ascii_alphanumeric(), out);
            self.touching = c.is_ascii_alphanumeric();
        }
    }

    fn end(&mut self, bounded_after: bool, out: &mut Vec<String>) {
        if self.len == 32 && !self.touching && bounded_after {
            out.push(String::from_utf8_lossy(&self.run).into_owned());
        }
        self.run.clear();
        self.len = 0;
    }
}

/// Finds ids in a byte stream fed in chunks: one ASCII stream and two
/// UTF-16LE streams (code units starting at even and at odd offsets).
#[derive(Default)]
struct HexScanner {
    ascii: HexRun,
    utf16: [HexRun; 2],
    /// The low byte of each UTF-16 stream's unfinished code unit.
    pending: [Option<u8>; 2],
    pos: usize,
    out: Vec<String>,
}

impl HexScanner {
    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.ascii.feed(b, &mut self.out);
            let lane = self.pos % 2;
            let other = 1 - lane;
            if let Some(low) = self.pending[other].take() {
                // A code unit outside ASCII reads as a separator.
                let c = if b == 0 { low } else { 0 };
                self.utf16[other].feed(c, &mut self.out);
            }
            self.pending[lane] = Some(b);
            self.pos += 1;
        }
    }

    fn finish(mut self) -> Vec<String> {
        self.ascii.end(true, &mut self.out);
        for lane in &mut self.utf16 {
            lane.end(true, &mut self.out);
        }
        self.out
    }
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
        sandbox_id: None,
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
        sandbox_id: None,
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        };
        assert_eq!(s.find("ACH_A").unwrap().display_name, "upper");
        assert_eq!(s.find("Ach_A").unwrap().display_name, "lower");
    }

    #[test]
    fn egdata_sets_map_base_first() {
        let ach = |name: &str| {
            json!({
                "name": name, "hidden": false,
                "unlockedDisplayName": format!("{name} title"), "lockedDisplayName": "x",
                "unlockedDescription": "", "lockedDescription": "Do the thing.",
                "unlockedIconLink": "https://cdn/u.png", "lockedIconLink": "https://cdn/l.png",
                "completedPercent": 82
            })
        };
        let body = json!([
            { "isBase": false, "achievements": [ach("DLC1")] },
            { "isBase": true, "achievements": [ach("01"), ach("02")] }
        ]);
        let s = egdata_schema(&body, "sb").unwrap();
        assert_eq!(s.source, "epic");
        assert_eq!(s.sandbox_id.as_deref(), Some("sb"));
        let names: Vec<_> = s.achievements.iter().map(|a| a.api_name.as_str()).collect();
        assert_eq!(names, ["01", "02", "DLC1"]);
        let a = &s.achievements[0];
        assert_eq!(a.display_name, "01 title");
        assert_eq!(
            a.description, "Do the thing.",
            "empty unlocked text falls back"
        );
        assert_eq!(a.icon.as_deref(), Some("https://cdn/u.png"));
        assert_eq!(a.icon_gray.as_deref(), Some("https://cdn/l.png"));
        assert_eq!(a.global_percent, Some(82.0));

        assert!(egdata_schema(&json!([]), "sb").is_none());
        assert!(egdata_schema(&json!({ "error": "x" }), "sb").is_none());
    }

    #[test]
    fn hex_ids_finds_bounded_ascii_and_utf16_ids() {
        let id = "2e92a78949e2474aa89271b8b893f3b0";
        let mut bytes = b"\0junk\0".to_vec();
        bytes.extend_from_slice(id.as_bytes());
        bytes.push(0);
        // Too long, upper case, and touching a letter: all ignored.
        bytes.extend_from_slice(b"\x002e92a78949e2474aa89271b8b893f3b0ff\0");
        bytes.extend_from_slice(b"\x002E92A78949E2474AA89271B8B893F3B0\0");
        bytes.extend_from_slice(b"\0x47a73210e3894e45a52a037968f37137\0");
        for u in "b839067d7a114eb18d4fbba8890d6f00".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        bytes.extend_from_slice(&[0, 0]);
        assert_eq!(
            hex_ids(&bytes),
            [id.to_string(), "b839067d7a114eb18d4fbba8890d6f00".into()]
        );
        // At the very end of the file.
        assert_eq!(hex_ids(id.as_bytes()), [id.to_string()]);
    }

    #[test]
    fn hex_scanner_finds_ids_split_across_chunks() {
        let id = "2e92a78949e2474aa89271b8b893f3b0";
        let mut bytes = b"\0".to_vec();
        bytes.extend_from_slice(id.as_bytes());
        bytes.push(b'\n');
        for u in id.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        // Every split point, including mid-code-unit.
        for split in 0..=bytes.len() {
            let mut s = HexScanner::default();
            s.feed(&bytes[..split]);
            s.feed(&bytes[split..]);
            assert_eq!(s.finish(), [id, id], "split at {split}");
        }
        // A run longer than 32 digits is not an id, however long.
        assert!(hex_ids(&[b'a'; 100]).is_empty());
    }

    #[test]
    fn epic_key_is_a_safe_file_stem() {
        assert_eq!(epic_key("AlanWakeRemastered"), "epic-AlanWakeRemastered");
        assert_eq!(epic_key("A*b?c<d>|\"e"), "epic-A_b_c_d___e");
    }
}

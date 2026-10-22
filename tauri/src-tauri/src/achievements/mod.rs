//! Achievement detection from Steam-emulator stats files.
//!
//! Spool has no Steam login, so it reads the files an emulator bundled with the
//! game already writes (Goldberg / GSE, EMPRESS, CODEX, RUNE, Reloaded, 3DM,
//! Skidrow, SmartSteamEmu, CreamAPI, OnlineFix). [`locate`] lists where each
//! one writes, [`parse`] reads the formats, and [`schema`] supplies display
//! names and icons from the Steam Web API or the game's own
//! `steam_settings/achievements.json`.
//!
//! During a play session [`SessionWatch`] polls the candidate files every
//! [`POLL`]. The earned set at launch is the baseline (imported silently); an
//! achievement that appears after that is recorded as a *live* unlock in the
//! library DB's `achievement_unlocks` table, broadcast as
//! `achievement:unlocked`, and shown in the desktop overlay window
//! ([`overlay`]) unless Spool's own window has focus. In Steam Game Mode there's
//! no desktop to overlay, so the Decky plugin polls the headless server's
//! `/achievements/recent` (which reads the live rows) and shows a Steam toast
//! instead. One last scan runs after the
//! game exits to catch unlocks written on shutdown.
//!
//! Opening a game's detail view ([`get_game_achievements`]) also scans, so
//! unlocks from sessions run outside Spool are imported (not toasted).

mod locate;
pub mod overlay;
mod parse;
pub mod schema;

use crate::error::{AppError, AppResult};
use crate::library::{AchievementUnlock, GameEntry, SharedLibrary};
use locate::{FolderScan, Roots, Source};
use parse::{Earned, Key};
use schema::Schema;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager, State};

/// How often the session watcher stats the candidate files.
const POLL: Duration = Duration::from_secs(2);
/// The install folder is re-walked for local-save files every this many polls,
/// so a file the emulator creates there mid-session is picked up.
const GAME_FOLDER_RESCAN_TICKS: u32 = 15;

/// What's needed to find and name one game's achievements.
#[derive(Debug, Clone)]
struct GameContext {
    game_id: String,
    game_name: String,
    appid: u64,
    roots: Roots,
    game_folder: Option<PathBuf>,
    folder_scan: Option<FolderScan>,
}

impl GameContext {
    /// `None` when no Steam app id is known (no `steam_id` on the entry and no
    /// `steam_appid.txt` in the install folder) — emulator files are keyed by it.
    /// Walks the install folder (cached), so call it off the async runtime.
    fn for_entry(entry: &GameEntry) -> Option<Self> {
        let game_folder = entry
            .game_folder_path
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        let folder_scan = game_folder.as_deref().map(FolderScan::cached);
        let appid = entry
            .steam_id
            .filter(|id| *id > 0)
            .or_else(|| folder_scan.as_ref().and_then(|s| s.appid))?;
        let prefix = crate::proton::resolve_prefix_root(
            crate::proton::exe_needs_proton(&entry.exe_path),
            entry.wine_prefix_path.as_deref(),
            &entry.id,
        );
        let roots = match prefix {
            Some(p) => Roots::prefix(&p),
            None => Roots::native(),
        };
        Some(Self {
            game_id: entry.id.clone(),
            game_name: entry.game_name.clone(),
            appid,
            roots,
            game_folder,
            folder_scan,
        })
    }

    async fn load(entry: GameEntry) -> Option<Self> {
        tokio::task::spawn_blocking(move || Self::for_entry(&entry))
            .await
            .ok()
            .flatten()
    }

    fn local_schema_file(&self) -> Option<PathBuf> {
        self.folder_scan.as_ref()?.schema_file.clone()
    }

    /// Every file worth reading: the emulator locations plus any local saves
    /// in the install folder.
    fn sources(&self) -> Vec<Source> {
        let mut out = locate::candidate_sources(&self.roots, self.appid);
        if let Some(scan) = &self.folder_scan {
            out.extend(scan.sources(self.appid));
        }
        out
    }
}

fn read_source(src: &Source) -> Vec<Earned> {
    std::fs::read(&src.path)
        .map(|bytes| parse::parse(src.format, &bytes))
        .unwrap_or_default()
}

/// Identity used to tell "already seen" from "new": the upper-cased API name,
/// or the CRC32 for SmartSteamEmu entries. A name known by its schema casing
/// is registered under both.
fn seen_keys(api_name: &str) -> [String; 2] {
    [
        format!("n:{}", api_name.to_ascii_uppercase()),
        format!("c:{:08x}", crc32fast::hash(api_name.as_bytes())),
    ]
}

fn key_id(key: &Key) -> String {
    match key {
        Key::Name(n) => format!("n:{}", n.to_ascii_uppercase()),
        Key::Crc(c) => format!("c:{c:08x}"),
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Turns an earned entry into a DB row, naming it from the schema when
/// possible. A CRC key with no schema match can't be named, so it's dropped.
fn to_unlock(
    ctx: &GameContext,
    e: &Earned,
    schema: Option<&Schema>,
    live: bool,
) -> Option<AchievementUnlock> {
    let entry = match &e.key {
        Key::Name(n) => schema.and_then(|s| s.find(n)),
        Key::Crc(c) => Some(schema?.find_crc(*c)?),
    };
    let api_name = match (&e.key, entry) {
        (_, Some(s)) => s.api_name.clone(),
        (Key::Name(n), None) => n.clone(),
        (Key::Crc(_), None) => return None,
    };
    let detected_at = now_ms();
    Some(AchievementUnlock {
        game_name: ctx.game_name.clone(),
        api_name,
        unlocked_at: e.unlocked_at.unwrap_or(detected_at / 1000),
        detected_at,
        live: live as i64,
        display_name: entry
            .map(|s| s.display_name.clone())
            .filter(|n| !n.is_empty()),
        icon: entry.and_then(|s| s.icon.clone()),
    })
}

fn http_client(app: &AppHandle) -> reqwest::Client {
    app.try_state::<crate::metadata::MetadataClient>()
        .map(|c| c.http().clone())
        .unwrap_or_default()
}

fn load_config() -> crate::config::ConfigData {
    crate::config::Config::load()
        .map(|c| c.data)
        .unwrap_or_default()
}

// ── Session watcher ──────────────────────────────────────────────────────────

/// Payload of the `achievement:unlocked` event.
#[derive(Debug, Clone, Serialize)]
struct UnlockedEvent {
    game_id: String,
    game_name: String,
    api_name: String,
    display_name: String,
    description: String,
    icon: Option<String>,
}

/// The files being watched and the (mtime, size) each had when last read.
struct Tracked {
    sources: Vec<Source>,
    stamps: HashMap<PathBuf, (SystemTime, u64)>,
}

impl Tracked {
    fn new(sources: Vec<Source>) -> Self {
        Self {
            sources,
            stamps: HashMap::new(),
        }
    }

    fn add(&mut self, src: Source) {
        if !self.sources.contains(&src) {
            self.sources.push(src);
        }
    }

    /// Reads every source that changed since the last call (all of them when
    /// `force`), returning their earned entries.
    fn read_changed(&mut self, force: bool) -> Vec<Earned> {
        let mut out = Vec::new();
        for src in &self.sources {
            let Ok(meta) = std::fs::metadata(&src.path) else {
                continue;
            };
            let stamp = (
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                meta.len(),
            );
            if !force && self.stamps.get(&src.path) == Some(&stamp) {
                continue;
            }
            self.stamps.insert(src.path.clone(), stamp);
            out.extend(read_source(src));
        }
        out
    }
}

struct Watcher {
    app: AppHandle,
    library: SharedLibrary,
    ctx: GameContext,
    notify: bool,
    /// Filled by a background fetch; read on each unlock.
    schema: Arc<std::sync::Mutex<Option<Schema>>>,
    files: Tracked,
    seen: HashSet<String>,
    ticks: u32,
}

impl Watcher {
    fn schema(&self) -> Option<Schema> {
        self.schema.lock().ok().and_then(|s| s.clone())
    }

    /// Records what's already earned at launch, without notifying.
    async fn baseline(&mut self) {
        if let Ok(rows) = self
            .library
            .achievement_unlocks_for(&self.ctx.game_name)
            .await
        {
            for r in rows {
                self.seen.extend(seen_keys(&r.api_name));
            }
        }
        let schema = self.schema();
        let earned = self.files.read_changed(true);
        let rows: Vec<_> = earned
            .iter()
            .filter(|e| !self.seen.contains(&key_id(&e.key)))
            .filter_map(|e| to_unlock(&self.ctx, e, schema.as_ref(), false))
            .collect();
        for e in &earned {
            self.seen.insert(key_id(&e.key));
        }
        if let Err(e) = self.library.insert_achievement_unlocks(&rows).await {
            tracing::warn!(error = %e, "achievements: baseline import failed");
        }
        tracing::info!(
            game = %self.ctx.game_name,
            appid = self.ctx.appid,
            sources = self.files.sources.len(),
            already_earned = self.seen.len(),
            "achievement watcher started"
        );
    }

    async fn tick(&mut self, force: bool) {
        self.ticks += 1;
        if self.ticks.is_multiple_of(GAME_FOLDER_RESCAN_TICKS) {
            if let Some(folder) = self.ctx.game_folder.clone() {
                let appid = self.ctx.appid;
                let found =
                    tokio::task::spawn_blocking(move || FolderScan::walk(&folder).sources(appid))
                        .await
                        .unwrap_or_default();
                for src in found {
                    self.files.add(src);
                }
            }
        }
        let earned = self.files.read_changed(force);
        if earned.is_empty() {
            return;
        }
        let schema = self.schema();
        let mut rows = Vec::new();
        for e in &earned {
            if self.seen.contains(&key_id(&e.key)) {
                continue;
            }
            // Unnamed (CRC without a schema yet) entries stay unseen so a later
            // read retries them once the schema has loaded.
            if let Some(row) = to_unlock(&self.ctx, e, schema.as_ref(), true) {
                self.seen.extend(seen_keys(&row.api_name));
                self.seen.insert(key_id(&e.key));
                rows.push(row);
            }
        }
        let added = match self.library.insert_achievement_unlocks(&rows).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(error = %e, "achievements: failed to record unlocks");
                return;
            }
        };
        for row in added {
            self.announce(&row, schema.as_ref());
        }
    }

    fn announce(&self, row: &AchievementUnlock, schema: Option<&Schema>) {
        let entry = schema.and_then(|s| s.find(&row.api_name));
        let display_name = row
            .display_name
            .clone()
            .unwrap_or_else(|| row.api_name.clone());
        tracing::info!(game = %self.ctx.game_name, achievement = %row.api_name, "achievement unlocked");
        let _ = self.app.emit(
            "achievement:unlocked",
            UnlockedEvent {
                game_id: self.ctx.game_id.clone(),
                game_name: self.ctx.game_name.clone(),
                api_name: row.api_name.clone(),
                display_name: display_name.clone(),
                description: entry.map(|e| e.description.clone()).unwrap_or_default(),
                icon: row.icon.clone(),
            },
        );
        // In Steam Game Mode the Decky plugin shows the toast (gamescope has no
        // desktop to overlay). When Spool's own window has focus, the in-app
        // toast from `achievement:unlocked` covers it.
        let spool_focused = self
            .app
            .get_webview_window("main")
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false);
        if self.notify && !crate::gamemode::is_steam_game_mode() && !spool_focused {
            overlay::show(
                &self.app,
                display_name,
                entry.map(|e| e.description.clone()).unwrap_or_default(),
                self.ctx.game_name.clone(),
                row.icon.clone(),
            );
        }
    }
}

/// Watches a game's achievement files for the length of a play session.
pub struct SessionWatch {
    task: tokio::task::JoinHandle<()>,
    /// `None` until the baseline has been taken, and for a game with no known
    /// Steam app id.
    watcher: Arc<tokio::sync::Mutex<Option<Watcher>>>,
}

impl SessionWatch {
    /// Starts watching in the background, so the launch isn't held up by the
    /// install-folder walk. The baseline is read as the game starts; games
    /// don't write their stats file in the first moments, and an unlock that
    /// did land before the baseline is still recorded, just not announced.
    pub fn start(app: &AppHandle, game_id: &str) -> Self {
        let slot = Arc::new(tokio::sync::Mutex::new(None));
        let task = {
            let (app, game_id, slot) = (app.clone(), game_id.to_string(), slot.clone());
            tokio::spawn(async move {
                let Some(watcher) = Watcher::start(app, &game_id).await else {
                    return;
                };
                *slot.lock().await = Some(watcher);
                let mut interval = tokio::time::interval(POLL);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    if let Some(w) = slot.lock().await.as_mut() {
                        w.tick(false).await;
                    }
                }
            })
        };
        Self {
            task,
            watcher: slot,
        }
    }

    /// Stops polling and reads every file once more, catching unlocks the
    /// game wrote as it exited.
    pub async fn finish(self) {
        self.task.abort();
        let _ = self.task.await;
        if let Some(w) = self.watcher.lock().await.as_mut() {
            w.tick(true).await;
        }
    }
}

impl Watcher {
    /// Builds the watcher and takes the baseline. `None` when the game has no
    /// known Steam app id.
    async fn start(app: AppHandle, game_id: &str) -> Option<Self> {
        let library = app.state::<SharedLibrary>().inner().clone();
        let entry = library.find(game_id).await.ok().flatten()?;
        let ctx = GameContext::load(entry).await?;
        let config = load_config();

        let schema_slot = Arc::new(std::sync::Mutex::new(schema::load_cached(ctx.appid)));
        {
            let slot = schema_slot.clone();
            let http = http_client(&app);
            let (appid, local, key) = (
                ctx.appid,
                ctx.local_schema_file(),
                config.steam_web_api_key.clone(),
            );
            tokio::spawn(async move {
                if let Some(s) = schema::ensure(&http, appid, local.as_deref(), &key).await {
                    if let Ok(mut g) = slot.lock() {
                        *g = Some(s);
                    }
                }
            });
        }

        let sources = ctx.sources();
        let mut watcher = Watcher {
            app,
            library,
            ctx,
            notify: config.achievement_notifications,
            schema: schema_slot,
            files: Tracked::new(sources),
            seen: HashSet::new(),
            ticks: 0,
        };
        watcher.baseline().await;
        Some(watcher)
    }
}

// ── Library view ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AchievementView {
    pub api_name: String,
    pub display_name: String,
    pub description: String,
    pub hidden: bool,
    pub icon: Option<String>,
    pub icon_gray: Option<String>,
    pub global_percent: Option<f64>,
    /// Unix seconds; `None` while locked.
    pub unlocked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameAchievements {
    /// The Steam app id the files are keyed by, `None` when unknown.
    pub appid: Option<u64>,
    /// `"steam"`, `"local"`, or `None` when no schema is available (the list
    /// then holds only unlocked achievements, by API name).
    pub schema_source: Option<String>,
    pub total: usize,
    pub unlocked: usize,
    pub items: Vec<AchievementView>,
}

/// Scans a game's achievement files (importing any unlocks not yet recorded)
/// and returns the full list, schema entries first, merged with the recorded
/// unlocks.
#[tauri::command]
pub async fn get_game_achievements(
    app: AppHandle,
    library: State<'_, SharedLibrary>,
    game_id: String,
) -> AppResult<GameAchievements> {
    let entry = library
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game {game_id} not found")))?;
    let game_name = entry.game_name.clone();
    let Some(ctx) = GameContext::load(entry).await else {
        let rows = library.achievement_unlocks_for(&game_name).await?;
        return Ok(merge(None, None, rows));
    };
    let config = load_config();
    let schema = schema::ensure(
        &http_client(&app),
        ctx.appid,
        ctx.local_schema_file().as_deref(),
        &config.steam_web_api_key,
    )
    .await;

    let earned = {
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || {
            ctx.sources()
                .iter()
                .flat_map(read_source)
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default()
    };
    let rows: Vec<_> = earned
        .iter()
        .filter_map(|e| to_unlock(&ctx, e, schema.as_ref(), false))
        .collect();
    library.insert_achievement_unlocks(&rows).await?;

    let rows = library.achievement_unlocks_for(&ctx.game_name).await?;
    Ok(merge(Some(ctx.appid), schema.as_ref(), rows))
}

fn merge(
    appid: Option<u64>,
    schema: Option<&Schema>,
    rows: Vec<AchievementUnlock>,
) -> GameAchievements {
    let mut by_name: HashMap<String, &AchievementUnlock> = rows
        .iter()
        .map(|r| (r.api_name.to_ascii_uppercase(), r))
        .collect();
    let mut items = Vec::new();
    if let Some(s) = schema {
        for e in &s.achievements {
            let row = by_name.remove(&e.api_name.to_ascii_uppercase());
            items.push(AchievementView {
                api_name: e.api_name.clone(),
                display_name: if e.display_name.is_empty() {
                    e.api_name.clone()
                } else {
                    e.display_name.clone()
                },
                description: e.description.clone(),
                hidden: e.hidden,
                icon: e.icon.clone(),
                icon_gray: e.icon_gray.clone(),
                global_percent: e.global_percent,
                unlocked_at: row.map(|r| r.unlocked_at),
            });
        }
    }
    // Unlocks the schema doesn't list (no schema, or renamed since).
    let mut extra: Vec<_> = by_name.into_values().collect();
    extra.sort_by_key(|r| r.unlocked_at);
    for r in extra {
        items.push(AchievementView {
            api_name: r.api_name.clone(),
            display_name: r.display_name.clone().unwrap_or_else(|| r.api_name.clone()),
            description: String::new(),
            hidden: false,
            icon: r.icon.clone(),
            icon_gray: None,
            global_percent: None,
            unlocked_at: Some(r.unlocked_at),
        });
    }
    GameAchievements {
        appid,
        schema_source: schema.map(|s| s.source.clone()),
        total: items.len(),
        unlocked: items.iter().filter(|i| i.unlocked_at.is_some()).count(),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> GameContext {
        GameContext {
            game_id: "g".into(),
            game_name: "Game".into(),
            appid: 480,
            roots: Roots::default(),
            game_folder: None,
            folder_scan: None,
        }
    }

    fn schema() -> Schema {
        let entry = |n: &str| schema::SchemaEntry {
            api_name: n.into(),
            display_name: format!("{n} name"),
            description: String::new(),
            hidden: false,
            icon: Some(format!("https://cdn/{n}.jpg")),
            icon_gray: None,
            global_percent: Some(12.5),
        };
        Schema {
            appid: 480,
            source: "steam".into(),
            fetched_at: 0,
            achievements: vec![entry("ACH_A"), entry("ACH_B")],
        }
    }

    #[test]
    fn to_unlock_names_from_schema_and_drops_unknown_crc() {
        let s = schema();
        let e = Earned {
            key: Key::Name("ach_a".into()),
            unlocked_at: Some(1_700_000_000),
        };
        let row = to_unlock(&ctx(), &e, Some(&s), true).unwrap();
        assert_eq!(row.api_name, "ACH_A");
        assert_eq!(row.display_name.as_deref(), Some("ACH_A name"));
        assert_eq!(row.live, 1);

        let crc = Earned {
            key: Key::Crc(crc32fast::hash(b"ACH_B")),
            unlocked_at: None,
        };
        assert_eq!(
            to_unlock(&ctx(), &crc, Some(&s), false).unwrap().api_name,
            "ACH_B"
        );
        assert!(to_unlock(&ctx(), &crc, None, false).is_none());

        // Without a schema the file's name is kept as-is.
        assert_eq!(
            to_unlock(&ctx(), &e, None, false).unwrap().api_name,
            "ach_a"
        );
    }

    #[test]
    fn merge_lists_schema_then_extra_unlocks() {
        let row = |n: &str, t: i64| AchievementUnlock {
            game_name: "Game".into(),
            api_name: n.into(),
            unlocked_at: t,
            detected_at: 0,
            live: 0,
            display_name: None,
            icon: None,
        };
        let out = merge(
            Some(480),
            Some(&schema()),
            vec![row("ach_b", 5), row("OLD", 3)],
        );
        assert_eq!(out.total, 3);
        assert_eq!(out.unlocked, 2);
        assert_eq!(out.items[0].unlocked_at, None);
        assert_eq!(out.items[1].unlocked_at, Some(5));
        assert_eq!(out.items[2].api_name, "OLD");
    }

    #[tokio::test]
    async fn watcher_reports_only_new_unlocks() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("GSE Saves/480/achievements.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"{"ACH_A":{"earned":true,"earned_time":1700000000}}"#,
        )
        .unwrap();

        let library = Arc::new(crate::library::Library::open_in_memory().await.unwrap());
        let mut c = ctx();
        c.roots = Roots {
            appdata: Some(dir.path().to_path_buf()),
            ..Roots::default()
        };
        let mut files = Tracked::new(c.sources());
        let mut seen = HashSet::new();
        let mut read = |force: bool| files.read_changed(force);

        // Baseline: ACH_A already earned, imported silently.
        for e in read(true) {
            seen.insert(key_id(&e.key));
            let row = to_unlock(&c, &e, None, false).unwrap();
            library.insert_achievement_unlocks(&[row]).await.unwrap();
        }
        assert!(read(false).is_empty(), "unchanged file is not re-read");

        std::fs::write(
            &file,
            r#"{"ACH_A":{"earned":true,"earned_time":1700000000},"ACH_B":{"earned":true,"earned_time":1700000500}}"#,
        )
        .unwrap();
        let new: Vec<_> = read(false)
            .into_iter()
            .filter(|e| !seen.contains(&key_id(&e.key)))
            .collect();
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].key, Key::Name("ACH_B".into()));
        let row = to_unlock(&c, &new[0], None, true).unwrap();
        let added = library
            .insert_achievement_unlocks(std::slice::from_ref(&row))
            .await
            .unwrap();
        assert_eq!(added.len(), 1);
        // Recording the same unlock again is a no-op.
        assert!(library
            .insert_achievement_unlocks(&[row])
            .await
            .unwrap()
            .is_empty());

        let live = library.live_achievement_unlocks_since(0).await.unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].api_name, "ACH_B");
    }
}

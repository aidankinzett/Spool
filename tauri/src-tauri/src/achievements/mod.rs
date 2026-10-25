//! Achievement detection from Steam- and Epic-emulator stats files.
//!
//! Spool has no Steam or Epic login, so it reads the files an emulator bundled
//! with the game already writes (Goldberg / GSE, EMPRESS, CODEX, RUNE,
//! Reloaded, 3DM, Skidrow, SmartSteamEmu, CreamAPI, OnlineFix, and RUNE's
//! Epic Online Services emulator). [`locate`] lists where each one writes,
//! [`parse`] reads the formats, and [`schema`] supplies display names and
//! icons from the Steam Web API, the game's own
//! `steam_settings/achievements.json`, or (for Epic games) egdata.app.
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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager, State};

/// How often the session watcher stats the candidate files.
const POLL: Duration = Duration::from_secs(2);
/// The install folder is re-walked for local-save files every this many polls
/// (once a minute), so a file the emulator creates there mid-session is picked
/// up. Uncached, since the point is to see new files.
const GAME_FOLDER_RESCAN_TICKS: u32 = 30;

/// What's needed to find and name one game's achievements.
#[derive(Debug, Clone)]
struct GameContext {
    game_id: String,
    game_name: String,
    /// The Steam app id Steam emulators key their files by.
    appid: Option<u64>,
    /// The `AppName` RUNE's Epic emulator keys its file by.
    epic_app: Option<String>,
    exe_path: PathBuf,
    roots: Roots,
    game_folder: Option<PathBuf>,
    folder_scan: Option<FolderScan>,
}

impl GameContext {
    /// `None` when neither a Steam app id (`steam_appid.txt` in the install
    /// folder, or the entry's `steam_id`) nor an Epic emulator `AppName` is
    /// known — emulator files are keyed by one of them.
    /// Walks the install folder (cached), so call it off the async runtime.
    fn for_entry(entry: &GameEntry) -> Option<Self> {
        let game_folder = entry
            .game_folder_path
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        let folder_scan = game_folder.as_deref().map(FolderScan::cached);
        // The emulator's own app id wins: its save folder is keyed by it, and
        // the library's `steam_id` (from metadata matching) can be another
        // edition's.
        let epic_app = folder_scan
            .as_ref()
            .and_then(|s| s.epic.as_ref())
            .map(|e| e.app_name.clone());
        // With an Epic emulator in the folder, the entry's `steam_id` only
        // says which Steam game matched the name; no Steam emulator writes
        // under it.
        let appid = folder_scan
            .as_ref()
            .and_then(|s| s.appid)
            .or(entry.steam_id.filter(|id| *id > 0 && epic_app.is_none()));
        if appid.is_none() && epic_app.is_none() {
            return None;
        }
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
            epic_app,
            exe_path: PathBuf::from(&entry.exe_path),
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
        let mut out = Vec::new();
        if let Some(appid) = self.appid {
            out.extend(locate::candidate_sources(&self.roots, appid));
            if let Some(scan) = &self.folder_scan {
                out.extend(scan.sources(appid));
            }
        }
        if let Some(app) = &self.epic_app {
            out.extend(locate::epic_candidate_sources(&self.roots, app));
        }
        out
    }

    /// The cached schema, without fetching. Reads a file, so call it off the
    /// async runtime.
    fn cached_schema(&self) -> Option<Schema> {
        self.epic_app
            .as_deref()
            .and_then(schema::load_cached_epic)
            .or_else(|| self.appid.and_then(schema::load_cached))
    }

    /// The schema, fetched if the cache is missing or stale. An Epic
    /// emulator's achievement ids are Epic's, so egdata is asked first when
    /// one is found. The Steam schema is the fallback; `appid` is only set
    /// for such a game when a Steam emulator ships beside it.
    async fn ensure_schema(&self, http: &reqwest::Client, api_key: &str) -> Option<Schema> {
        if let Some(app) = &self.epic_app {
            let exe = self.exe_path.clone();
            let dir = self
                .folder_scan
                .as_ref()
                .and_then(|s| s.epic.as_ref())
                .map(|e| e.dir.clone());
            let epic =
                schema::ensure_epic(http, app, move || epic_exes(&exe, dir.as_deref())).await;
            if epic.is_some() {
                return epic;
            }
        }
        let appid = self.appid?;
        schema::ensure(http, appid, self.local_schema_file().as_deref(), api_key).await
    }
}

/// Path equality, ignoring case on Windows, where the library's paths and the
/// folder walk's can differ only in case.
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

/// Executables that may hold the game's EOS ids, most likely first: the
/// entry's exe when it sits beside the Epic emulator's ini, the other exes
/// there, then the entry's exe when it's elsewhere (often a launcher).
fn epic_exes(entry_exe: &Path, ini_dir: Option<&Path>) -> Vec<PathBuf> {
    let in_dir = ini_dir.is_some_and(|d| entry_exe.parent().is_some_and(|p| same_path(p, d)));
    let mut out = Vec::new();
    if in_dir {
        out.push(entry_exe.to_path_buf());
    }
    if let Some(rd) = ini_dir.and_then(|d| std::fs::read_dir(d).ok()) {
        let mut beside: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                name.ends_with(".exe") && !name.starts_with("unins")
            })
            .map(|e| e.path())
            .filter(|p| !same_path(p, entry_exe))
            .collect();
        beside.sort();
        out.extend(beside);
    }
    if !in_dir {
        out.push(entry_exe.to_path_buf());
    }
    out
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
        // This device's own row.
        device_id: String::new(),
        device_name: String::new(),
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
#[derive(Default)]
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
    /// Unix ms when the watcher started; see [`Library::record_live_unlocks`].
    started_ms: i64,
    /// Filled by a background fetch; read on each unlock.
    schema: Arc<std::sync::Mutex<Option<Schema>>>,
    files: Tracked,
    /// Achievements this device has recorded (see [`seen_keys`]).
    seen: HashSet<String>,
    /// Achievements only another device has recorded (folded in). When this
    /// device earns one, it's recorded as this device's but not announced: with
    /// synced saves, a game often re-awards everything from the other device
    /// the first time it loads the save, which would be a burst of toasts.
    elsewhere: HashSet<String>,
    ticks: u32,
}

impl Watcher {
    fn schema(&self) -> Option<Schema> {
        self.schema.lock().ok().and_then(|s| s.clone())
    }

    /// [`Tracked::read_changed`] on the blocking pool (it stats every
    /// candidate file).
    async fn read_changed(&mut self, force: bool) -> Vec<Earned> {
        let mut files = std::mem::take(&mut self.files);
        let (files, earned) = tokio::task::spawn_blocking(move || {
            let earned = files.read_changed(force);
            (files, earned)
        })
        .await
        .unwrap_or_default();
        self.files = files;
        earned
    }

    /// Records what's already earned at launch, without notifying.
    async fn baseline(&mut self) {
        if let Ok(rows) = self
            .library
            .achievement_unlocks_for(&self.ctx.game_name)
            .await
        {
            for r in rows {
                if r.device_id.is_empty() {
                    self.seen.extend(seen_keys(&r.api_name));
                } else {
                    self.elsewhere.extend(seen_keys(&r.api_name));
                }
            }
        }
        let schema = self.schema();
        let earned = self.read_changed(true).await;
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
            appid = ?self.ctx.appid,
            epic_app = ?self.ctx.epic_app,
            sources = self.files.sources.len(),
            already_earned = self.seen.len(),
            "achievement watcher started"
        );
    }

    async fn tick(&mut self, force: bool) {
        self.ticks += 1;
        if self.ticks.is_multiple_of(GAME_FOLDER_RESCAN_TICKS) {
            if let (Some(folder), Some(appid)) = (self.ctx.game_folder.clone(), self.ctx.appid) {
                let found =
                    tokio::task::spawn_blocking(move || FolderScan::walk(&folder).sources(appid))
                        .await
                        .unwrap_or_default();
                for src in found {
                    self.files.add(src);
                }
            }
        }
        let earned = self.read_changed(force).await;
        if earned.is_empty() {
            return;
        }
        if earned.iter().any(|e| !self.seen.contains(&key_id(&e.key))) {
            self.refresh_elsewhere().await;
        }
        let schema = self.schema();
        let (mut live, mut quiet, mut keys) = (Vec::new(), Vec::new(), Vec::new());
        for e in &earned {
            let key = key_id(&e.key);
            if self.seen.contains(&key) {
                continue;
            }
            // A CRC entry with no schema loaded yet can't be named, so it stays
            // unseen. It's retried the next time its file changes, and on the
            // forced read after the game exits.
            let Some(row) = to_unlock(&self.ctx, e, schema.as_ref(), true) else {
                continue;
            };
            let known_elsewhere = self.elsewhere.contains(&key)
                || seen_keys(&row.api_name)
                    .iter()
                    .any(|k| self.elsewhere.contains(k));
            keys.push(key);
            if known_elsewhere {
                quiet.push(AchievementUnlock { live: 0, ..row });
            } else {
                live.push(row);
            }
        }
        if live.is_empty() && quiet.is_empty() {
            return;
        }
        let recorded = async {
            self.library.insert_achievement_unlocks(&quiet).await?;
            self.library
                .record_live_unlocks(&live, self.started_ms)
                .await
        };
        let added = match recorded.await {
            Ok(a) => a,
            Err(e) => {
                // Nothing is marked seen; forget the file stamps so the next
                // tick re-reads every file and retries.
                tracing::warn!(error = %e, "achievements: failed to record unlocks");
                self.files.stamps.clear();
                return;
            }
        };
        for row in live.iter().chain(&quiet) {
            self.seen.extend(seen_keys(&row.api_name));
        }
        self.seen.extend(keys);
        for row in added {
            self.announce(&row, schema.as_ref());
        }
    }

    /// Re-reads which achievements other devices have recorded. A fold can
    /// run mid-session (the detail pane, the headless server's `/fold`), so
    /// the set from the baseline may be out of date by the time this device
    /// earns one. Only called when a tick finds something new.
    async fn refresh_elsewhere(&mut self) {
        match self
            .library
            .achievement_unlocks_for(&self.ctx.game_name)
            .await
        {
            Ok(rows) => {
                for r in rows.iter().filter(|r| !r.device_id.is_empty()) {
                    self.elsewhere.extend(seen_keys(&r.api_name));
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "achievements: couldn't refresh other devices' unlocks")
            }
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
        let spool_focused = self.app.webview_windows().iter().any(|(label, w)| {
            !label.starts_with(overlay::LABEL_PREFIX) && w.is_focused().unwrap_or(false)
        });
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
/// Dropping it without [`finish`](Self::finish) also stops the polling loop,
/// since the loop ends when `stop` is dropped.
pub struct SessionWatch {
    task: tokio::task::JoinHandle<()>,
    stop: tokio::sync::watch::Sender<bool>,
    /// `None` until the baseline has been taken, and for a game with neither
    /// a known Steam app id nor an Epic emulator.
    watcher: Arc<tokio::sync::Mutex<Option<Watcher>>>,
}

impl SessionWatch {
    /// Starts watching in the background, so the launch isn't held up by the
    /// install-folder walk. The baseline is read as the game starts; games
    /// don't write their stats file in the first moments, and an unlock that
    /// did land before the baseline is still recorded, just not announced.
    pub fn start(app: &AppHandle, game_id: &str) -> Self {
        let slot = Arc::new(tokio::sync::Mutex::new(None));
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let task = {
            let (app, game_id, slot) = (app.clone(), game_id.to_string(), slot.clone());
            tokio::spawn(tracing::Instrument::in_current_span(async move {
                let Some(watcher) = Watcher::start(app, &game_id).await else {
                    return;
                };
                *slot.lock().await = Some(watcher);
                let mut interval = tokio::time::interval(POLL);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                interval.tick().await;
                loop {
                    // Only the wait is raced against the stop signal; a tick
                    // that has started always runs to completion, so an unlock
                    // is never half-recorded when the session ends.
                    tokio::select! {
                        _ = interval.tick() => {}
                        _ = stopped.changed() => break,
                    }
                    if let Some(w) = slot.lock().await.as_mut() {
                        w.tick(false).await;
                    }
                }
            }))
        };
        Self {
            task,
            stop,
            watcher: slot,
        }
    }

    /// Stops polling and reads every file once more, catching unlocks the
    /// game wrote as it exited. Waits for the baseline if it's still running.
    pub async fn finish(self) {
        let _ = self.stop.send(true);
        let _ = self.task.await;
        if let Some(w) = self.watcher.lock().await.as_mut() {
            w.tick(true).await;
        }
    }
}

impl Watcher {
    /// Builds the watcher and takes the baseline. `None` when the game has
    /// neither a known Steam app id nor an Epic emulator.
    async fn start(app: AppHandle, game_id: &str) -> Option<Self> {
        let library = app.state::<SharedLibrary>().inner().clone();
        let entry = library.find(game_id).await.ok().flatten()?;
        let ctx = GameContext::load(entry).await?;
        let config = load_config();

        let cached = {
            let ctx = ctx.clone();
            tokio::task::spawn_blocking(move || ctx.cached_schema())
                .await
                .ok()
                .flatten()
        };
        let schema_slot = Arc::new(std::sync::Mutex::new(cached));
        {
            let slot = schema_slot.clone();
            let http = http_client(&app);
            let (ctx, key) = (ctx.clone(), config.steam_web_api_key.clone());
            tokio::spawn(tracing::Instrument::in_current_span(async move {
                if let Some(s) = ctx.ensure_schema(&http, &key).await {
                    if let Ok(mut g) = slot.lock() {
                        *g = Some(s);
                    }
                }
            }));
        }

        let sources = {
            let ctx = ctx.clone();
            tokio::task::spawn_blocking(move || ctx.sources())
                .await
                .unwrap_or_default()
        };
        let mut watcher = Watcher {
            app,
            library,
            ctx,
            notify: config.achievement_notifications,
            started_ms: now_ms(),
            schema: schema_slot,
            files: Tracked::new(sources),
            seen: HashSet::new(),
            elsewhere: HashSet::new(),
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
    /// Set when another device recorded the unlock (folded in over the rclone
    /// control plane); `None` for this device's.
    pub unlocked_on: Option<String>,
    /// Unix ms when a Spool session on this device caught the unlock live;
    /// `None` when it was imported from an existing file or came from another
    /// device. Tells which session earned it: `unlocked_at` falls back to the
    /// import time for files that record no unlock times.
    pub live_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameAchievements {
    /// The Steam app id the files are keyed by, `None` when unknown.
    pub appid: Option<u64>,
    /// The Epic emulator's `AppName`, when the game ships one.
    pub epic_app: Option<String>,
    /// `"steam"`, `"local"`, `"epic"`, or `None` when no schema is available
    /// (the list then holds only unlocked achievements, by API name).
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
    // Pick up unlocks other devices published since the last fold (throttled).
    // A fold that changes anything bumps `achievements_version`, and the
    // change poll then reloads this card.
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            crate::rclone::fold_achievements_throttled(&app).await
        });
    }
    let (achievements, imported) =
        scan_game_achievements(&http_client(&app), &library, entry).await?;
    // Unlocks earned outside a Spool session are new to the other devices
    // too, so publish them rather than waiting for the next session.
    if imported {
        tauri::async_runtime::spawn(async move { crate::rclone::sync_achievements(&app).await });
    }
    Ok(achievements)
}

/// The body of [`get_game_achievements`] without the `AppHandle`, so the
/// headless plugin server can call it too. Also returns whether the scan
/// imported any unlocks, for the caller to publish them to other devices.
pub async fn scan_game_achievements(
    http: &reqwest::Client,
    library: &SharedLibrary,
    entry: GameEntry,
) -> AppResult<(GameAchievements, bool)> {
    let game_name = entry.game_name.clone();
    let Some(ctx) = GameContext::load(entry).await else {
        let rows = library.achievement_unlocks_for(&game_name).await?;
        return Ok((merge(None, None, None, rows), false));
    };
    let config = load_config();
    let schema = if ctx.epic_app.is_some() {
        background_epic_schema(http, library, &ctx, config.steam_web_api_key).await
    } else {
        ctx.ensure_schema(http, &config.steam_web_api_key).await
    };

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
    let imported = !library.insert_achievement_unlocks(&rows).await?.is_empty();

    let rows = library.achievement_unlocks_for(&ctx.game_name).await?;
    Ok((
        merge(ctx.appid, ctx.epic_app, schema.as_ref(), rows),
        imported,
    ))
}

/// The cached schema for a game with an Epic emulator, starting a fetch in
/// the background when it's missing or stale. Finding an Epic game's schema
/// can mean reading its exes and querying egdata for several ids, which the
/// card shouldn't wait on; when the fetch caches something new, the
/// `achievements_version` bump makes an open card reload.
async fn background_epic_schema(
    http: &reqwest::Client,
    library: &SharedLibrary,
    ctx: &GameContext,
    api_key: String,
) -> Option<Schema> {
    let cached = {
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || ctx.cached_schema())
            .await
            .ok()
            .flatten()
    };
    if cached.as_ref().is_some_and(schema::is_fresh) {
        return cached;
    }
    let (http, library, ctx, before) = (http.clone(), library.clone(), ctx.clone(), cached.clone());
    tokio::spawn(tracing::Instrument::in_current_span(async move {
        let fetched = ctx.ensure_schema(&http, &api_key).await;
        if fetched.is_some() && fetched != before {
            if let Err(e) = library.bump_achievements_version().await {
                tracing::warn!(error = %e, "achievements: couldn't signal the new schema");
            }
        }
    }));
    cached
}

fn merge(
    appid: Option<u64>,
    epic_app: Option<String>,
    schema: Option<&Schema>,
    rows: Vec<AchievementUnlock>,
) -> GameAchievements {
    // Every device's rows for one achievement (matched case-insensitively, as
    // devices without a schema keep the file's casing). The card shows the
    // first unlock on any device, and which device when it wasn't this one.
    let mut by_name: HashMap<String, Vec<&AchievementUnlock>> = HashMap::new();
    for r in &rows {
        by_name
            .entry(r.api_name.to_ascii_uppercase())
            .or_default()
            .push(r);
    }
    /// The earliest row (this device's on a tie), its time, and the other
    /// device's name when it isn't this device's.
    fn first<'a>(
        group: &[&'a AchievementUnlock],
    ) -> Option<(i64, Option<String>, &'a AchievementUnlock)> {
        let r = *group
            .iter()
            .min_by_key(|r| (r.unlocked_at, !r.device_id.is_empty()))?;
        let on = (!r.device_id.is_empty())
            .then(|| r.device_name.clone())
            .filter(|n| !n.is_empty());
        Some((r.unlocked_at, on, r))
    }
    /// The detection time when the shown row is this device's and was caught
    /// during a session.
    fn live_at(r: &AchievementUnlock) -> Option<i64> {
        (r.device_id.is_empty() && r.live == 1).then_some(r.detected_at)
    }
    let mut items = Vec::new();
    if let Some(s) = schema {
        for e in &s.achievements {
            let unlock = by_name
                .remove(&e.api_name.to_ascii_uppercase())
                .and_then(|g| first(&g).map(|(at, on, r)| (at, on, live_at(r))));
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
                unlocked_at: unlock.as_ref().map(|(at, _, _)| *at),
                live_at: unlock.as_ref().and_then(|(_, _, live)| *live),
                unlocked_on: unlock.and_then(|(_, on, _)| on),
            });
        }
    }
    // Unlocks the schema doesn't list (no schema, or renamed since).
    let mut extra: Vec<_> = by_name.into_values().filter_map(|g| first(&g)).collect();
    extra.sort_by_key(|(at, _, _)| *at);
    for (at, on, r) in extra {
        items.push(AchievementView {
            api_name: r.api_name.clone(),
            display_name: r.display_name.clone().unwrap_or_else(|| r.api_name.clone()),
            description: String::new(),
            hidden: false,
            icon: r.icon.clone(),
            icon_gray: None,
            global_percent: None,
            unlocked_at: Some(at),
            live_at: live_at(r),
            unlocked_on: on,
        });
    }
    GameAchievements {
        appid,
        epic_app,
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
            appid: Some(480),
            epic_app: None,
            exe_path: PathBuf::new(),
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
            ..Default::default()
        }
    }

    #[test]
    fn epic_exes_puts_the_ini_folder_first() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        std::fs::create_dir_all(&game).unwrap();
        for f in ["Game.exe", "b.exe", "unins000.exe", "notes.txt"] {
            std::fs::write(game.join(f), "").unwrap();
        }
        let launcher = dir.path().join("Launcher.exe");
        assert_eq!(
            epic_exes(&launcher, Some(&game)),
            [game.join("Game.exe"), game.join("b.exe"), launcher.clone()]
        );
        assert_eq!(
            epic_exes(&game.join("b.exe"), Some(&game)),
            [game.join("b.exe"), game.join("Game.exe")]
        );
        assert_eq!(epic_exes(&launcher, None), [launcher]);
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
            device_id: String::new(),
            device_name: String::new(),
        };
        let peer = |n: &str, t: i64| AchievementUnlock {
            device_id: "dev-b".into(),
            device_name: "Steam Deck".into(),
            ..row(n, t)
        };
        // ACH_B: this device at 5 and the Deck earlier at 4 under another
        // casing; OLD: only the Deck.
        let out = merge(
            Some(480),
            None,
            Some(&schema()),
            vec![row("ach_b", 5), peer("ACH_B", 4), peer("OLD", 3)],
        );
        assert_eq!(out.total, 3);
        assert_eq!(out.unlocked, 2);
        assert_eq!(out.items[0].unlocked_at, None);
        assert_eq!(
            out.items[1].unlocked_at,
            Some(4),
            "first unlock on any device"
        );
        assert_eq!(out.items[2].api_name, "OLD");
        assert_eq!(out.items[1].unlocked_on.as_deref(), Some("Steam Deck"));
        assert_eq!(out.items[2].unlocked_on.as_deref(), Some("Steam Deck"));
        // When this device has the earliest unlock, no device is shown.
        let out = merge(
            Some(480),
            None,
            Some(&schema()),
            vec![row("ACH_B", 2), peer("ACH_B", 4)],
        );
        assert_eq!(out.items[1].unlocked_at, Some(2));
        assert_eq!(out.items[1].unlocked_on, None);
    }

    #[test]
    fn merge_reports_live_detection_for_this_devices_rows() {
        let row = |n: &str, live: i64, device: &str| AchievementUnlock {
            game_name: "Game".into(),
            api_name: n.into(),
            unlocked_at: 100,
            detected_at: 5_000,
            live,
            display_name: None,
            icon: None,
            device_id: device.into(),
            device_name: if device.is_empty() { "" } else { "Steam Deck" }.into(),
        };
        let out = merge(
            None,
            None,
            None,
            vec![
                row("LIVE", 1, ""),
                row("IMPORTED", 0, ""),
                row("PEER", 1, "dev-b"),
            ],
        );
        let get = |n: &str| out.items.iter().find(|i| i.api_name == n).unwrap().live_at;
        assert_eq!(get("LIVE"), Some(5_000));
        assert_eq!(get("IMPORTED"), None);
        assert_eq!(get("PEER"), None);
    }

    #[tokio::test]
    async fn live_record_upgrades_only_imports_from_this_session() {
        let library = crate::library::Library::open_in_memory().await.unwrap();
        let row = |name: &str, detected_at: i64| AchievementUnlock {
            game_name: "Game".into(),
            api_name: name.into(),
            unlocked_at: 1_700_000_000,
            detected_at,
            live: 0,
            display_name: None,
            icon: None,
            device_id: String::new(),
            device_name: String::new(),
        };
        let start = 1_000;
        // Imported before the session (a previous detail-pane scan) and during it.
        library
            .insert_achievement_unlocks(&[row("OLD", 500), row("MID", 1_500)])
            .await
            .unwrap();

        let added = library
            .record_live_unlocks(
                &[row("OLD", 2_000), row("MID", 2_000), row("NEW", 2_000)],
                start,
            )
            .await
            .unwrap();
        let mut names: Vec<_> = added.iter().map(|r| r.api_name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["MID", "NEW"]);

        // Already live: recording again announces nothing.
        assert!(library
            .record_live_unlocks(&[row("MID", 3_000), row("NEW", 3_000)], start)
            .await
            .unwrap()
            .is_empty());
        let live = library.live_achievement_unlocks_since(0).await.unwrap();
        assert_eq!(live.len(), 2);
    }

    #[tokio::test]
    async fn fold_keeps_devices_apart_and_never_goes_live() {
        let library = crate::library::Library::open_in_memory().await.unwrap();
        let row =
            |name: &str, unlocked_at: i64, device: &str, device_name: &str| AchievementUnlock {
                game_name: "Game".into(),
                api_name: name.into(),
                unlocked_at,
                detected_at: 10,
                live: 0,
                display_name: None,
                icon: None,
                device_id: device.into(),
                device_name: device_name.into(),
            };
        // This device: A imported, B live.
        library
            .insert_achievement_unlocks(&[row("A", 500, "", "")])
            .await
            .unwrap();
        library
            .record_live_unlocks(&[row("B", 900, "", "")], 0)
            .await
            .unwrap();

        // The Deck's blob has A (earlier) and C; our own blob (dev-a) has B and
        // D, where D was lost from the local DB.
        let fold = [
            row("A", 100, "dev-b", "Steam Deck"),
            row("C", 300, "dev-b", "Steam Deck"),
            row("B", 900, "dev-a", "Desk PC"),
            row("D", 50, "dev-a", "Desk PC"),
        ];
        assert_eq!(
            library
                .fold_achievement_unlocks(&fold, "dev-a")
                .await
                .unwrap(),
            3
        );
        let rows = library.achievement_unlocks_for("Game").await.unwrap();
        let get = |n: &str, d: &str| {
            rows.iter()
                .find(|r| r.api_name == n && r.device_id == d)
                .cloned()
        };
        // This device's A keeps its own time; the Deck's A is a separate row.
        assert_eq!(get("A", "").unwrap().unlocked_at, 500);
        assert_eq!(get("A", "dev-b").unwrap().unlocked_at, 100);
        assert!(get("C", "dev-b").is_some());
        // Our own blob restores D as a local row and doesn't duplicate B.
        assert!(get("D", "").is_some());
        assert_eq!(rows.iter().filter(|r| r.api_name == "B").count(), 1);
        // Folded rows never reach the Decky toast feed.
        let live = library.live_achievement_unlocks_since(0).await.unwrap();
        assert_eq!(
            live.iter().map(|r| r.api_name.as_str()).collect::<Vec<_>>(),
            ["B"]
        );

        // The projection publishes only this device's rows.
        let mut mine: Vec<_> = library
            .local_achievement_unlocks()
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.api_name)
            .collect();
        mine.sort();
        assert_eq!(mine, ["A", "B", "D"]);

        // Folding the same blobs again changes nothing; a renamed device's
        // rows pick up the new name.
        let before = library.achievements_version().await.unwrap();
        assert_eq!(
            library
                .fold_achievement_unlocks(&fold, "dev-a")
                .await
                .unwrap(),
            0
        );
        assert_eq!(library.achievements_version().await.unwrap(), before);
        assert_eq!(
            library
                .fold_achievement_unlocks(&[row("C", 300, "dev-b", "Deck OLED")], "dev-a")
                .await
                .unwrap(),
            1
        );
        assert!(library.achievements_version().await.unwrap() > before);
        let rows = library.achievement_unlocks_for("Game").await.unwrap();
        assert_eq!(
            rows.iter().find(|r| r.api_name == "C").unwrap().device_name,
            "Deck OLED"
        );
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

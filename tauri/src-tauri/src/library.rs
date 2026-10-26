//! Persistent game library — the heart of the app.
//!
//! Stored in a SQLite database (`library.db`) accessed through [`sqlx`]. SQLite
//! is used instead of a single JSON document because several Spool processes
//! write the library concurrently — the tray GUI, the per-launch attached
//! `spool --run` instance, and the headless Decky server. Whole-document JSON
//! rewrites made those processes clobber each other (last-writer-wins lost
//! updates); SQLite in WAL mode serialises writers and lets each write touch
//! only the fields it owns.
//!
//! Each game is one row: `id`, `catalog_number`, `game_name`, and a `data`
//! column holding the whole [`GameEntry`] as JSON. Reads deserialise `data`,
//! so adding a `GameEntry` field needs no schema migration — old rows still
//! parse via `serde(default)`, exactly as the old `library.json` did. Targeted
//! writes use SQLite's `json_set()` to update individual fields atomically, so
//! a playtime bump in one process can't overwrite a name edit in another.
//!
//! On first run the legacy `library.json` is imported once (then renamed to
//! `library.json.migrated` as a backup) — see [`Library::open`].

use crate::error::{AppError, AppResult};
use crate::paths;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// A user-defined save location for a game ludusavi's manifest doesn't cover
/// (or covers wrongly). Projected into a ludusavi `customGames` entry by
/// [`crate::custom_saves`] so backup/restore treat it like any manifest game.
///
/// `files` hold ludusavi path templates — placeholder tokens like
/// `<winLocalAppData>/MyGame` (portable: they resolve to `%LOCALAPPDATA%` on
/// Windows and into the Proton prefix's `drive_c` under Wine), or `<base>/Saves`
/// (relative to the game's install folder), or a literal absolute path. The same
/// portable definition is replicated to every device via the rclone control
/// plane so the user only picks the folder once. `registry` holds Windows
/// registry keys (rarely used; inert under Proton).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct CustomSave {
    pub files: Vec<String>,
    pub registry: Vec<String>,
}

/// A user's narrowing of which manifest-derived save locations actually sync for
/// a manifest-covered game — the lever for "back up my saves, not my settings".
///
/// Stored as *exclusions of intent*, never as resolved paths, so it stays
/// correct across machines: a game can be native on one OS and a Windows build
/// (under Proton) on another, with different manifest paths per OS. Each device
/// re-derives its own ludusavi `customGames` override from *its* manifest minus
/// these exclusions ([`crate::custom_saves::sync_ludusavi_custom_games`]).
///
/// * `excluded_tags` — manifest file tags to drop (e.g. `"config"`). Semantic and
///   device-independent, so this is the cross-OS-correct primary mechanism.
/// * `excluded_paths` — literal manifest templates to drop, for per-path control.
///   Template strings are device-specific, so an entry only matches on a device
///   whose manifest carries that exact template (it degrades to a no-op
///   elsewhere) — acceptable, since tags carry the intent across OSes.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct ManifestOverride {
    pub excluded_tags: Vec<String>,
    pub excluded_paths: Vec<String>,
}

impl ManifestOverride {
    /// Whether this override actually excludes anything. An empty override is
    /// equivalent to none — the plain manifest entry stands, no `customGames`
    /// override is written.
    pub fn is_active(&self) -> bool {
        !self.excluded_tags.is_empty() || !self.excluded_paths.is_empty()
    }
}

/// One game in the library. Matches the C# `GameEntry` JSON shape exactly
/// for the legacy fields, plus a small set of manifest-derived metadata
/// new to the Tauri rewrite (steam id, gog id, save paths, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GameEntry {
    pub id: String,
    /// Sequential shelf catalog number, formatted as `SPL-NNNN` in the UI.
    /// Assigned at add-time and stable for the entry's lifetime; deleting a
    /// game leaves a gap rather than reusing the number. Zero means "not yet
    /// assigned" — assigned by `Library::insert`, and backfilled for legacy
    /// entries during the one-time `library.json` import.
    pub catalog_number: u32,
    pub game_name: String,
    pub exe_path: String,
    pub safe_name: String,

    pub cover_image_path: Option<String>,
    pub hero_image_path: Option<String>,

    pub added_at: Option<DateTime<Utc>>,
    pub last_played_at: Option<DateTime<Utc>>,

    pub launcher_exe_path: Option<String>,
    pub game_folder_path: Option<String>,

    /// Whether the game's files are currently on disk. `true` for a normal
    /// added/installed entry; flipped to `false` by `uninstall_game` (remove
    /// from disk but keep the catalogue entry — playtime, art, save backups
    /// survive). An uninstalled entry renders dimmed with its Play button
    /// disabled until it's re-added (which reuses this same row). Defaults to
    /// `true` so legacy rows missing the field load as installed; written
    /// out-of-band by uninstall/reinstall, so it's in [`RUNTIME_FIELDS`] to
    /// survive a whole-entry editor save.
    pub installed: bool,

    pub run_as_admin: bool,

    // ── Proton / Linux launch (inert on Windows) ────────────────────────────
    /// Legacy on/off toggle for launching through Proton. **No longer read** —
    /// Proton is now used automatically for Windows `.exe` games on Linux (see
    /// [`GameEntry::uses_proton`] / issue #80). Kept only so existing
    /// `library.json` files round-trip unchanged; nothing sets or consults it.
    pub use_proton: bool,
    /// Override the Proton build directory. `None` = use the global default
    /// (`ConfigData.launch.default_proton_path`) or auto-pick the newest.
    pub proton_version_path: Option<String>,
    /// Override the Wine prefix ROOT. `None` = `paths::proton_prefixes_dir()/<id>`.
    pub wine_prefix_path: Option<String>,
    /// Extra command-line args appended after the exe (space-split at use).
    pub launch_args: Option<String>,

    // Metadata
    pub description: String,
    pub developer: String,
    pub publisher: String,
    pub genres: Vec<String>,
    pub release_date: Option<DateTime<Utc>>,
    /// True once a Steam Store metadata fetch has been ATTEMPTED for this entry
    /// — i.e. Steam responded, even if it returned no developer/publisher/etc.
    /// Stops the startup backfill from re-requesting forever for titles Steam
    /// has only partial (or no) data on. A network *failure* leaves it false, so
    /// the next boot retries. Written out-of-band by the metadata enrich/backfill
    /// (never the editor save), so it is in [`RUNTIME_FIELDS`].
    pub metadata_fetched: bool,
    pub install_size_mb: f64,

    // Play tracking
    pub playtime_minutes: i32,

    // LAN sharing
    pub lan_shared: bool,
    pub lan_share_folder: Option<String>,

    // Save backup stats (updated by the run workflow)
    pub save_backup_count: i32,
    pub save_last_backed_up_at: Option<DateTime<Utc>>,
    pub save_backup_size_mb: f64,

    pub install_source: String,
    pub lan_install_source_device_name: Option<String>,
    pub lan_install_source_device_id: Option<String>,

    // ── Manifest-derived metadata (new in Tauri rewrite) ────────────────────
    //
    // Snapshot of the ludusavi manifest entry that matched this game at
    // add-time. Stays stable for the entry's lifetime; the user can re-run
    // identification to refresh it. Empty/None when the user added a game
    // without save tracking (no ludusavi match).
    pub steam_id: Option<u64>,
    pub gog_id: Option<u64>,
    pub lutris_slug: Option<String>,
    /// The folder name ludusavi expects, e.g. `"Hades"`. Useful for hinting
    /// at the install dir when the user picks an exe.
    pub manifest_install_dir: Option<String>,
    /// Save path templates from the manifest, in display form (e.g.
    /// `%APPDATA%/Hades`). First entry is the canonical / primary location.
    pub save_paths: Vec<String>,

    /// User-defined save location for a non-manifest game (or a manual override
    /// of a wrong manifest entry). `None` = track via the manifest as usual /
    /// not tracked. When set, [`crate::custom_saves`] writes a ludusavi
    /// `customGames` entry so the run workflow backs up and restores it like any
    /// other game. Written out-of-band by the Saves editor and the cross-device
    /// adopt fold, so it's listed in [`RUNTIME_FIELDS`] and survives a
    /// whole-entry editor save.
    pub custom_save: Option<CustomSave>,

    /// User's narrowing of which manifest save locations sync for this game (e.g.
    /// "exclude settings/config"). `None`/empty = the full manifest entry applies.
    /// When active, [`crate::custom_saves`] re-derives a ludusavi `customGames`
    /// override from this device's manifest minus the exclusions. Written
    /// out-of-band by the Saves editor and the cross-device adopt fold, so it's in
    /// [`RUNTIME_FIELDS`] and survives a whole-entry editor save.
    pub manifest_override: Option<ManifestOverride>,
    /// Dominant cover-art colour as `#rrggbb`, extracted when the cover
    /// downloads. Drives hero / button / accent tinting in the detail
    /// view; falls back to the brand `spool` colour when None.
    pub accent_color: Option<String>,

    /// Cross-device save-sync status, derived from the rclone device blobs
    /// at startup and updated after each backup. One of:
    ///
    ///   "synced"        this device holds the most recent backup
    ///   "local-newer"   we backed up locally but the cloud hasn't
    ///                   confirmed it yet (offline / sync disabled)
    ///   "cloud-newer"   another device backed up after us — our
    ///                   local saves are behind
    ///
    /// `None` means not enough info to badge (cloud not configured,
    /// no backup history). The library sidebar renders a small
    /// coloured dot on the cover when this is set.
    pub sync_badge: Option<String>,

    /// Latest ludusavi backup name (the `mapping.yaml` tip) that was last
    /// reconciled with the cloud on THIS device. Acts as the merge-base for
    /// fast-forward vs. true-divergence detection: when ludusavi reports a
    /// cloud conflict, comparing this baseline against the local and cloud
    /// tips tells us whether one side is cleanly ahead (auto-resolve) or both
    /// changed (real conflict — prompt the user). `None` for legacy entries /
    /// games never synced — the workflow falls back to a timestamp heuristic
    /// until the first sync records a baseline.
    pub cloud_sync_baseline: Option<String>,

    /// Display name of the device that holds the most recent cloud backup, when
    /// that's a device *other* than this one (i.e. paired with a `cloud-newer`
    /// `sync_badge`). Folded from the rclone device blobs at startup. `None`
    /// when this device is the latest backer or cloud sync isn't configured.
    pub save_last_backer_device: Option<String>,
    /// Timestamp of that newer cloud backup (the latest backer's upload time).
    /// Pairs with `save_last_backer_device` to render "Desktop-PC · 2h ago" on
    /// the `cloud-newer` state. `None` when we're the latest backer / no sync.
    pub save_cloud_revision_at: Option<DateTime<Utc>>,

    /// The Steam non-Steam-shortcut appid last written to `shortcuts.vdf` for
    /// this game (`Some` once "Add to Steam" has run, `None` otherwise). The
    /// appid is name-derived (`steam::calculate_app_id` over the spool exe +
    /// game name), so a rename changes it — storing the previously-written value
    /// lets the edit/remove paths find and reconcile the *exact* old shortcut +
    /// collection entry instead of orphaning it. Written out-of-band by the
    /// Steam integration (`add_to_steam` / rename reconcile), not the editor, so
    /// it's in [`RUNTIME_FIELDS`] and survives a whole-entry editor save.
    pub steam_app_id: Option<u32>,

    /// Where the user is with the game: one of [`PLAY_STATUSES`], or `None` for
    /// no status. Set only by the user (`set_play_status`), never by a launch.
    /// A string rather than an enum so a status synced from a newer Spool
    /// doesn't stop this row from parsing; the UI ignores values it doesn't know.
    ///
    /// Not stored on the row: it lives in the `play_statuses` table, keyed by
    /// game name, and reads go through the `games_with_status` view, which lays
    /// it over `data`. Whatever a whole-entry write puts in these two fields is
    /// ignored on the next read.
    pub play_status: Option<String>,
    /// When `play_status` was last changed, as Unix milliseconds (see
    /// [`PlayStatusRow::updated_at`]).
    pub play_status_updated_at: Option<i64>,
}

impl Default for GameEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            catalog_number: 0,
            game_name: String::new(),
            exe_path: String::new(),
            safe_name: String::new(),
            cover_image_path: None,
            hero_image_path: None,
            added_at: None,
            last_played_at: None,
            launcher_exe_path: None,
            game_folder_path: None,
            installed: true,
            run_as_admin: false,
            use_proton: false,
            proton_version_path: None,
            wine_prefix_path: None,
            launch_args: None,
            description: String::new(),
            developer: String::new(),
            publisher: String::new(),
            genres: Vec::new(),
            release_date: None,
            metadata_fetched: false,
            install_size_mb: 0.0,
            playtime_minutes: 0,
            lan_shared: false,
            lan_share_folder: None,
            save_backup_count: 0,
            save_last_backed_up_at: None,
            save_backup_size_mb: 0.0,
            install_source: "manual".to_string(),
            lan_install_source_device_name: None,
            lan_install_source_device_id: None,
            steam_id: None,
            gog_id: None,
            lutris_slug: None,
            manifest_install_dir: None,
            save_paths: Vec::new(),
            custom_save: None,
            manifest_override: None,
            accent_color: None,
            sync_badge: None,
            cloud_sync_baseline: None,
            save_last_backer_device: None,
            save_cloud_revision_at: None,
            steam_app_id: None,
            play_status: None,
            play_status_updated_at: None,
        }
    }
}

impl GameEntry {
    /// Whether this entry launches through Proton. Derived from the platform +
    /// executable type rather than a stored flag — see
    /// [`crate::proton::exe_needs_proton`] (issue #80).
    pub fn uses_proton(&self) -> bool {
        crate::proton::exe_needs_proton(&self.exe_path)
    }
}

/// Payload accepted by the `add_game` command. The frontend constructs this
/// from a picked `SearchCandidate` plus the user-chosen exe path. Empty
/// ludusavi-derived fields represent the "add without save tracking" path.
#[derive(Debug, Deserialize)]
pub struct NewGame {
    pub game_name: String,
    pub exe_path: String,
    #[serde(default)]
    pub steam_id: Option<u64>,
    #[serde(default)]
    pub gog_id: Option<u64>,
    #[serde(default)]
    pub lutris_slug: Option<String>,
    #[serde(default)]
    pub manifest_install_dir: Option<String>,
    #[serde(default)]
    pub save_paths: Vec<String>,
    /// Install folder on disk (defaults to the exe's parent directory in the
    /// Add flow, confirmed by the user). Required for LAN sharing to have
    /// something to stream — see `PeerGame::from_entry` in `lan/mod.rs`.
    #[serde(default)]
    pub game_folder_path: Option<String>,
    /// Override for the Wine prefix ROOT (Linux). Set by the guided
    /// Windows-installer flow (`guided_install.rs`) so the game launches in the
    /// very prefix it was installed into — keeping any vcredist/dotnet/registry
    /// state the installer set up. `None` for the normal Add flow (the runner then uses
    /// the default `prefixes/<id>`).
    #[serde(default)]
    pub wine_prefix_path: Option<String>,
    /// Proton build dir used during install. Pinned so the game always launches
    /// with the same Proton version the prefix was created with.
    #[serde(default)]
    pub proton_version_path: Option<String>,
    /// Optional custom save location set at add-time (e.g. adopted from a
    /// cross-device definition for the same game name). `None` for the normal
    /// "identify via manifest" / "without save tracking" paths.
    #[serde(default)]
    pub custom_save: Option<CustomSave>,
    /// When set, re-add (reinstall) this exact existing entry rather than
    /// creating a new one — passed by the "Reinstall…" affordance, which knows
    /// the uninstalled entry's id. Ignored (falls back to a steam-id / name
    /// match, then a fresh insert) if the id is missing or no longer refers to
    /// an uninstalled entry. See [`add_game`].
    #[serde(default)]
    pub reinstall_target_id: Option<String>,
    /// When true, the Add flow may relocate the selected install folder into
    /// the default library folder before registering the entry. If the folder is
    /// already inside a configured library folder, this is a no-op.
    #[serde(default)]
    pub import_to_library: bool,
}

/// JSON paths (under `$.`) of the fields that are owned by the running
/// workflow / background tasks rather than the game editor: playtime, backup
/// stats, sync badges, and system-derived art/size. A whole-entry [`replace`]
/// (the editor's "save") must NOT overwrite these, because another process
/// (an attached `--run` launch bumping playtime, say) may be writing them at
/// the same time — clobbering them is the exact multi-process lost update the
/// SQLite move exists to prevent. [`Library::replace`] re-overlays these from
/// the existing row so only the editor-owned fields change.
///
/// [`replace`]: Library::replace
const RUNTIME_FIELDS: &[&str] = &[
    "last_played_at",
    "playtime_minutes",
    "save_backup_count",
    "save_last_backed_up_at",
    "save_backup_size_mb",
    "sync_badge",
    "cloud_sync_baseline",
    "save_last_backer_device",
    "save_cloud_revision_at",
    "accent_color",
    "install_size_mb",
    "cover_image_path",
    "hero_image_path",
    // Written out-of-band by the Saves editor / cross-device adopt fold, not the
    // whole-entry editor save, so the editor's `replace` must re-overlay it.
    "custom_save",
    "manifest_override",
    // Resolved out-of-band by the add-game enrich task (Steam store search →
    // steam id) for untracked adds, not by the whole-entry editor save, which
    // has no steam-id control. Overlaying it stops a stale open editor (loaded
    // before the async resolve landed) from clobbering the resolved id back to
    // null on save — which would lose the Steam-CDN art + Steam-Store metadata.
    "steam_id",
    // Flipped by the metadata enrich/backfill once Steam has been queried, not
    // the editor save (which has no such control). Overlaying it stops a stale
    // editor save from resetting it to false and re-triggering the backfill.
    "metadata_fetched",
    // Flipped out-of-band by uninstall / reinstall (and the Decky plugin), not
    // the editor save. Overlaying it stops a stale open editor from resurrecting
    // an uninstalled game on save. (`game_folder_path` / `exe_path` are NOT
    // overlaid — the editor legitimately edits those; `installed` being the
    // launch/UI source of truth makes any stale path harmless.)
    "installed",
    // Written out-of-band by the Steam integration (add_to_steam / the rename
    // reconcile in update_game), not the editor save, which has no Steam-appid
    // control. Overlaying it stops a stale open editor from clobbering a freshly
    // recorded appid back to null on save — which would lose the link the
    // edit/remove paths use to find and clean up the game's Steam shortcut.
    "steam_app_id",
];

/// The play statuses a game can have (`GameEntry::play_status`).
pub const PLAY_STATUSES: &[&str] = &["playing", "to-play", "finished", "dropped"];

pub fn is_valid_play_status(status: &str) -> bool {
    PLAY_STATUSES.contains(&status)
}

/// One row of the `play_statuses` table, and one entry of a device's
/// `_spool/play-status/` blob (`rclone.rs`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlayStatusRow {
    pub game_name: String,
    /// `None` once the user cleared it. The row stays, so the clear syncs.
    pub status: Option<String>,
    /// Unix ms of the change. The fold keeps the largest per game name.
    pub updated_at: i64,
}

/// Sentinel `device_id` for the synthetic legacy session that
/// [`Library::migrate_legacy_playtime`] seeds for pre-sessions playtime. Not a
/// real device, so `sessions_for_device` never folds it into a history blob.
const LEGACY_DEVICE_ID: &str = "legacy-import";

/// Timestamp stamped on a legacy session when the game has no recorded
/// `last_played_at` to anchor it: a fixed early date so it sorts before real
/// sessions in the timeline rather than landing at the Unix epoch.
fn legacy_baseline_ts() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
        .expect("valid literal")
        .with_timezone(&Utc)
}

/// One finished play session — an immutable record of a single launch on a
/// single device. Sessions are append-only facts: each is created exactly once,
/// by exactly one device, and never edited afterwards. That makes the
/// cross-device store conflict-free (a union of per-device rows keyed by
/// `session_id`), so syncing them needs no database merge — see
/// `rclone::sync_play_history`. The `play_sessions` table is the source of
/// truth; the per-device rclone history blob is just a projection of it.
///
/// `#[serde(default)]` at the container level keeps older blobs loadable when a
/// field is added later — the same JSON-shape rule the rest of the persisted
/// state follows, important because rows round-trip across devices that may run
/// different Spool versions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaySession {
    /// Globally unique across devices: `<device_id>:<started_at_millis>`.
    pub session_id: String,
    pub device_id: String,
    pub device_name: String,
    /// Match key shared with the rest of the control plane (markers, blobs).
    pub game_name: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    /// Wall-clock seconds played, with any mid-session suspend time subtracted.
    pub duration_secs: i64,
}

/// Deserialise a `play_sessions` row into a [`PlaySession`]. Timestamps are
/// stored as RFC 3339 text; a row that can't parse is a corrupt write we'd
/// rather surface than silently drop, so this returns a result.
fn row_to_session(row: &sqlx::sqlite::SqliteRow) -> AppResult<PlaySession> {
    let parse = |s: String| {
        DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| AppError::Other(format!("bad session timestamp {s:?}: {e}")))
    };
    Ok(PlaySession {
        session_id: row.get("session_id"),
        device_id: row.get("device_id"),
        device_name: row.get("device_name"),
        game_name: row.get("game_name"),
        started_at: parse(row.get("started_at"))?,
        ended_at: parse(row.get("ended_at"))?,
        duration_secs: row.get("duration_secs"),
    })
}

/// `CREATE TABLE` for `achievement_unlocks` under a given name (the migration
/// builds a copy first); see `Library::init_achievement_tables`.
macro_rules! achievement_unlocks_table {
    ($name:literal) => {
        concat!(
            "CREATE TABLE IF NOT EXISTS ",
            $name,
            " (
                 game_name    TEXT NOT NULL,
                 api_name     TEXT NOT NULL,
                 device_id    TEXT NOT NULL DEFAULT '',
                 device_name  TEXT NOT NULL DEFAULT '',
                 unlocked_at  INTEGER NOT NULL,
                 detected_at  INTEGER NOT NULL,
                 live         INTEGER NOT NULL,
                 display_name TEXT,
                 icon         TEXT,
                 PRIMARY KEY (game_name, api_name, device_id)
             )"
        )
    };
}

/// Whether `achievement_unlocks.device_id` is part of the primary key, i.e. the
/// table already has the per-device shape.
const DEVICE_IN_KEY: &str =
    "SELECT COUNT(*) FROM pragma_table_info('achievement_unlocks') WHERE name = 'device_id' AND pk > 0";

/// One row of the `achievement_unlocks` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AchievementUnlock {
    pub game_name: String,
    pub api_name: String,
    /// Unix seconds.
    pub unlocked_at: i64,
    /// Unix milliseconds.
    pub detected_at: i64,
    /// 1 = caught during a play session, 0 = imported from an existing file.
    pub live: i64,
    pub display_name: Option<String>,
    pub icon: Option<String>,
    /// The other device that recorded it; empty for this device's own rows.
    pub device_id: String,
    pub device_name: String,
}

fn row_to_unlock(row: &sqlx::sqlite::SqliteRow) -> AchievementUnlock {
    AchievementUnlock {
        game_name: row.get("game_name"),
        api_name: row.get("api_name"),
        unlocked_at: row.get("unlocked_at"),
        detected_at: row.get("detected_at"),
        live: row.get("live"),
        display_name: row.get("display_name"),
        icon: row.get("icon"),
        device_id: row.get("device_id"),
        device_name: row.get("device_name"),
    }
}

/// Returns `items` with duplicates removed, preserving first-seen order.
fn dedup_preserve_order(items: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .iter()
        .filter(|s| seen.insert((*s).clone()))
        .cloned()
        .collect()
}

/// The game library, backed by a SQLite connection pool. Cheap to clone the
/// pool (it's an `Arc` internally); the whole `Library` is wrapped in an
/// [`Arc`] as [`SharedLibrary`] so spawned tasks can hold a handle.
#[derive(Clone)]
pub struct Library {
    pool: SqlitePool,
}

impl Library {
    /// Opens (creating if absent) the library database at [`paths::library_db`],
    /// sets up the schema, and — on a fresh DB — imports the legacy
    /// `library.json` once, renaming it to `library.json.migrated` afterward.
    pub async fn open() -> AppResult<Self> {
        let path = paths::library_db();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            // WAL lets concurrent Spool processes read while one writes, and a
            // busy_timeout makes a writer wait for the lock instead of erroring.
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        let lib = Self { pool };
        lib.init_schema().await?;
        lib.import_json_if_needed().await?;
        lib.migrate_legacy_playtime().await?;
        Ok(lib)
    }

    /// In-memory database — used as a graceful fallback when the on-disk DB
    /// can't be opened, and by unit tests. A single connection so all queries
    /// see the same memory DB.
    pub async fn open_in_memory() -> AppResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        let lib = Self { pool };
        lib.init_schema().await?;
        Ok(lib)
    }

    /// Creates the `games` + `meta` tables and the triggers that bump
    /// `meta.version` on every games mutation. Idempotent (`IF NOT EXISTS`).
    async fn init_schema(&self) -> AppResult<()> {
        // `data` holds the full GameEntry JSON; `id`/`catalog_number`/`game_name`
        // are mirrored as columns for cheap lookups and ordering.
        let stmts = [
            "CREATE TABLE IF NOT EXISTS games (
                 id TEXT PRIMARY KEY NOT NULL,
                 catalog_number INTEGER NOT NULL,
                 game_name TEXT NOT NULL,
                 data TEXT NOT NULL
             )",
            "CREATE TABLE IF NOT EXISTS meta (
                 k TEXT PRIMARY KEY NOT NULL,
                 v INTEGER NOT NULL
             )",
            "INSERT OR IGNORE INTO meta (k, v) VALUES ('version', 0)",
            // `meta.version` is the cross-process change signal: the GUI polls
            // it to notice writes made by other Spool processes (Tauri events
            // don't cross process boundaries). Triggers bump it on any change.
            "CREATE TRIGGER IF NOT EXISTS games_version_ai AFTER INSERT ON games
             BEGIN UPDATE meta SET v = v + 1 WHERE k = 'version'; END",
            "CREATE TRIGGER IF NOT EXISTS games_version_au AFTER UPDATE ON games
             BEGIN UPDATE meta SET v = v + 1 WHERE k = 'version'; END",
            "CREATE TRIGGER IF NOT EXISTS games_version_ad AFTER DELETE ON games
             BEGIN UPDATE meta SET v = v + 1 WHERE k = 'version'; END",
            // Append-only log of finished play sessions, one row per launch.
            // Keyed by a globally-unique `session_id` so re-folding a peer's
            // history (INSERT OR IGNORE) is idempotent. Deliberately *not* wired
            // to `meta.version`: a session insert / cross-device fold shouldn't
            // trigger a full library reload in every Spool process.
            "CREATE TABLE IF NOT EXISTS play_sessions (
                 session_id  TEXT PRIMARY KEY NOT NULL,
                 device_id   TEXT NOT NULL,
                 device_name TEXT NOT NULL,
                 game_name   TEXT NOT NULL,
                 started_at  TEXT NOT NULL,
                 ended_at    TEXT NOT NULL,
                 duration_secs INTEGER NOT NULL
             )",
            "CREATE INDEX IF NOT EXISTS play_sessions_game ON play_sessions (game_name)",
            "CREATE INDEX IF NOT EXISTS play_sessions_device ON play_sessions (device_id)",
            // Each game's play status, keyed by name like `play_sessions` so it
            // outlives the game's row (forget + re-add keeps it) and holds the
            // statuses other devices have for games not in this library, which
            // this device's blob republishes. A cleared status keeps its row
            // with `status` NULL. Wired to `meta.version` because the statuses
            // show on the library rows.
            "CREATE TABLE IF NOT EXISTS play_statuses (
                 game_name  TEXT PRIMARY KEY NOT NULL,
                 status     TEXT,
                 updated_at INTEGER NOT NULL
             )",
            "CREATE TRIGGER IF NOT EXISTS play_statuses_version_ai AFTER INSERT ON play_statuses
             BEGIN UPDATE meta SET v = v + 1 WHERE k = 'version'; END",
            "CREATE TRIGGER IF NOT EXISTS play_statuses_version_au AFTER UPDATE ON play_statuses
             BEGIN UPDATE meta SET v = v + 1 WHERE k = 'version'; END",
            "CREATE TRIGGER IF NOT EXISTS play_statuses_version_ad AFTER DELETE ON play_statuses
             BEGIN UPDATE meta SET v = v + 1 WHERE k = 'version'; END",
            // `games` with each row's play status laid over its `data`. Every
            // read of a full `GameEntry` goes through this.
            "CREATE VIEW IF NOT EXISTS games_with_status AS
             SELECT g.id, g.catalog_number, g.game_name,
                    json_set(g.data, '$.play_status', ps.status,
                             '$.play_status_updated_at', ps.updated_at) AS data
             FROM games g LEFT JOIN play_statuses ps ON ps.game_name = g.game_name",
        ];
        for sql in stmts {
            sqlx::query(sql).execute(&self.pool).await?;
        }
        self.init_achievement_tables().await?;
        Ok(())
    }

    /// The `achievement_unlocks` table: one row per achievement per device
    /// that recorded it, read from the game's Steam-emulator stats file
    /// (`achievements.rs`) or folded in from another device's blob
    /// (`rclone.rs`). Keyed by game name like `play_sessions`.
    ///
    /// * `device_id` is empty for this device's own rows, else the recording
    ///   device's id (`device_name` is its display name, refreshed by each
    ///   fold). Rows from other devices mirror their blob and are never
    ///   published from here.
    /// * `unlocked_at` is the emulator's unix time (or detection time when it
    ///   records none); `detected_at` is unix ms when this device stored the row.
    /// * `live` is 1 for unlocks caught during a play session (they drive the
    ///   Decky in-game toast), 0 for imports from an existing file and folded rows.
    ///
    /// Not wired to `meta.version` (a fold shouldn't reload the whole library);
    /// triggers bump a separate `achievements_version` counter instead, which
    /// the GUI polls to refresh after another process writes.
    async fn init_achievement_tables(&self) -> AppResult<()> {
        sqlx::query(achievement_unlocks_table!("achievement_unlocks"))
            .execute(&self.pool)
            .await?;
        self.migrate_achievement_key().await?;
        let stmts = [
            "CREATE INDEX IF NOT EXISTS achievement_unlocks_detected ON achievement_unlocks (detected_at)",
            "INSERT OR IGNORE INTO meta (k, v) VALUES ('achievements_version', 0)",
            "CREATE TRIGGER IF NOT EXISTS achievement_unlocks_version_ai AFTER INSERT ON achievement_unlocks
             BEGIN UPDATE meta SET v = v + 1 WHERE k = 'achievements_version'; END",
            "CREATE TRIGGER IF NOT EXISTS achievement_unlocks_version_au AFTER UPDATE ON achievement_unlocks
             BEGIN UPDATE meta SET v = v + 1 WHERE k = 'achievements_version'; END",
        ];
        for sql in stmts {
            sqlx::query(sql).execute(&self.pool).await?;
        }
        Ok(())
    }

    /// Rebuilds an `achievement_unlocks` table created before cross-device sync
    /// (keyed by game + achievement, no device columns) into the per-device
    /// shape. Its rows are all this device's, so they get an empty
    /// `device_id`. SQLite can't change a primary key in place, hence the copy.
    /// Runs under `BEGIN IMMEDIATE` and re-checks inside it, so two Spool
    /// processes starting at once don't both rebuild.
    async fn migrate_achievement_key(&self) -> AppResult<()> {
        let keyed: i64 = sqlx::query_scalar(DEVICE_IN_KEY)
            .fetch_one(&self.pool)
            .await?;
        if keyed > 0 {
            return Ok(());
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let keyed: i64 = sqlx::query_scalar(DEVICE_IN_KEY)
            .fetch_one(&mut *tx)
            .await?;
        if keyed > 0 {
            return Ok(());
        }
        for sql in [
            "DROP TABLE IF EXISTS achievement_unlocks_new",
            achievement_unlocks_table!("achievement_unlocks_new"),
        ] {
            sqlx::query(sql).execute(&mut *tx).await?;
        }
        sqlx::query(
            "INSERT INTO achievement_unlocks_new
                 (game_name, api_name, unlocked_at, detected_at, live, display_name, icon,
                  device_id, device_name)
             SELECT game_name, api_name, unlocked_at, detected_at, live, display_name, icon, '', ''
             FROM achievement_unlocks",
        )
        .execute(&mut *tx)
        .await?;
        for sql in [
            "DROP TABLE achievement_unlocks",
            "ALTER TABLE achievement_unlocks_new RENAME TO achievement_unlocks",
        ] {
            sqlx::query(sql).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        tracing::info!("library: migrated achievement_unlocks to per-device rows");
        Ok(())
    }

    /// One-time import of the legacy `library.json`. No-op when the DB already
    /// has games or there's no JSON file. On success the JSON is renamed to
    /// `library.json.migrated` so it survives as a manual rollback backup.
    async fn import_json_if_needed(&self) -> AppResult<()> {
        let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games")
            .fetch_one(&self.pool)
            .await?;
        if existing > 0 {
            return Ok(());
        }
        let path = paths::library_file();
        if !path.exists() {
            return Ok(());
        }
        let json = std::fs::read_to_string(&path)?;
        let mut entries: Vec<GameEntry> = serde_json::from_str(&json)?;
        backfill_catalog_numbers(&mut entries);

        let mut tx = self.pool.begin().await?;
        for entry in &entries {
            let data = serde_json::to_string(entry)?;
            sqlx::query(
                "INSERT INTO games (id, catalog_number, game_name, data) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(&entry.id)
            .bind(entry.catalog_number as i64)
            .bind(&entry.game_name)
            .bind(&data)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        let migrated = path.with_extension("json.migrated");
        if let Err(e) = std::fs::rename(&path, &migrated) {
            tracing::warn!(error = %e, "library import: couldn't rename library.json to .migrated");
        }
        tracing::info!(count = entries.len(), "imported library.json into SQLite");
        Ok(())
    }

    /// One-time migration run when playtime/last-played became *derived* from the
    /// `play_sessions` timeline instead of an accumulator. Playtime predates the
    /// sessions table, so early libraries hold accrued `playtime_minutes` with no
    /// backing session rows; deriving purely from sessions would drop it. For
    /// each game whose cached playtime exceeds the sum of its existing sessions,
    /// this seeds one synthetic "legacy" session covering the gap, so the derived
    /// total matches the previously-displayed value and stays lossless thereafter.
    ///
    /// The legacy row is keyed on the game name (not this device) and attributed
    /// to a sentinel device id, so [`sessions_for_device`] excludes it from this
    /// device's history blob — it never propagates to peers. Each device seeds its
    /// own from its own cached (already cross-device-folded) total, so the
    /// per-device baselines agree without the rows crossing the wire. Guarded by a
    /// `meta` flag so it runs exactly once.
    async fn migrate_legacy_playtime(&self) -> AppResult<()> {
        let done: i64 = sqlx::query_scalar(
            "SELECT COALESCE((SELECT v FROM meta WHERE k = 'playtime_migrated'), 0)",
        )
        .fetch_one(&self.pool)
        .await?;
        if done != 0 {
            return Ok(());
        }

        let entries = self.list().await?;
        let mut seeded = 0usize;
        for g in &entries {
            if g.playtime_minutes <= 0 {
                continue;
            }
            let existing: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(duration_secs), 0) FROM play_sessions WHERE game_name = ?1",
            )
            .bind(&g.game_name)
            .fetch_one(&self.pool)
            .await?;
            let gap = g.playtime_minutes as i64 * 60 - existing;
            if gap <= 0 {
                continue;
            }
            let ts = g.last_played_at.unwrap_or_else(legacy_baseline_ts);
            let legacy = PlaySession {
                session_id: format!("legacy:{}", g.game_name),
                device_id: LEGACY_DEVICE_ID.to_string(),
                device_name: "Imported history".to_string(),
                game_name: g.game_name.clone(),
                started_at: ts,
                ended_at: ts,
                duration_secs: gap,
            };
            if self.insert_session(&legacy).await? {
                seeded += 1;
            }
        }
        sqlx::query(
            "INSERT INTO meta (k, v) VALUES ('playtime_migrated', 1)
             ON CONFLICT(k) DO UPDATE SET v = 1",
        )
        .execute(&self.pool)
        .await?;
        if seeded > 0 {
            tracing::info!(
                seeded,
                "seeded legacy play sessions for pre-sessions playtime"
            );
        }
        Ok(())
    }

    /// All games, ordered by catalog number (their stable shelf order).
    pub async fn list(&self) -> AppResult<Vec<GameEntry>> {
        let rows = sqlx::query("SELECT data FROM games_with_status ORDER BY catalog_number")
            .fetch_all(&self.pool)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let data: String = row.get("data");
            out.push(serde_json::from_str(&data)?);
        }
        Ok(out)
    }

    /// One game by id, or `None` if absent.
    pub async fn find(&self, id: &str) -> AppResult<Option<GameEntry>> {
        let row = sqlx::query("SELECT data FROM games_with_status WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => {
                let data: String = row.get("data");
                Ok(Some(serde_json::from_str(&data)?))
            }
            None => Ok(None),
        }
    }

    /// The id of the first game with an exact `game_name` match.
    pub async fn find_id_by_name(
        &self,
        name: &str,
        exe_path: Option<&str>,
    ) -> AppResult<Option<String>> {
        let row = if let Some(exe) = exe_path {
            sqlx::query(
                "SELECT id FROM games
                 WHERE game_name = ?1
                 ORDER BY
                   CASE WHEN json_extract(data, '$.exe_path') = ?2 THEN 0 ELSE 1 END,
                   catalog_number
                 LIMIT 1",
            )
            .bind(name)
            .bind(exe)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id FROM games
                 WHERE game_name = ?1
                 ORDER BY catalog_number
                 LIMIT 1",
            )
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
        };
        Ok(row.map(|r| r.get::<String, _>("id")))
    }

    /// An existing *uninstalled* entry to reuse when the user re-adds a game
    /// (via the Add flow or a LAN install), so playtime / art / save backups
    /// carry over instead of spawning a duplicate. Prefers a `steam_id` match
    /// (most reliable), falling back to an exact `game_name`; the oldest
    /// `catalog_number` wins when several match. Only ever returns entries with
    /// `installed = false`, so a currently-installed game is never silently
    /// overwritten. Legacy rows missing the `installed` field yield SQL NULL for
    /// `json_extract(... '$.installed')`, which `= 0` excludes — i.e. they're
    /// treated as installed and left alone.
    ///
    /// The name fallback skips a candidate whose `steam_id` *positively
    /// conflicts* with `steam_id` (both known and differing): two genuinely
    /// different games that happen to share a name must not be merged into one
    /// entry. A candidate with no steam id (an untracked entry), or a `None`
    /// request, has no conflict and is still reused by name.
    pub async fn find_reusable_entry(
        &self,
        steam_id: Option<u64>,
        name: &str,
    ) -> AppResult<Option<GameEntry>> {
        if let Some(sid) = steam_id {
            let row = sqlx::query(
                "SELECT data FROM games_with_status
                 WHERE json_extract(data, '$.installed') = 0
                   AND json_extract(data, '$.steam_id') = ?1
                 ORDER BY catalog_number LIMIT 1",
            )
            .bind(sid as i64)
            .fetch_optional(&self.pool)
            .await?;
            if let Some(row) = row {
                let data: String = row.get("data");
                return Ok(Some(serde_json::from_str(&data)?));
            }
        }
        let row = sqlx::query(
            "SELECT data FROM games_with_status
             WHERE json_extract(data, '$.installed') = 0
               AND game_name = ?1
               AND (?2 IS NULL
                    OR json_extract(data, '$.steam_id') IS NULL
                    OR json_extract(data, '$.steam_id') = ?2)
             ORDER BY catalog_number LIMIT 1",
        )
        .bind(name)
        .bind(steam_id.map(|s| s as i64))
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => {
                let data: String = row.get("data");
                Ok(Some(serde_json::from_str(&data)?))
            }
            None => Ok(None),
        }
    }

    /// Number of games in the library.
    pub async fn count(&self) -> AppResult<usize> {
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games")
            .fetch_one(&self.pool)
            .await?;
        Ok(c as usize)
    }

    /// Current value of the `meta.version` counter — bumped by every games
    /// mutation (including ones made by other processes). The GUI polls this
    /// to refresh after an external write.
    pub async fn version(&self) -> AppResult<i64> {
        let v: i64 = sqlx::query_scalar("SELECT v FROM meta WHERE k = 'version'")
            .fetch_one(&self.pool)
            .await?;
        Ok(v)
    }

    /// Inserts a new entry. When `catalog_number` is 0 it's assigned
    /// `max + 1` inside the transaction so concurrent adds don't collide.
    /// Returns the stored entry (with its assigned catalog number).
    pub async fn insert(&self, mut entry: GameEntry) -> AppResult<GameEntry> {
        let mut tx = self.pool.begin().await?;
        if entry.catalog_number == 0 {
            let max: Option<i64> = sqlx::query_scalar("SELECT MAX(catalog_number) FROM games")
                .fetch_one(&mut *tx)
                .await?;
            entry.catalog_number = (max.unwrap_or(0) as u32) + 1;
        }
        let data = serde_json::to_string(&entry)?;
        sqlx::query(
            "INSERT INTO games (id, catalog_number, game_name, data) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&entry.id)
        .bind(entry.catalog_number as i64)
        .bind(&entry.game_name)
        .bind(&data)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(entry)
    }

    /// Replaces an entry from the editor. Writes every editor-owned field but
    /// re-overlays the [`RUNTIME_FIELDS`] from the existing row, so a
    /// concurrent playtime/backup/sync write from another process is preserved.
    /// Atomic: a single `UPDATE` whose `json_set` reads the old row's runtime
    /// values. Returns `false` if no row matched the id.
    pub async fn replace(&self, entry: &GameEntry) -> AppResult<bool> {
        let base = serde_json::to_string(entry)?;
        // Build json_set(?1, '$.f', COALESCE(data -> '$.f', ?1 -> '$.f'), …)
        // nesting so the incoming blob (?1) keeps the existing row's runtime
        // field values. Two subtleties:
        //   * `->` (not json_extract): returns the value preserving its JSON
        //     type, so a boolean like `installed` round-trips as JSON
        //     true/false. json_extract coerces a JSON boolean to SQL integer
        //     0/1, which json_set then writes back as a JSON *number* —
        //     unparseable as a serde `bool`.
        //   * COALESCE(..., ?1 -> '$.f'): when the existing row LACKS the field
        //     (a row written before that field existed — e.g. `installed` on a
        //     pre-upgrade entry), `data -> '$.f'` is SQL NULL and a bare
        //     json_set would write an explicit `"f":null`, which a non-Option
        //     field (`installed: bool`) can't deserialize, corrupting the row.
        //     The fallback uses the incoming entry's own value instead. A field
        //     that's PRESENT but JSON-null (a legitimately null Option) is
        //     SQL-non-NULL via `->`, so COALESCE keeps it — the live value
        //     still wins.
        let mut expr = "?1".to_string();
        for f in RUNTIME_FIELDS {
            expr = format!("json_set({expr}, '$.{f}', COALESCE(data -> '$.{f}', ?1 -> '$.{f}'))");
        }
        // `exe_path` / `game_folder_path` are editor-owned (browse buttons), so
        // they're NOT runtime fields — the editor's value normally wins. The one
        // exception: when the LIVE row is uninstalled (`installed = false`, paths
        // already cleared), a stale editor copy opened while the game was still
        // installed must not resurrect those paths on save. Keep the live
        // (cleared) values in that case; otherwise apply the editor's value.
        // (`json_extract(data,'$.installed') = 0` is false/NULL for an installed
        // or legacy row, so the editor keeps full control there.)
        for f in ["exe_path", "game_folder_path"] {
            expr = format!(
                "json_set({expr}, '$.{f}', \
                 CASE WHEN json_extract(data, '$.installed') = 0 \
                 THEN data -> '$.{f}' ELSE ?1 -> '$.{f}' END)"
            );
        }
        let sql = format!("UPDATE games SET game_name = ?2, data = {expr} WHERE id = ?3");
        let res = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&base)
            .bind(&entry.game_name)
            .bind(&entry.id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Removes an entry by id. Returns whether a row was deleted. Retiring the
    /// id also cleans up its per-game run-lock marker file (best-effort) — this
    /// is the single chokepoint every retirement path goes through (forget,
    /// delete-from-disk, the Decky forget route); uninstall keeps the entry via
    /// `update_fields`, so its lock file is correctly left in place.
    pub async fn remove(&self, id: &str) -> AppResult<bool> {
        let res = sqlx::query("DELETE FROM games WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        let removed = res.rows_affected() > 0;
        if removed {
            crate::proc_lock::remove_run_lock(id);
        }
        Ok(removed)
    }

    /// Atomically sets one or more JSON fields on a single entry, leaving every
    /// other field (in particular the runtime counters other processes write)
    /// untouched. Each value is round-tripped through `json()` so numbers,
    /// strings, arrays and null all land with their proper JSON type. Returns
    /// whether a row matched.
    pub async fn update_fields(&self, id: &str, fields: &[(&str, Value)]) -> AppResult<bool> {
        if fields.is_empty() {
            return Ok(false);
        }
        // data is wrapped left-to-right: json_set(json_set(data,'$.a',?2),'$.b',?3)…
        let mut expr = "data".to_string();
        for (i, (path, _)) in fields.iter().enumerate() {
            let p = i + 2; // ?1 is the id
            expr = format!("json_set({expr}, '$.{path}', json(?{p}))");
        }
        let sql = format!("UPDATE games SET data = {expr} WHERE id = ?1");
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id);
        for (_, v) in fields {
            q = q.bind(serde_json::to_string(v)?);
        }
        let res = q.execute(&self.pool).await?;
        Ok(res.rows_affected() > 0)
    }

    /// Recompute a game's cached `playtime_minutes` + `last_played_at` from the
    /// `play_sessions` timeline — the source of truth — and write them onto every
    /// library row sharing that `game_name` (the control-plane match key).
    /// Playtime is the summed session duration across *all* devices (peer rows
    /// are folded in by [`crate::rclone::fold_history`]); last-played is the
    /// latest session end. Done in SQL so it races safely with other processes.
    ///
    /// No-op returning `Ok(false)` when the game has no sessions yet, so it never
    /// blanks an existing cached value with an empty aggregate.
    pub async fn recompute_playtime(&self, game_name: &str) -> AppResult<bool> {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(duration_secs), 0) AS secs, MAX(ended_at) AS last
             FROM play_sessions WHERE game_name = ?1",
        )
        .bind(game_name)
        .fetch_one(&self.pool)
        .await?;
        let secs: i64 = row.get("secs");
        let last: Option<String> = row.get("last");
        let Some(last) = last else {
            return Ok(false); // no sessions for this game; leave the cache as-is
        };
        let minutes = (secs / 60).min(i32::MAX as i64);
        // A bound text becomes a JSON string under json_set; a bound integer a
        // JSON number — matching the GameEntry serde shapes for the two fields.
        // Only write rows whose cached value actually differs (`IS NOT` is the
        // null-safe compare): SQLite counts a same-value UPDATE as a change, so
        // an unguarded write would bump `meta.version` and make the cross-device
        // fold emit a spurious `library:changed` on every startup. So the
        // returned `rows_affected` reflects real changes, which the fold relies on.
        let res = sqlx::query(
            "UPDATE games SET data = json_set(
                    data,
                    '$.playtime_minutes', ?2,
                    '$.last_played_at', ?3
                 )
             WHERE game_name = ?1
               AND (json_extract(data, '$.playtime_minutes') IS NOT ?2
                    OR json_extract(data, '$.last_played_at') IS NOT ?3)",
        )
        .bind(game_name)
        .bind(minutes)
        .bind(last)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Bulk [`recompute_playtime`] for the cross-device fold: re-derive every
    /// game that has sessions. Returns how many rows were updated. Looped per
    /// game (rather than one `UPDATE … FROM`) to stay portable across SQLite
    /// builds; the session count is small enough that this is cheap.
    pub async fn recompute_all_playtime(&self) -> AppResult<usize> {
        let names: Vec<String> = sqlx::query_scalar("SELECT DISTINCT game_name FROM play_sessions")
            .fetch_all(&self.pool)
            .await?;
        let mut updated = 0usize;
        for name in names {
            if self.recompute_playtime(&name).await? {
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// Records a finished play session. `INSERT OR IGNORE` so re-recording the
    /// same `session_id` (e.g. folding a peer's history that includes a session
    /// we already have) is a no-op rather than an error. Returns whether a new
    /// row was actually inserted.
    pub async fn insert_session(&self, s: &PlaySession) -> AppResult<bool> {
        let res = sqlx::query(
            "INSERT OR IGNORE INTO play_sessions
                 (session_id, device_id, device_name, game_name, started_at, ended_at, duration_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&s.session_id)
        .bind(&s.device_id)
        .bind(&s.device_name)
        .bind(&s.game_name)
        .bind(s.started_at.to_rfc3339())
        .bind(s.ended_at.to_rfc3339())
        .bind(s.duration_secs)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Bulk-insert sessions (the cross-device fold). Each is `INSERT OR IGNORE`d
    /// in one transaction. Returns the number of *new* rows added.
    pub async fn upsert_sessions(&self, sessions: &[PlaySession]) -> AppResult<usize> {
        if sessions.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await?;
        let mut added = 0usize;
        for s in sessions {
            let res = sqlx::query(
                "INSERT OR IGNORE INTO play_sessions
                     (session_id, device_id, device_name, game_name, started_at, ended_at, duration_secs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(&s.session_id)
            .bind(&s.device_id)
            .bind(&s.device_name)
            .bind(&s.game_name)
            .bind(s.started_at.to_rfc3339())
            .bind(s.ended_at.to_rfc3339())
            .bind(s.duration_secs)
            .execute(&mut *tx)
            .await?;
            added += res.rows_affected() as usize;
        }
        tx.commit().await?;
        Ok(added)
    }

    /// All sessions recorded by `device_id`, oldest first. The rclone history
    /// blob is built from this (a projection of the local rows for our device).
    pub async fn sessions_for_device(&self, device_id: &str) -> AppResult<Vec<PlaySession>> {
        let rows = sqlx::query(
            "SELECT session_id, device_id, device_name, game_name, started_at, ended_at, duration_secs
             FROM play_sessions WHERE device_id = ?1 ORDER BY started_at",
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_session).collect()
    }

    /// All sessions across every device, oldest first. When `game_name` is
    /// `Some`, only that game's sessions. Feeds the cross-device timeline chart.
    pub async fn list_sessions(&self, game_name: Option<&str>) -> AppResult<Vec<PlaySession>> {
        let rows = match game_name {
            Some(name) => {
                sqlx::query(
                    "SELECT session_id, device_id, device_name, game_name, started_at, ended_at, duration_secs
                     FROM play_sessions WHERE game_name = ?1 ORDER BY started_at",
                )
                .bind(name)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT session_id, device_id, device_name, game_name, started_at, ended_at, duration_secs
                     FROM play_sessions ORDER BY started_at",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.iter().map(row_to_session).collect()
    }

    /// Records achievement unlocks. `INSERT OR IGNORE` on `(game_name,
    /// api_name)`, so an unlock is recorded once and a re-scan is a no-op.
    /// Returns the rows actually inserted.
    pub async fn insert_achievement_unlocks(
        &self,
        unlocks: &[AchievementUnlock],
    ) -> AppResult<Vec<AchievementUnlock>> {
        if unlocks.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self.pool.begin().await?;
        let mut added = Vec::new();
        for u in unlocks {
            let res = sqlx::query(
                "INSERT OR IGNORE INTO achievement_unlocks
                     (game_name, api_name, unlocked_at, detected_at, live, display_name, icon,
                      device_id, device_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(&u.game_name)
            .bind(&u.api_name)
            .bind(u.unlocked_at)
            .bind(u.detected_at)
            .bind(u.live)
            .bind(&u.display_name)
            .bind(&u.icon)
            .bind(&u.device_id)
            .bind(&u.device_name)
            .execute(&mut *tx)
            .await?;
            if res.rows_affected() > 0 {
                added.push(u.clone());
            }
        }
        tx.commit().await?;
        Ok(added)
    }

    /// Records unlocks caught during a play session (`live = 1`). A row that
    /// already exists as an import (`live = 0`) recorded at or after
    /// `session_start_ms` is upgraded to live: something else (the detail
    /// pane's scan, another process) read the file first during this session,
    /// so the unlock is still new. Returns the rows inserted or upgraded.
    pub async fn record_live_unlocks(
        &self,
        unlocks: &[AchievementUnlock],
        session_start_ms: i64,
    ) -> AppResult<Vec<AchievementUnlock>> {
        if unlocks.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self.pool.begin().await?;
        let mut added = Vec::new();
        for u in unlocks {
            let res = sqlx::query(
                "INSERT INTO achievement_unlocks
                     (game_name, api_name, unlocked_at, detected_at, live, display_name, icon,
                      device_id, device_name)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?8, ?9)
                 ON CONFLICT (game_name, api_name, device_id) DO UPDATE
                     SET live = 1, detected_at = excluded.detected_at
                     WHERE achievement_unlocks.live = 0
                       AND achievement_unlocks.detected_at >= ?7",
            )
            .bind(&u.game_name)
            .bind(&u.api_name)
            .bind(u.unlocked_at)
            .bind(u.detected_at)
            .bind(&u.display_name)
            .bind(&u.icon)
            .bind(session_start_ms)
            .bind(&u.device_id)
            .bind(&u.device_name)
            .execute(&mut *tx)
            .await?;
            if res.rows_affected() > 0 {
                added.push(u.clone());
            }
        }
        tx.commit().await?;
        Ok(added)
    }

    /// Every recorded unlock for one game.
    pub async fn achievement_unlocks_for(
        &self,
        game_name: &str,
    ) -> AppResult<Vec<AchievementUnlock>> {
        let rows = sqlx::query(
            "SELECT game_name, api_name, unlocked_at, detected_at, live, display_name, icon,
                    device_id, device_name
             FROM achievement_unlocks WHERE game_name = ?1 ORDER BY unlocked_at",
        )
        .bind(game_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(row_to_unlock).collect())
    }

    /// This device's own unlocks (empty `device_id`). The rclone achievements
    /// blob is a projection of these.
    pub async fn local_achievement_unlocks(&self) -> AppResult<Vec<AchievementUnlock>> {
        let rows = sqlx::query(
            "SELECT game_name, api_name, unlocked_at, detected_at, live, display_name, icon,
                    device_id, device_name
             FROM achievement_unlocks WHERE device_id = ''
             ORDER BY game_name, unlocked_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(row_to_unlock).collect())
    }

    /// Folds unlocks from devices' blobs (the rclone fold); each row's
    /// `device_id` is the blob it came from.
    ///
    /// * Rows from `our_device_id`'s own blob are stored as this device's
    ///   (empty `device_id`) when missing, which restores them after the local
    ///   DB was wiped. Existing local rows are left alone.
    /// * Other devices' rows mirror their blob: inserted, or updated when the
    ///   blob's time or device name differs (a rename shows up on the next fold).
    ///
    /// Folded rows are never live, so they don't toast. Returns the number of
    /// rows inserted or changed.
    pub async fn fold_achievement_unlocks(
        &self,
        unlocks: &[AchievementUnlock],
        our_device_id: &str,
    ) -> AppResult<usize> {
        if unlocks.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await?;
        let mut changed = 0usize;
        for u in unlocks {
            let ours = !our_device_id.is_empty() && u.device_id == our_device_id;
            let (device_id, device_name) = if ours {
                ("", "")
            } else {
                (u.device_id.as_str(), u.device_name.as_str())
            };
            let res = sqlx::query(
                "INSERT INTO achievement_unlocks
                     (game_name, api_name, unlocked_at, detected_at, live, display_name, icon,
                      device_id, device_name)
                 VALUES (?1, ?2, ?3, ?4, 0, NULL, NULL, ?5, ?6)
                 ON CONFLICT (game_name, api_name, device_id) DO UPDATE
                     SET unlocked_at = excluded.unlocked_at,
                         device_name = excluded.device_name
                     WHERE excluded.device_id != ''
                       AND (achievement_unlocks.unlocked_at != excluded.unlocked_at
                            OR achievement_unlocks.device_name != excluded.device_name)",
            )
            .bind(&u.game_name)
            .bind(&u.api_name)
            .bind(u.unlocked_at)
            .bind(u.detected_at)
            .bind(device_id)
            .bind(device_name)
            .execute(&mut *tx)
            .await?;
            changed += res.rows_affected() as usize;
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Bumps the `achievements_version` counter without changing any unlock,
    /// so an open achievements card reloads (e.g. once a schema it had to
    /// fetch in the background is cached).
    pub async fn bump_achievements_version(&self) -> AppResult<()> {
        sqlx::query("UPDATE meta SET v = v + 1 WHERE k = 'achievements_version'")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Current value of the `achievements_version` counter, bumped by every
    /// insert or update of `achievement_unlocks` in any Spool process.
    pub async fn achievements_version(&self) -> AppResult<i64> {
        let v: i64 = sqlx::query_scalar(
            "SELECT COALESCE((SELECT v FROM meta WHERE k = 'achievements_version'), 0)",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(v)
    }

    /// Unlocks caught during a play session (`live = 1`) and recorded after
    /// `since_ms`, oldest first. The Decky plugin polls this for its toasts.
    #[cfg_attr(not(unix), allow(dead_code))] // only reached via the unix-gated plugin server
    pub async fn live_achievement_unlocks_since(
        &self,
        since_ms: i64,
    ) -> AppResult<Vec<AchievementUnlock>> {
        let rows = sqlx::query(
            "SELECT game_name, api_name, unlocked_at, detected_at, live, display_name, icon,
                    device_id, device_name
             FROM achievement_unlocks WHERE live = 1 AND detected_at > ?1
             ORDER BY detected_at LIMIT 50",
        )
        .bind(since_ms)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(row_to_unlock).collect())
    }

    /// Persists post-backup stats. `size_mb` is optional — `None` leaves the
    /// existing recorded size in place (the refresh-only path doesn't know it).
    pub async fn record_backup_stats(
        &self,
        id: &str,
        count: i32,
        last_at: Option<DateTime<Utc>>,
        size_mb: Option<f64>,
    ) -> AppResult<bool> {
        let mut fields = vec![
            ("save_backup_count", json!(count)),
            ("save_last_backed_up_at", serde_json::to_value(last_at)?),
        ];
        if let Some(mb) = size_mb {
            fields.push(("save_backup_size_mb", json!(mb)));
        }
        self.update_fields(id, &fields).await
    }

    /// Sets the cross-device sync badge ("synced" / "local-newer" / …).
    pub async fn set_sync_badge(&self, id: &str, badge: &str) -> AppResult<bool> {
        self.update_fields(id, &[("sync_badge", json!(badge))])
            .await
    }

    /// Records the cloud-sync merge-base for fast-forward vs. divergence
    /// detection.
    pub async fn set_cloud_baseline(&self, id: &str, tip: &str) -> AppResult<bool> {
        self.update_fields(id, &[("cloud_sync_baseline", json!(tip))])
            .await
    }

    /// Sets (or clears, with `None`) a game's custom save location. Written
    /// atomically via `json_set` so it doesn't race a concurrent editor save or
    /// playtime bump — and `custom_save` is in [`RUNTIME_FIELDS`], so a later
    /// whole-entry `replace` re-overlays this value rather than clobbering it.
    pub async fn set_custom_save(&self, id: &str, custom: Option<&CustomSave>) -> AppResult<bool> {
        // Dedup files/registry (first-seen order) so a malformed or hand-edited
        // cross-device definition with repeated paths can't reach the editor's
        // keyed `{#each}` and crash it. This is the single chokepoint every
        // write path (editor command + cross-device adopt) flows through.
        let deduped = custom.map(|cs| CustomSave {
            files: dedup_preserve_order(&cs.files),
            registry: dedup_preserve_order(&cs.registry),
        });
        let value = serde_json::to_value(deduped.as_ref())?;
        self.update_fields(id, &[("custom_save", value)]).await
    }

    /// Sets the custom save only when the entry doesn't already have one. Used by
    /// the cross-device adopt path so it can't clobber a custom save the user set
    /// during the (network) fetch — the check-and-write is one atomic conditional
    /// UPDATE rather than a find()-then-set() race. Returns whether a row was set.
    pub async fn set_custom_save_if_absent(
        &self,
        id: &str,
        custom: &CustomSave,
    ) -> AppResult<bool> {
        let deduped = CustomSave {
            files: dedup_preserve_order(&custom.files),
            registry: dedup_preserve_order(&custom.registry),
        };
        let value = serde_json::to_string(&deduped)?;
        let sql = "UPDATE games SET data = json_set(data, '$.custom_save', json(?2))
                   WHERE id = ?1 AND json_extract(data, '$.custom_save') IS NULL";
        let res = sqlx::query(sql)
            .bind(id)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Sets (or clears, with `None`) a game's manifest override. Written via
    /// `json_set` like `custom_save`, and `manifest_override` is in
    /// [`RUNTIME_FIELDS`] so a whole-entry editor save re-overlays it.
    pub async fn set_manifest_override(
        &self,
        id: &str,
        ov: Option<&ManifestOverride>,
    ) -> AppResult<bool> {
        // An inactive (empty) override is equivalent to none — store NULL so the
        // re-derivation and the UI both treat it as "plain manifest".
        let normalized = ov.filter(|o| o.is_active());
        let value = serde_json::to_value(normalized)?;
        self.update_fields(id, &[("manifest_override", value)])
            .await
    }

    /// Sets the manifest override only when the entry doesn't already have one —
    /// the cross-device adopt path, so it can't clobber an override the user set
    /// during the (network) fetch. Returns whether a row was set.
    pub async fn set_manifest_override_if_absent(
        &self,
        id: &str,
        ov: &ManifestOverride,
    ) -> AppResult<bool> {
        let value = serde_json::to_string(ov)?;
        let sql = "UPDATE games SET data = json_set(data, '$.manifest_override', json(?2))
                   WHERE id = ?1 AND json_extract(data, '$.manifest_override') IS NULL";
        let res = sqlx::query(sql)
            .bind(id)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Sets (or clears, with `None`) the play status for `game_name`. Stamped
    /// `now`, or 1 ms past the stored time when that's later (a change adopted
    /// from a device whose clock runs ahead), so the user's change is always
    /// the newest and a fold can't put the old status back. Returns the stamp.
    pub async fn set_play_status(
        &self,
        game_name: &str,
        status: Option<&str>,
        now: i64,
    ) -> AppResult<i64> {
        let at: i64 = sqlx::query_scalar(
            "INSERT INTO play_statuses (game_name, status, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (game_name) DO UPDATE SET
                 status = excluded.status,
                 updated_at = MAX(excluded.updated_at, play_statuses.updated_at + 1)
             RETURNING updated_at",
        )
        .bind(game_name)
        .bind(status)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(at)
    }

    /// Adopts each row that's newer than what's stored for its game name (or
    /// has no stored row), in one transaction. Two changes with the same time
    /// are ordered by status (a clear sorts first), so every device settles on
    /// the same one. Statuses this build doesn't know are kept as-is, so an
    /// older build republishes a newer build's change instead of reviving the
    /// one before it. Returns the number of rows changed.
    pub async fn fold_play_statuses(&self, rows: &[PlayStatusRow]) -> AppResult<u64> {
        let mut tx = self.pool.begin().await?;
        let mut changed = 0;
        for r in rows.iter().filter(|r| !r.game_name.is_empty()) {
            changed += sqlx::query(
                "INSERT INTO play_statuses (game_name, status, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (game_name) DO UPDATE SET
                     status = excluded.status, updated_at = excluded.updated_at
                 WHERE excluded.updated_at > play_statuses.updated_at
                    OR (excluded.updated_at = play_statuses.updated_at
                        AND COALESCE(excluded.status, '') > COALESCE(play_statuses.status, ''))",
            )
            .bind(&r.game_name)
            .bind(&r.status)
            .bind(r.updated_at)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Every stored play status, cleared ones included, by game name.
    pub async fn play_statuses(&self) -> AppResult<Vec<PlayStatusRow>> {
        let rows = sqlx::query(
            "SELECT game_name, status, updated_at FROM play_statuses ORDER BY game_name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| PlayStatusRow {
                game_name: r.get("game_name"),
                status: r.get("status"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    /// Copies the play status stored for `old` to `new`, for a game being
    /// renamed. Stamped like [`Library::set_play_status`], so it replaces any
    /// status `new` already had and wins over it on other devices. The `old`
    /// row stays: another entry (or another device's game) may still use that
    /// name, and renaming back later overwrites it the same way. Returns
    /// whether there was a status to copy.
    pub async fn copy_play_status(&self, old: &str, new: &str, now: i64) -> AppResult<bool> {
        let res = sqlx::query(
            "INSERT INTO play_statuses (game_name, status, updated_at)
             SELECT ?2, status, ?3 FROM play_statuses WHERE game_name = ?1
             ON CONFLICT (game_name) DO UPDATE SET
                 status = excluded.status,
                 updated_at = MAX(excluded.updated_at, play_statuses.updated_at + 1)",
        )
        .bind(old)
        .bind(new)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Updates any of cover path / hero path / accent colour that are `Some`.
    pub async fn set_art(
        &self,
        id: &str,
        cover: Option<&str>,
        hero: Option<&str>,
        accent: Option<&str>,
    ) -> AppResult<bool> {
        let mut fields = Vec::new();
        if let Some(c) = cover {
            fields.push(("cover_image_path", json!(c)));
        }
        if let Some(h) = hero {
            fields.push(("hero_image_path", json!(h)));
        }
        if let Some(a) = accent {
            fields.push(("accent_color", json!(a)));
        }
        self.update_fields(id, &fields).await
    }

    /// Sets the accent colour only if the entry doesn't already have one — so a
    /// concurrent SteamGridDB refresh that already set it isn't overwritten by
    /// the startup backfill.
    pub async fn set_accent_if_empty(&self, id: &str, color: &str) -> AppResult<bool> {
        let sql = "UPDATE games SET data = json_set(data, '$.accent_color', ?2)
                   WHERE id = ?1 AND json_extract(data, '$.accent_color') IS NULL";
        let res = sqlx::query(sql)
            .bind(id)
            .bind(color)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Replaces the accent colour only if it's still `old`, so a concurrent
    /// update isn't overwritten. Used by `accent::normalize_stored`.
    pub async fn replace_accent(&self, id: &str, old: &str, new: &str) -> AppResult<bool> {
        let sql = "UPDATE games SET data = json_set(data, '$.accent_color', ?3)
                   WHERE id = ?1 AND json_extract(data, '$.accent_color') = ?2";
        let res = sqlx::query(sql)
            .bind(id)
            .bind(old)
            .bind(new)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Sets the install size only if the entry doesn't already have one (> 0).
    pub async fn set_install_size_if_empty(&self, id: &str, mb: f64) -> AppResult<bool> {
        let sql = "UPDATE games SET data = json_set(data, '$.install_size_mb', ?2)
                   WHERE id = ?1 AND COALESCE(json_extract(data, '$.install_size_mb'), 0) <= 0";
        let res = sqlx::query(sql)
            .bind(id)
            .bind(mb)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}

/// Assigns sequential catalog numbers to any entries missing one (0).
/// Preserves existing assignments. Used during the one-time JSON import.
fn backfill_catalog_numbers(entries: &mut [GameEntry]) {
    let mut next = entries.iter().map(|e| e.catalog_number).max().unwrap_or(0);
    for entry in entries.iter_mut() {
        if entry.catalog_number == 0 {
            next += 1;
            entry.catalog_number = next;
        }
    }
}

/// Shared library state. The [`Arc`] lets callers clone a handle into spawned
/// tasks without touching Tauri's `State<'_, _>` lifetime — in particular
/// `lan/install.rs`'s download task and the headless plugin server both need to
/// add a new entry after the partial rename.
pub type SharedLibrary = Arc<Library>;

/// Filesystem-safe filename derived from a game name.
///
/// Invalid path characters are replaced with spaces (not stripped) so word
/// boundaries are preserved — `"A: B/C"` becomes `"A B C"` rather than
/// `"A BC"`. Unicode is preserved; whitespace runs are then collapsed.
pub fn make_safe_filename(name: &str) -> String {
    const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    // Pass 1: invalid-or-control chars become spaces.
    let stage: String = name
        .chars()
        .map(|c| {
            if INVALID.contains(&c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();

    // Pass 2: collapse runs of whitespace to a single space.
    let mut collapsed = String::with_capacity(stage.len());
    let mut last_space = false;
    for c in stage.chars() {
        if c.is_whitespace() {
            if !last_space {
                collapsed.push(' ');
                last_space = true;
            }
        } else {
            collapsed.push(c);
            last_space = false;
        }
    }
    let trimmed = collapsed.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "Game".to_string()
    } else {
        trimmed.to_string()
    }
}

struct AddGameImportResult {
    new_game: NewGame,
    original_folder: Option<PathBuf>,
    source_to_delete: Option<PathBuf>,
}

fn library_folder_contains_game(root: &Path, game_folder: &Path) -> bool {
    let root_real = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let game_real =
        std::fs::canonicalize(game_folder).unwrap_or_else(|_| game_folder.to_path_buf());
    game_real.starts_with(&root_real)
}

fn default_library_folder(app: &AppHandle) -> AppResult<Option<String>> {
    let config = app.state::<crate::config::SharedConfig>();
    let guard = config.lock().map_err(|_| AppError::LockPoisoned)?;
    Ok(guard.data.default_install_folder().map(|f| f.path.clone()))
}

async fn maybe_import_add_game(
    app: &AppHandle,
    mut new_game: NewGame,
) -> AppResult<AddGameImportResult> {
    if !new_game.import_to_library {
        return Ok(AddGameImportResult {
            new_game,
            original_folder: None,
            source_to_delete: None,
        });
    }

    let src_folder = match new_game
        .game_folder_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        Some(p) => PathBuf::from(p),
        None => {
            return Ok(AddGameImportResult {
                new_game,
                original_folder: None,
                source_to_delete: None,
            })
        }
    };
    if !src_folder.is_dir() {
        return Err(AppError::Other(format!(
            "Install folder doesn't exist on disk: {}",
            src_folder.display()
        )));
    }

    let rel_exe = crate::move_install::relative_inside(Path::new(&new_game.exe_path), &src_folder)
        .ok_or_else(|| {
            AppError::Other(
                "The game's executable is outside its install folder, so it can't be imported automatically. Pick the folder that contains the executable.".into(),
            )
        })?;

    let default_root = match default_library_folder(app)? {
        Some(root) => PathBuf::from(root),
        None => {
            return Err(AppError::Other(
                "Add a library folder in Settings before importing games into the library.".into(),
            ))
        }
    };

    let config = app.state::<crate::config::SharedConfig>();
    {
        let guard = config.lock().map_err(|_| AppError::LockPoisoned)?;
        if guard
            .data
            .library_folders
            .iter()
            .any(|f| library_folder_contains_game(Path::new(&f.path), &src_folder))
        {
            return Ok(AddGameImportResult {
                new_game,
                original_folder: None,
                source_to_delete: None,
            });
        }
    }

    let dest = default_root.join(make_safe_filename(&new_game.game_name));
    if crate::move_install::paths_equal(&src_folder, &dest) {
        return Ok(AddGameImportResult {
            new_game,
            original_folder: None,
            source_to_delete: None,
        });
    }
    if dest.exists() {
        return Err(AppError::Other(format!(
            "A folder named '{}' already exists in the default library folder.",
            dest.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("this game")
        )));
    }

    let src_real = std::fs::canonicalize(&src_folder).unwrap_or_else(|_| src_folder.clone());
    let default_root_real =
        std::fs::canonicalize(&default_root).unwrap_or_else(|_| default_root.clone());
    if default_root_real.starts_with(&src_real) {
        return Err(AppError::Other(
            "The default library folder is inside the game's install folder. Pick a different library folder in Settings.".into(),
        ));
    }

    tracing::info!(
        game_name = %new_game.game_name,
        src = %src_folder.display(),
        dest = %dest.display(),
        "add_game: importing install folder into library"
    );
    let move_state = Arc::new(crate::move_install::MoveState::default());
    let outcome = crate::move_install::run_move(app, &move_state, &src_folder, &dest).await?;

    new_game.exe_path = dest.join(rel_exe).to_string_lossy().to_string();
    new_game.game_folder_path = Some(dest.to_string_lossy().to_string());
    Ok(AddGameImportResult {
        new_game,
        original_folder: Some(src_folder),
        source_to_delete: outcome.source_to_delete,
    })
}

async fn cleanup_imported_source(source_to_delete: Option<PathBuf>) {
    if let Some(old_src) = source_to_delete {
        match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&old_src)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "add_game import: couldn't delete original source after copy")
            }
            Err(e) => tracing::warn!(error = %e, "add_game import: source delete task join failed"),
        }
    }
}

async fn rollback_imported_folder(
    folder: Option<&String>,
    original_folder: Option<PathBuf>,
    source_to_delete: Option<PathBuf>,
) {
    let (Some(folder), Some(src)) = (folder, original_folder) else {
        return;
    };
    let dest = PathBuf::from(folder);
    if source_to_delete.is_some() {
        if let Err(e) = tokio::fs::remove_dir_all(&dest).await {
            tracing::warn!(error = %e, "add_game import: couldn't remove copied destination after DB failure");
        }
    } else if dest.exists() {
        tracing::info!(dest = %dest.display(), "add_game import: rolling back same-filesystem rename after DB failure");
        if let Err(e) = tokio::fs::rename(&dest, &src).await {
            tracing::warn!(error = %e, "add_game import: rollback rename failed after DB failure");
        }
    }
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_games(state: State<'_, SharedLibrary>) -> AppResult<Vec<GameEntry>> {
    state.list().await
}

/// All recorded play sessions across every device, oldest first. Pass
/// `game_name` to scope to one game. Feeds the cross-device activity timeline.
#[tauri::command]
pub async fn list_play_sessions(
    state: State<'_, SharedLibrary>,
    game_name: Option<String>,
) -> AppResult<Vec<PlaySession>> {
    state.list_sessions(game_name.as_deref()).await
}

/// Sets (or clears, with `None`) a game's play status, then publishes this
/// device's statuses to the cloud remote in the background.
#[tauri::command]
pub async fn set_play_status(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    game_id: String,
    status: Option<String>,
) -> AppResult<()> {
    if let Some(s) = status.as_deref() {
        if !is_valid_play_status(s) {
            return Err(AppError::Other(format!("unknown play status: {s}")));
        }
    }
    let game = state
        .find(&game_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
    state
        .set_play_status(
            &game.game_name,
            status.as_deref(),
            Utc::now().timestamp_millis(),
        )
        .await?;
    let _ = app.emit("library:changed", &game_id);
    tauri::async_runtime::spawn(async move {
        crate::rclone::sync_play_status(&app).await;
    });
    Ok(())
}

/// Adds a new game. Assigns id/catalog/timestamps server-side; persists;
/// emits `library:changed` so any open windows can refresh.
#[tauri::command]
pub async fn add_game(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    new_game: NewGame,
) -> AppResult<GameEntry> {
    let AddGameImportResult {
        new_game,
        original_folder: imported_original_folder,
        source_to_delete: imported_source_to_delete,
    } = maybe_import_add_game(&app, new_game).await?;

    // Seed the per-entry Run-As-Admin toggle from the Windows AppCompatFlags
    // registry so an exe the OS already flags as "always run as administrator"
    // imports with the toggle on (no-op / false on non-Windows). Launches honour
    // the registry at runtime regardless, but reflecting it on the entry keeps
    // the editor toggle truthful instead of showing "off" for an elevated exe.
    let run_as_admin = crate::registry::run_as_admin_in_registry(&new_game.exe_path);

    // Re-add reuse: if this game matches an existing *uninstalled* entry,
    // reinstall it in place so its catalog number, playtime, art, and save
    // backups carry over instead of spawning a duplicate. An explicit
    // `reinstall_target_id` (from the "Reinstall…" affordance) wins; otherwise
    // match by steam id / name. A stale target (deleted, or already installed)
    // falls through to the name/steam-id match, then to a fresh insert.
    let reuse = match &new_game.reinstall_target_id {
        Some(id) => match state.find(id).await? {
            // Reuse the explicitly chosen target only when it's uninstalled AND
            // not clearly a *different* game. A positive steam-id conflict (both
            // known and differing) means the user picked another game's exe, so
            // we fall through rather than repurpose the entry. Name isn't
            // required to match — an "add without save tracking" or renamed
            // reinstall legitimately differs from the stored name.
            Some(e)
                if !e.installed
                    && !matches!(
                        (e.steam_id, new_game.steam_id),
                        (Some(a), Some(b)) if a != b
                    ) =>
            {
                Some(e)
            }
            _ => {
                state
                    .find_reusable_entry(new_game.steam_id, &new_game.game_name)
                    .await?
            }
        },
        None => {
            state
                .find_reusable_entry(new_game.steam_id, &new_game.game_name)
                .await?
        }
    };

    if let Some(existing) = reuse {
        // Overwrite the install-identifying fields and flip `installed` back on.
        // Manifest / identification fields are only overwritten when the add
        // actually supplies them, so an "add without save tracking" reinstall
        // doesn't wipe the entry's existing steam id / save paths. Everything
        // else (catalog_number, added_at, accent, playtime, save stats,
        // custom_save) is left untouched by `update_fields`.
        let mut fields: Vec<(&str, Value)> = vec![
            ("installed", json!(true)),
            ("exe_path", json!(new_game.exe_path)),
            (
                "game_folder_path",
                match &new_game.game_folder_path {
                    Some(f) => json!(f),
                    None => Value::Null,
                },
            ),
            ("run_as_admin", json!(run_as_admin)),
        ];
        if new_game.steam_id.is_some() {
            fields.push(("steam_id", json!(new_game.steam_id)));
        }
        if new_game.gog_id.is_some() {
            fields.push(("gog_id", json!(new_game.gog_id)));
        }
        if new_game.lutris_slug.is_some() {
            fields.push(("lutris_slug", json!(new_game.lutris_slug)));
        }
        if new_game.manifest_install_dir.is_some() {
            fields.push(("manifest_install_dir", json!(new_game.manifest_install_dir)));
        }
        if !new_game.save_paths.is_empty() {
            fields.push(("save_paths", json!(new_game.save_paths)));
        }
        if new_game.wine_prefix_path.is_some() {
            fields.push(("wine_prefix_path", json!(new_game.wine_prefix_path)));
        }
        if new_game.proton_version_path.is_some() {
            fields.push(("proton_version_path", json!(new_game.proton_version_path)));
        }
        if new_game.custom_save.is_some() {
            fields.push(("custom_save", json!(new_game.custom_save)));
        }
        if let Err(e) = state.update_fields(&existing.id, &fields).await {
            rollback_imported_folder(
                new_game.game_folder_path.as_ref(),
                imported_original_folder,
                imported_source_to_delete,
            )
            .await;
            return Err(e);
        }
        cleanup_imported_source(imported_source_to_delete).await;
        if let Err(e) = app.emit("library:changed", &existing.id) {
            tracing::warn!(error = %e, "failed to emit library:changed after reinstall");
        }
        // The entry already has its cover / hero / metadata from when it was
        // first added, so no art or Steam-Store fetch is needed here.
        let refreshed = state
            .find(&existing.id)
            .await?
            .ok_or_else(|| AppError::Other(format!("reinstalled game {} vanished", existing.id)))?;
        // The reinstalled folder's hashes are stale (or absent) in the LAN
        // manifest cache — pre-hash in the background so a peer's first
        // manifest request doesn't have to (#435).
        if refreshed.lan_shared
            && refreshed
                .game_folder_path
                .as_deref()
                .is_some_and(|p| !p.is_empty())
        {
            crate::lan::spawn_manifest_prep(app.clone(), refreshed.id.clone());
        }
        return Ok(refreshed);
    }

    let imported_folder_for_rollback = new_game.game_folder_path.clone();
    let entry = GameEntry {
        id: uuid::Uuid::new_v4().to_string(),
        // 0 → insert() assigns the next catalog number atomically.
        catalog_number: 0,
        game_name: new_game.game_name.clone(),
        exe_path: new_game.exe_path,
        run_as_admin,
        safe_name: make_safe_filename(&new_game.game_name),
        added_at: Some(Utc::now()),
        steam_id: new_game.steam_id,
        gog_id: new_game.gog_id,
        lutris_slug: new_game.lutris_slug,
        manifest_install_dir: new_game.manifest_install_dir,
        save_paths: new_game.save_paths,
        custom_save: new_game.custom_save,
        game_folder_path: new_game.game_folder_path,
        wine_prefix_path: new_game.wine_prefix_path,
        proton_version_path: new_game.proton_version_path,
        // Newly added games are shared on the LAN by default; the user can
        // turn this off per-game in the editor. Sharing only actually
        // streams when game_folder_path is set (auto-detected on add).
        lan_shared: true,
        ..GameEntry::default()
    };
    let entry = match state.insert(entry).await {
        Ok(entry) => entry,
        Err(e) => {
            rollback_imported_folder(
                imported_folder_for_rollback.as_ref(),
                imported_original_folder,
                imported_source_to_delete,
            )
            .await;
            return Err(e);
        }
    };
    cleanup_imported_source(imported_source_to_delete).await;
    if let Err(e) = app.emit("library:changed", &entry.id) {
        tracing::warn!(error = %e, "failed to emit library:changed after add_game");
    }

    // Pre-hash the install folder for LAN sharing so the manifest is ready
    // before any peer asks for it (#435). A separate spawn from the metadata
    // enrichment below — hashing a large folder must not delay artwork, and
    // vice versa.
    if entry.lan_shared
        && entry
            .game_folder_path
            .as_deref()
            .is_some_and(|p| !p.is_empty())
    {
        crate::lan::spawn_manifest_prep(app.clone(), entry.id.clone());
    }

    // Enrich the new entry in the background — the card shows immediately while
    // artwork and Steam-Store metadata land a moment later via library:changed
    // emits. For an untracked add (no steam id from ludusavi) we first try to
    // resolve a steam id from the game name via Steam's store search and persist
    // it, so the art + metadata fetches below key off it (Steam-CDN art +
    // Steam-Store metadata) instead of a name-only SteamGridDB lookup. The
    // resolved id only drives art/metadata; it does not enable save tracking.
    // Best-effort throughout — any miss leaves steam_id unset and falls back.
    let app_for_enrich = app.clone();
    let id_for_enrich = entry.id.clone();
    let name_for_enrich = entry.game_name.clone();
    let had_steam_id = entry.steam_id.is_some();
    tauri::async_runtime::spawn(async move {
        if !had_steam_id {
            let client = app_for_enrich.state::<crate::metadata::MetadataClient>();
            match crate::metadata::resolve_steam_appid(client.http(), &name_for_enrich).await {
                Ok(Some(appid)) => {
                    let library = app_for_enrich.state::<SharedLibrary>();
                    if let Err(e) = library
                        .update_fields(&id_for_enrich, &[("steam_id", json!(appid))])
                        .await
                    {
                        tracing::warn!(game_id = %id_for_enrich, error = %e, "failed to persist resolved steam id");
                    } else if let Err(e) = app_for_enrich.emit("library:changed", &id_for_enrich) {
                        tracing::warn!(error = %e, "failed to emit library:changed after steam id resolve");
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(name = %name_for_enrich, error = %e, "steam appid resolve failed");
                }
            }
        }

        // Cover + hero banner (one sgdb game-id / Steam CDN lookup feeds both)
        // and Steam-Store metadata (fills only empty fields; a no-op without a
        // steam id) run concurrently — they touch different fields. Any steam id
        // resolved above is already persisted, so both see it.
        let art =
            crate::steamgriddb::fetch_and_save_cover_and_hero(&app_for_enrich, &id_for_enrich);
        let meta = crate::metadata::fetch_and_save_metadata(&app_for_enrich, &id_for_enrich);
        let (art_res, meta_res) = tokio::join!(art, meta);
        if let Err(e) = art_res {
            tracing::warn!(game_id = %id_for_enrich, error = %e, "cover/hero fetch failed");
        }
        if let Err(e) = meta_res {
            tracing::warn!(game_id = %id_for_enrich, error = %e, "metadata fetch failed");
        }
    });

    // Adopt a cross-device custom-save definition for this game name, if another
    // device published one — so a non-manifest game's save location only has to
    // be picked once. Best-effort; no-op when added with its own custom save or
    // when none is published. Gated on cloud being configured so the common
    // no-cloud add doesn't spawn a task with no remote to read.
    if crate::ludusavi_config::cloud_remote_is_configured() {
        let app_for_adopt = app.clone();
        let id_for_adopt = entry.id.clone();
        let name_for_adopt = entry.game_name.clone();
        tauri::async_runtime::spawn(async move {
            crate::custom_saves::adopt_for_new_game(&app_for_adopt, &id_for_adopt, &name_for_adopt)
                .await;
        });
    }

    Ok(entry)
}

/// Replaces an entry by id with the provided value. The id field on
/// `entry` is the lookup key; mismatches between in-memory state and
/// disk are resolved by overwriting.
#[tauri::command]
pub async fn update_game(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    entry: GameEntry,
) -> AppResult<GameEntry> {
    // Snapshot the pre-edit row so we can tell whether the Steam shortcut needs
    // reconciling (name/exe changed) — `steam_app_id` is a runtime field, so the
    // replace below preserves it, but we read the *old* name/exe here.
    let prev = state.find(&entry.id).await?;
    // The play status is keyed by name, so a rename copies it to the new name.
    // Done before the replace, so a read in between never sees the renamed
    // game without its status.
    let status_copied = match prev.as_ref().filter(|p| p.game_name != entry.game_name) {
        Some(prev) => {
            state
                .copy_play_status(
                    &prev.game_name,
                    &entry.game_name,
                    Utc::now().timestamp_millis(),
                )
                .await?
        }
        None => false,
    };
    if !state.replace(&entry).await? {
        return Err(AppError::Other(format!(
            "game with id {} not found",
            entry.id
        )));
    }
    if status_copied {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            crate::rclone::sync_play_status(&app).await;
        });
    }

    // Pre-hash for LAN sharing when this edit made the game newly shareable
    // (sharing turned on, or the install folder changed while shared) so the
    // manifest is ready before a peer asks (#435). Unchanged shares skip the
    // spawn entirely — no point re-walking on every save of other fields.
    let folder_set = entry
        .game_folder_path
        .as_deref()
        .is_some_and(|p| !p.is_empty());
    let newly_shareable = entry.lan_shared
        && folder_set
        && prev
            .as_ref()
            .is_none_or(|p| !p.lan_shared || p.game_folder_path != entry.game_folder_path);
    if newly_shareable {
        crate::lan::spawn_manifest_prep(app.clone(), entry.id.clone());
    }

    // Reconcile the Steam shortcut for a game that was added to Steam (has a
    // tracked appid) when its name or exe changed — the appid is name-derived,
    // so a rename would otherwise orphan the old shortcut. Best-effort: a
    // reconcile failure (e.g. Steam wouldn't shut down) never fails the save.
    if let Some(prev) = prev {
        if let Some(old_app_id) = prev.steam_app_id {
            let changed = prev.game_name != entry.game_name || prev.exe_path != entry.exe_path;
            if changed {
                match crate::steam::reconcile_renamed_game(
                    old_app_id,
                    &entry.game_name,
                    &entry.exe_path,
                )
                .await
                {
                    Ok(new_app_id) if new_app_id != old_app_id => {
                        if let Err(e) = state
                            .update_fields(
                                &entry.id,
                                &[("steam_app_id", serde_json::json!(new_app_id))],
                            )
                            .await
                        {
                            tracing::warn!(error = %e, "update_game: failed to record new steam_app_id");
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "update_game: Steam shortcut reconcile failed");
                    }
                }
            }
        }
    }

    let updated = entry;
    if let Err(e) = app.emit("library:changed", &updated.id) {
        tracing::warn!(error = %e, "failed to emit library:changed after update_game");
    }
    Ok(updated)
}

/// Removes an entry by id. No-op if the id isn't present (returns false).
/// Emits `library.changed` when something was actually removed.
#[tauri::command]
pub async fn remove_game(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    id: String,
) -> AppResult<bool> {
    // Capture the Steam shortcut identity before deleting the row, so we can
    // clean it up afterwards (the appid is gone once the entry is removed).
    let steam = capture_steam_identity(state.inner(), &id).await?;
    let removed = state.remove(&id).await?;
    if removed {
        reconcile_steam_removal(steam).await;
        if let Err(e) = app.emit("library:changed", &id) {
            tracing::warn!(error = %e, "failed to emit library:changed after remove_game");
        }
    }
    Ok(removed)
}

/// Snapshots a game's Steam shortcut identity (`(appid, name)`) before its row
/// is deleted, so [`reconcile_steam_removal`] can clean the shortcut up after.
/// `None` when the game is absent or was never added to Steam.
async fn capture_steam_identity(
    library: &SharedLibrary,
    id: &str,
) -> AppResult<Option<(u32, String)>> {
    Ok(library
        .find(id)
        .await?
        .and_then(|e| e.steam_app_id.map(|aid| (aid, e.game_name))))
}

/// Removes a forgotten/deleted game's Steam shortcut, given its captured
/// `(appid, name)` (or `None` when it was never added to Steam). Best-effort:
/// logs and continues on failure — the library entry is already gone.
async fn reconcile_steam_removal(steam: Option<(u32, String)>) {
    if let Some((app_id, name)) = steam {
        if let Err(e) = crate::steam::remove_game_from_steam(app_id, &name).await {
            tracing::warn!(error = %e, "failed to remove Steam shortcut for removed game");
        }
    }
}

/// Deletes a game's install folder from disk, then removes its library entry.
///
/// Unlike [`remove_game`] (which only forgets the entry), this reclaims the
/// disk space by deleting the folder recorded in `game_folder_path`. The
/// folder is removed first; only if that succeeds is the library entry
/// dropped, so a failed delete leaves the library pointing at a folder that
/// still exists rather than orphaning files the UI can no longer find.
///
/// On Linux it also deletes the game's per-game Proton/Wine prefix (the
/// `wine_prefix_path` override, or `prefixes/<id>` under Spool's data dir) as
/// a best-effort step — a failed or missing prefix delete doesn't abort the
/// operation. On non-Linux platforms the prefix step is skipped entirely
/// (Proton is Linux-only), so a populated override is never touched there.
///
/// Refuses to run when the game has no known install folder, and rejects
/// obviously-too-broad targets (filesystem root, the user's home dir, Spool's
/// own data dir, or any path fewer than two components deep) so a bad
/// `game_folder_path` can't wipe out unrelated files. A folder that's already
/// gone is treated as success.
#[tauri::command]
pub async fn delete_game_from_disk(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    id: String,
) -> AppResult<()> {
    // Capture the Steam shortcut identity before the row is deleted below.
    let steam = capture_steam_identity(state.inner(), &id).await?;
    // Run-vs-wipe exclusion lives in `wipe_install_files` (the shared chokepoint),
    // so it covers this command, the uninstall path, and the Decky plugin server
    // uniformly — and across processes, which the in-process RunState can't.
    delete_game_core(state.inner(), &id).await?;
    reconcile_steam_removal(steam).await;
    if let Err(e) = app.emit("library:changed", &id) {
        tracing::warn!(error = %e, "failed to emit library:changed after delete_game_from_disk");
    }
    Ok(())
}

/// Removes a game's installed files from disk but keeps its library entry
/// (dimmed, Play disabled) so playtime / art / save backups survive and a
/// re-add reuses the same row. Backs the saves up first (the wipe also deletes
/// the Proton prefix, which holds live in-prefix save state) — see
/// [`crate::runner::uninstall_game_with_backup`]; a backup failure aborts the
/// uninstall so files are never wiped before their saves are captured.
#[tauri::command]
pub async fn uninstall_game(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    id: String,
) -> AppResult<()> {
    // The run-vs-wipe exclusion is enforced in `wipe_install_files` (reached via
    // uninstall_game_with_backup → uninstall_game_core), cross-process.
    let ludusavi_exe = crate::paths::resolve_ludusavi_path()
        .ok_or_else(|| AppError::Other("Ludusavi sidecar not found — reinstall Spool.".into()))?;
    let config_dir = crate::paths::ludusavi_config_dir();
    let ludusavi_client = app.state::<crate::ludusavi::LudusaviClient>();
    crate::runner::uninstall_game_with_backup(
        &ludusavi_client,
        &ludusavi_exe,
        &config_dir,
        state.inner(),
        &id,
    )
    .await?;
    if let Err(e) = app.emit("library:changed", &id) {
        tracing::warn!(error = %e, "failed to emit library:changed after uninstall_game");
    }
    Ok(())
}

/// Deletes a game's install folder (and, on Linux, its per-game Proton/Wine
/// prefix) from disk. Shared by [`delete_game_core`] (which then forgets the
/// entry) and [`uninstall_game_core`] (which keeps it). A missing/empty
/// `game_folder_path` is an error — both callers act on files on disk, so with
/// nothing to delete there's nothing to do (and marking an entry "removed from
/// disk" when no files were touched would be a lie). The Proton prefix is only
/// managed on Linux, so it's resolved + deleted there alone — a populated
/// `wine_prefix_path` override on Windows/macOS must never be recurse-deleted.
/// Best-effort prefix cleanup never aborts the operation.
///
/// Holds the machine-wide per-game run lock across the whole wipe so it can't
/// overlap a play session for this game in ANY Spool process (the run workflow
/// holds the same lock for the session). `None` ⇒ the game is currently running
/// (or another wipe is in flight) — refuse rather than delete files out from
/// under it. This is the single chokepoint, so every wipe caller (the
/// uninstall / delete commands and the Decky plugin server) is covered.
async fn wipe_install_files(library: &SharedLibrary, id: &str) -> AppResult<()> {
    let _run_lock = crate::proc_lock::try_acquire_run(id)?.ok_or_else(|| {
        AppError::Other(
            "This game is busy — it's running, or finishing a save backup. Close it and try again."
                .into(),
        )
    })?;

    // Capture the folder + prefix paths before any blocking IO.
    let (folder, prefix_root) = {
        let entry = library
            .find(id)
            .await?
            .ok_or_else(|| AppError::Other(format!("game with id {id} not found")))?;
        // Per-game Proton prefix: the override if set, else the default
        // `prefixes/<id>` under Spool's data dir.
        #[cfg(target_os = "linux")]
        let prefix_root = Some(
            entry
                .wine_prefix_path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| crate::proton::game_prefix_path(id)),
        );
        #[cfg(not(target_os = "linux"))]
        let prefix_root: Option<std::path::PathBuf> = None;
        (entry.game_folder_path.clone(), prefix_root)
    };

    let Some(folder) = folder.filter(|f| !f.trim().is_empty()) else {
        return Err(AppError::Other(
            "This game has no known install folder to delete.".to_string(),
        ));
    };
    // Recursive delete can be slow for a large game — run it off the async
    // runtime's worker threads.
    tokio::task::spawn_blocking(move || delete_install_dir(&folder))
        .await
        .map_err(|e| AppError::Other(format!("delete task join failed: {e}")))??;

    // Best-effort Proton prefix cleanup (Linux only) — never aborts. A missing
    // prefix (e.g. a never-launched game) is a no-op.
    if let Some(prefix_root) = prefix_root {
        let prefix_str = prefix_root.to_string_lossy().to_string();
        match tokio::task::spawn_blocking(move || delete_install_dir(&prefix_str)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(
                prefix = %prefix_root.display(),
                error = %e,
                "couldn't delete Proton prefix; leaving it in place",
            ),
            Err(e) => tracing::warn!(error = %e, "prefix delete task join failed"),
        }
    }
    Ok(())
}

/// Folder-delete + entry-removal shared by the [`delete_game_from_disk`]
/// command and the Decky plugin server's `DELETE /games/:id`. Does not emit
/// `library:changed` — the caller does that where a Tauri `AppHandle` exists.
pub async fn delete_game_core(library: &SharedLibrary, id: &str) -> AppResult<()> {
    wipe_install_files(library, id).await?;
    // Folder gone (or already absent) — now forget the entry.
    library.remove(id).await?;
    Ok(())
}

/// Removes a game's installed files from disk but KEEPS its library entry —
/// the "remove from disk, keep in library" option. Wipes the install folder
/// and Proton prefix like [`delete_game_core`], then flips `installed` off and
/// clears the now-stale install paths/size instead of deleting the row. The
/// catalog number, playtime, cover art, accent colour, and save backups all
/// survive, so re-adding the game (Add flow or LAN install) reuses this same
/// entry via [`Library::find_reusable_entry`]. Errors (leaving the entry
/// installed) when there's no install folder to delete — there's nothing to
/// remove from disk, so [`remove_game`] (forget) is the right action instead.
/// Does not emit `library:changed` — the caller does.
pub async fn uninstall_game_core(library: &SharedLibrary, id: &str) -> AppResult<()> {
    wipe_install_files(library, id).await?;
    library
        .update_fields(
            id,
            &[
                ("installed", json!(false)),
                ("game_folder_path", Value::Null),
                ("exe_path", json!("")),
                ("install_size_mb", json!(0)),
            ],
        )
        .await?;
    Ok(())
}

/// Recursively deletes `folder` after validating it's a safe target. A path
/// that doesn't exist is treated as already-deleted (`Ok`). See
/// [`delete_game_from_disk`] for the guard rationale.
fn delete_install_dir(folder: &str) -> AppResult<()> {
    let path = std::path::Path::new(folder);
    if !path.exists() {
        // Already gone — nothing to reclaim, and removing the entry is still
        // the right outcome.
        return Ok(());
    }
    if !path.is_dir() {
        return Err(AppError::Other(format!(
            "Install path is not a folder: {folder}"
        )));
    }
    // Resolve symlinks / `..` so the safety check sees the real target.
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| AppError::Other(format!("couldn't resolve {folder}: {e}")))?;
    if is_unsafe_delete_target(&canonical) {
        return Err(AppError::Other(format!(
            "Refusing to delete '{}' — it looks too broad to be a single game folder.",
            canonical.display()
        )));
    }
    std::fs::remove_dir_all(&canonical)
        .map_err(|e| AppError::Other(format!("couldn't delete {}: {e}", canonical.display())))?;
    Ok(())
}

/// True when `path` is too dangerous to recursively delete: fewer than two
/// path components below root, the user's home directory, or an ancestor of
/// (or equal to) Spool's own data directory.
fn is_unsafe_delete_target(path: &std::path::Path) -> bool {
    use std::path::Component;
    let normals = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if normals < 2 {
        return true;
    }
    if let Some(home) = dirs::home_dir() {
        if path == home {
            return true;
        }
    }
    // Never delete Spool's data dir, nor any ancestor that contains it.
    let app_data = paths::app_data_dir();
    if app_data.starts_with(path) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn achievement_table_from_before_sync_is_rebuilt_per_device() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        // The table as created before cross-device sync, with one row.
        sqlx::query(
            "CREATE TABLE achievement_unlocks (
                 game_name TEXT NOT NULL, api_name TEXT NOT NULL,
                 unlocked_at INTEGER NOT NULL, detected_at INTEGER NOT NULL,
                 live INTEGER NOT NULL, display_name TEXT, icon TEXT,
                 PRIMARY KEY (game_name, api_name))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO achievement_unlocks VALUES ('G', 'A', 1, 2, 1, 'Alpha', NULL)")
            .execute(&pool)
            .await
            .unwrap();
        let lib = Library { pool };
        lib.init_schema().await.unwrap();
        // Running it again is harmless.
        lib.init_schema().await.unwrap();

        let rows = lib.local_achievement_unlocks().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].display_name.as_deref(), Some("Alpha"));
        assert_eq!(rows[0].live, 1);
        // Another device's row for the same achievement now fits beside it.
        let peer = AchievementUnlock {
            device_id: "dev-b".into(),
            device_name: "Deck".into(),
            ..rows[0].clone()
        };
        assert_eq!(
            lib.fold_achievement_unlocks(&[peer], "dev-a")
                .await
                .unwrap(),
            1
        );
        assert_eq!(lib.achievement_unlocks_for("G").await.unwrap().len(), 2);
    }

    #[test]
    fn make_safe_filename_handles_basics() {
        assert_eq!(make_safe_filename("Hades II"), "Hades II");
        assert_eq!(make_safe_filename("Game: Bad/Chars?"), "Game Bad Chars");
        assert_eq!(make_safe_filename("  many   spaces  "), "many spaces");
        assert_eq!(make_safe_filename(""), "Game");
        assert_eq!(make_safe_filename("...."), "Game");
    }

    #[test]
    fn make_safe_filename_preserves_unicode() {
        assert_eq!(make_safe_filename("Tëst Gämé 旅"), "Tëst Gämé 旅");
    }

    #[test]
    fn unsafe_delete_target_rejects_shallow_paths() {
        // Root and one-component paths are too broad to be a game folder.
        assert!(is_unsafe_delete_target(std::path::Path::new("/")));
        assert!(is_unsafe_delete_target(std::path::Path::new("/games")));
        // Two components or deeper is fine.
        assert!(!is_unsafe_delete_target(std::path::Path::new(
            "/games/Hades"
        )));
        assert!(!is_unsafe_delete_target(std::path::Path::new(
            "/home/user/Games/Hades"
        )));
    }

    #[test]
    fn unsafe_delete_target_rejects_home_dir() {
        if let Some(home) = dirs::home_dir() {
            assert!(is_unsafe_delete_target(&home));
        }
    }

    #[test]
    fn delete_install_dir_noop_when_missing() {
        // A path that doesn't exist is treated as already-deleted.
        let missing = std::env::temp_dir().join("spool-delete-test-does-not-exist-xyz");
        assert!(delete_install_dir(&missing.to_string_lossy()).is_ok());
    }

    fn sample(id: &str, name: &str) -> GameEntry {
        GameEntry {
            id: id.to_string(),
            game_name: name.to_string(),
            safe_name: make_safe_filename(name),
            ..GameEntry::default()
        }
    }

    #[tokio::test]
    async fn insert_assigns_catalog_and_round_trips() {
        let lib = Library::open_in_memory().await.unwrap();
        let a = lib.insert(sample("a", "Hades")).await.unwrap();
        let b = lib.insert(sample("b", "Celeste")).await.unwrap();
        assert_eq!(a.catalog_number, 1);
        assert_eq!(b.catalog_number, 2);

        let all = lib.list().await.unwrap();
        assert_eq!(all.len(), 2);
        let found = lib.find("a").await.unwrap().unwrap();
        assert_eq!(found.game_name, "Hades");
        assert_eq!(
            lib.find_id_by_name("Celeste", None)
                .await
                .unwrap()
                .as_deref(),
            Some("b")
        );
    }

    fn psr(name: &str, status: Option<&str>, at: i64) -> PlayStatusRow {
        PlayStatusRow {
            game_name: name.into(),
            status: status.map(Into::into),
            updated_at: at,
        }
    }

    #[tokio::test]
    async fn play_status_reads_through_the_view_and_survives_re_add() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        lib.set_play_status("Hades", Some("playing"), 1_000)
            .await
            .unwrap();
        let g = lib.find("a").await.unwrap().unwrap();
        assert_eq!(g.play_status.as_deref(), Some("playing"));
        assert_eq!(g.play_status_updated_at, Some(1_000));

        // A whole-entry write can't change it.
        let mut edited = g.clone();
        edited.play_status = Some("dropped".into());
        lib.replace(&edited).await.unwrap();
        let g = lib.find("a").await.unwrap().unwrap();
        assert_eq!(g.play_status.as_deref(), Some("playing"));

        // Forget + re-add under the same name keeps it.
        lib.remove("a").await.unwrap();
        lib.insert(sample("b", "Hades")).await.unwrap();
        let all = lib.list().await.unwrap();
        assert_eq!(all[0].play_status.as_deref(), Some("playing"));
    }

    #[tokio::test]
    async fn set_play_status_stamps_past_a_later_adopted_change() {
        let lib = Library::open_in_memory().await.unwrap();
        // Adopted from a device whose clock is ahead of ours.
        lib.fold_play_statuses(&[psr("Hades", Some("finished"), 5_000)])
            .await
            .unwrap();
        let at = lib
            .set_play_status("Hades", Some("playing"), 1_000)
            .await
            .unwrap();
        assert_eq!(at, 5_001);
        // Re-folding the peer's (older) change doesn't revert ours.
        assert_eq!(
            lib.fold_play_statuses(&[psr("Hades", Some("finished"), 5_000)])
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            lib.play_statuses().await.unwrap(),
            vec![psr("Hades", Some("playing"), 5_001)]
        );
    }

    #[tokio::test]
    async fn fold_play_statuses_keeps_the_newest_including_clears_and_unknowns() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.set_play_status("Hades", Some("playing"), 1_000)
            .await
            .unwrap();
        let changed = lib
            .fold_play_statuses(&[
                psr("Hades", Some("dropped"), 500),
                psr("Celeste", Some("someday"), 10),
                psr("Tunic", None, 20),
                psr("", Some("playing"), 99),
            ])
            .await
            .unwrap();
        assert_eq!(changed, 2);
        assert_eq!(
            lib.play_statuses().await.unwrap(),
            vec![
                psr("Celeste", Some("someday"), 10),
                psr("Hades", Some("playing"), 1_000),
                psr("Tunic", None, 20),
            ]
        );
        lib.fold_play_statuses(&[psr("Hades", None, 2_000)])
            .await
            .unwrap();
        assert_eq!(
            lib.play_statuses().await.unwrap()[1],
            psr("Hades", None, 2_000)
        );
    }

    #[tokio::test]
    async fn rename_copies_the_play_status_and_renaming_back_keeps_the_latest() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.set_play_status("Hades", Some("playing"), 1)
            .await
            .unwrap();
        assert!(lib.copy_play_status("Hades", "Hades II", 10).await.unwrap());
        lib.set_play_status("Hades II", Some("finished"), 20)
            .await
            .unwrap();
        // Renaming back onto the old name carries the newer status over it.
        assert!(lib.copy_play_status("Hades II", "Hades", 30).await.unwrap());
        assert_eq!(
            lib.play_statuses().await.unwrap(),
            vec![
                psr("Hades", Some("finished"), 30),
                psr("Hades II", Some("finished"), 20),
            ]
        );
        // Nothing to copy for a game without a status.
        assert!(!lib.copy_play_status("Tunic", "Tunic 2", 40).await.unwrap());
    }

    #[tokio::test]
    async fn fold_play_statuses_breaks_a_timestamp_tie_the_same_way_everywhere() {
        let a = Library::open_in_memory().await.unwrap();
        let b = Library::open_in_memory().await.unwrap();
        a.fold_play_statuses(&[psr("Hades", Some("playing"), 5)])
            .await
            .unwrap();
        b.fold_play_statuses(&[psr("Hades", Some("dropped"), 5)])
            .await
            .unwrap();
        a.fold_play_statuses(&[psr("Hades", Some("dropped"), 5)])
            .await
            .unwrap();
        b.fold_play_statuses(&[psr("Hades", Some("playing"), 5)])
            .await
            .unwrap();
        assert_eq!(
            a.play_statuses().await.unwrap(),
            b.play_statuses().await.unwrap()
        );
        assert_eq!(
            a.play_statuses().await.unwrap(),
            vec![psr("Hades", Some("playing"), 5)]
        );
    }

    #[tokio::test]
    async fn remove_deletes_and_bumps_version() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        let v0 = lib.version().await.unwrap();
        assert!(lib.remove("a").await.unwrap());
        assert!(!lib.remove("a").await.unwrap()); // already gone
        assert!(lib.version().await.unwrap() > v0);
        assert_eq!(lib.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn remove_cleans_up_run_lock_file() {
        // Retiring an entry deletes its per-game run-lock marker so the files
        // don't accumulate. Unique id so the marker can't collide with others.
        let lib = Library::open_in_memory().await.unwrap();
        let id = "test-remove-cleans-lock-3c9d";
        lib.insert(sample(id, "Hades")).await.unwrap();

        // Materialise the marker (created on acquire), then release the lock —
        // dropping the guard unlocks but leaves the file in place.
        drop(crate::proc_lock::try_acquire_run(id).unwrap());
        let lock_path = crate::paths::run_lock_file(id);
        assert!(lock_path.exists(), "marker exists before removal");

        assert!(lib.remove(id).await.unwrap());
        assert!(
            !lock_path.exists(),
            "run-lock marker removed when the entry is retired"
        );
    }

    #[tokio::test]
    async fn targeted_update_does_not_clobber_other_fields() {
        // A backup-stats write must not lose a concurrent playtime bump — the
        // whole point of the SQLite move. Simulate the two writes interleaving.
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();

        give_playtime(&lib, "Hades", 30).await;
        lib.record_backup_stats("a", 3, Some(Utc::now()), Some(12.5))
            .await
            .unwrap();

        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.playtime_minutes, 30); // survived the backup write
        assert_eq!(e.save_backup_count, 3);
        assert_eq!(e.save_backup_size_mb, 12.5);
    }

    #[tokio::test]
    async fn replace_preserves_runtime_fields() {
        // The editor saving an entry must not wipe playtime/backup counters set
        // by the run workflow after the editor loaded its (stale) copy.
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        give_playtime(&lib, "Hades", 45).await;

        // Editor loaded the entry before the session and saves a renamed copy
        // whose playtime is still 0.
        let mut edited = sample("a", "Hades Renamed");
        edited.playtime_minutes = 0;
        assert!(lib.replace(&edited).await.unwrap());

        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.game_name, "Hades Renamed"); // editor change applied
        assert_eq!(e.playtime_minutes, 45); // runtime field preserved
        assert_eq!(
            lib.find_id_by_name("Hades Renamed", None)
                .await
                .unwrap()
                .as_deref(),
            Some("a")
        );
    }

    #[tokio::test]
    async fn find_id_by_name_disambiguates_by_exe_path() {
        let lib = Library::open_in_memory().await.unwrap();
        let mut g1 = sample("a", "Hades");
        g1.exe_path = "/games/Hades_Steam/hades.exe".to_string();
        lib.insert(g1).await.unwrap();

        let mut g2 = sample("b", "Hades");
        g2.exe_path = "/games/Hades_Epic/hades.exe".to_string();
        lib.insert(g2).await.unwrap();

        // Exact match for the second one:
        assert_eq!(
            lib.find_id_by_name("Hades", Some("/games/Hades_Epic/hades.exe"))
                .await
                .unwrap()
                .as_deref(),
            Some("b")
        );
        // Exact match for the first one:
        assert_eq!(
            lib.find_id_by_name("Hades", Some("/games/Hades_Steam/hades.exe"))
                .await
                .unwrap()
                .as_deref(),
            Some("a")
        );
        // No exe_path provided: falls back to catalog_number (oldest first, i.e., "a")
        assert_eq!(
            lib.find_id_by_name("Hades", None).await.unwrap().as_deref(),
            Some("a")
        );
        // Mismatched exe_path: still falls back to catalog_number (oldest first, i.e., "a")
        assert_eq!(
            lib.find_id_by_name("Hades", Some("/other/hades.exe"))
                .await
                .unwrap()
                .as_deref(),
            Some("a")
        );
    }

    #[tokio::test]
    async fn replace_preserves_resolved_steam_id() {
        // An untracked add resolves a steam id asynchronously (Steam store
        // search) and persists it via update_fields. An editor opened before
        // that resolve landed has steam_id: None; saving it must not clobber the
        // resolved id, so steam_id is overlaid from the existing row like other
        // out-of-band fields.
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Game")).await.unwrap();
        lib.update_fields("a", &[("steam_id", json!(1145360u64))])
            .await
            .unwrap();

        let mut edited = sample("a", "Game"); // editor's stale copy, steam_id None
        assert!(edited.steam_id.is_none());
        assert!(lib.replace(&edited).await.unwrap());
        assert_eq!(
            lib.find("a").await.unwrap().unwrap().steam_id,
            Some(1145360)
        );

        // A genuine identity change still applies the rest of the editor's edits.
        edited.game_name = "Renamed".into();
        assert!(lib.replace(&edited).await.unwrap());
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.game_name, "Renamed");
        assert_eq!(e.steam_id, Some(1145360));
    }

    #[tokio::test]
    async fn set_if_empty_guards() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();

        assert!(lib.set_accent_if_empty("a", "#ff0000").await.unwrap());
        // Second call no-ops because accent is already set.
        assert!(!lib.set_accent_if_empty("a", "#00ff00").await.unwrap());
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.accent_color.as_deref(), Some("#ff0000"));

        assert!(lib.set_install_size_if_empty("a", 500.0).await.unwrap());
        assert!(!lib.set_install_size_if_empty("a", 999.0).await.unwrap());
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.install_size_mb, 500.0);
    }

    /// Give a game `mins` of playtime the way the app now does it: record a
    /// session, then derive the cached totals from the timeline.
    async fn give_playtime(lib: &Library, game: &str, mins: i64) {
        let s = session(
            &format!("seed:{game}:{mins}"),
            "seed-device",
            game,
            "2026-01-01T00:00:00Z",
            mins,
        );
        lib.insert_session(&s).await.unwrap();
        lib.recompute_playtime(game).await.unwrap();
    }

    fn session(id: &str, device: &str, game: &str, start: &str, mins: i64) -> PlaySession {
        let started = DateTime::parse_from_rfc3339(start)
            .unwrap()
            .with_timezone(&Utc);
        PlaySession {
            session_id: id.to_string(),
            device_id: device.to_string(),
            device_name: format!("{device}-name"),
            game_name: game.to_string(),
            started_at: started,
            ended_at: started + chrono::Duration::minutes(mins),
            duration_secs: mins * 60,
        }
    }

    #[tokio::test]
    async fn recompute_playtime_sums_devices_and_takes_max_end() {
        // Playtime is the summed session duration across every device (peer rows
        // are folded into the same table); last-played is the latest session end.
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        lib.insert_session(&session(
            "deck:1",
            "deck",
            "Hades",
            "2026-05-01T10:00:00Z",
            20,
        ))
        .await
        .unwrap();
        lib.insert_session(&session("pc:1", "pc", "Hades", "2026-05-03T10:00:00Z", 25))
            .await
            .unwrap();

        assert!(lib.recompute_playtime("Hades").await.unwrap());
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.playtime_minutes, 45, "summed across devices");
        assert_eq!(
            e.last_played_at.unwrap(),
            DateTime::parse_from_rfc3339("2026-05-03T10:25:00Z")
                .unwrap()
                .with_timezone(&Utc),
            "latest session end",
        );
    }

    #[tokio::test]
    async fn recompute_playtime_noop_without_sessions() {
        // No sessions ⇒ leave the cached value untouched rather than zeroing it.
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        assert!(!lib.recompute_playtime("Hades").await.unwrap());
        assert_eq!(lib.find("a").await.unwrap().unwrap().playtime_minutes, 0);
    }

    #[tokio::test]
    async fn legacy_playtime_migration_seeds_the_gap() {
        // A library upgraded from the accumulator era: cached playtime that
        // predates the sessions table, partly covered by later real sessions.
        let lib = Library::open_in_memory().await.unwrap();
        let mut g = sample("a", "Hades");
        g.playtime_minutes = 120;
        g.last_played_at = Some(
            DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        lib.insert(g).await.unwrap();
        lib.insert_session(&session(
            "deck:1",
            "deck",
            "Hades",
            "2025-06-01T00:00:00Z",
            30,
        ))
        .await
        .unwrap();

        // open() already ran the once-guarded migration on the (empty) DB, so
        // clear the flag to exercise it against the seeded game.
        sqlx::query("DELETE FROM meta WHERE k = 'playtime_migrated'")
            .execute(&lib.pool)
            .await
            .unwrap();
        lib.migrate_legacy_playtime().await.unwrap();

        // The 90-min gap (120 − 30) is seeded as one synthetic session, so the
        // derived total matches the previously-displayed value.
        lib.recompute_playtime("Hades").await.unwrap();
        assert_eq!(lib.find("a").await.unwrap().unwrap().playtime_minutes, 120);

        // Idempotent: the flag is set again, so a second run seeds nothing.
        lib.migrate_legacy_playtime().await.unwrap();
        lib.recompute_playtime("Hades").await.unwrap();
        assert_eq!(lib.find("a").await.unwrap().unwrap().playtime_minutes, 120);
    }

    #[tokio::test]
    async fn insert_session_is_idempotent_by_id() {
        let lib = Library::open_in_memory().await.unwrap();
        let s = session("deck:1", "deck", "Hades", "2026-05-01T10:00:00Z", 30);
        assert!(lib.insert_session(&s).await.unwrap(), "first insert is new");
        assert!(!lib.insert_session(&s).await.unwrap(), "same id is a no-op");
        assert_eq!(lib.list_sessions(None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn list_sessions_filters_by_game_and_orders_by_start() {
        let lib = Library::open_in_memory().await.unwrap();
        // Insert out of chronological order to prove ORDER BY started_at.
        lib.insert_session(&session(
            "deck:2",
            "deck",
            "Hades",
            "2026-05-02T10:00:00Z",
            10,
        ))
        .await
        .unwrap();
        lib.insert_session(&session(
            "deck:1",
            "deck",
            "Hades",
            "2026-05-01T10:00:00Z",
            20,
        ))
        .await
        .unwrap();
        lib.insert_session(&session("pc:1", "pc", "Celeste", "2026-05-01T12:00:00Z", 5))
            .await
            .unwrap();

        let hades = lib.list_sessions(Some("Hades")).await.unwrap();
        assert_eq!(hades.len(), 2);
        assert_eq!(hades[0].session_id, "deck:1", "oldest first");
        assert_eq!(hades[1].session_id, "deck:2");

        assert_eq!(lib.list_sessions(None).await.unwrap().len(), 3, "all games");
    }

    #[tokio::test]
    async fn sessions_for_device_scopes_to_one_device() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert_session(&session(
            "deck:1",
            "deck",
            "Hades",
            "2026-05-01T10:00:00Z",
            20,
        ))
        .await
        .unwrap();
        lib.insert_session(&session("pc:1", "pc", "Hades", "2026-05-01T12:00:00Z", 5))
            .await
            .unwrap();
        let deck = lib.sessions_for_device("deck").await.unwrap();
        assert_eq!(deck.len(), 1);
        assert_eq!(deck[0].device_id, "deck");
    }

    #[tokio::test]
    async fn upsert_sessions_skips_existing_and_counts_new() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert_session(&session(
            "deck:1",
            "deck",
            "Hades",
            "2026-05-01T10:00:00Z",
            20,
        ))
        .await
        .unwrap();
        // Folding a peer batch that re-includes deck:1 only adds the new rows.
        let batch = [
            session("deck:1", "deck", "Hades", "2026-05-01T10:00:00Z", 20),
            session("pc:1", "pc", "Hades", "2026-05-02T10:00:00Z", 15),
            session("pc:2", "pc", "Celeste", "2026-05-03T10:00:00Z", 5),
        ];
        assert_eq!(
            lib.upsert_sessions(&batch).await.unwrap(),
            2,
            "only the two new rows"
        );
        assert_eq!(lib.list_sessions(None).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn custom_save_dedups_and_survives_editor_save() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();

        // Duplicate paths are deduped on write so the UI's keyed list can't crash.
        let cs = CustomSave {
            files: vec![
                "<winLocalAppData>/Hades".into(),
                "<winLocalAppData>/Hades".into(),
                "<home>/Saved Games/Hades".into(),
            ],
            registry: vec![],
        };
        assert!(lib.set_custom_save("a", Some(&cs)).await.unwrap());
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(
            e.custom_save.unwrap().files,
            vec![
                "<winLocalAppData>/Hades".to_string(),
                "<home>/Saved Games/Hades".to_string(),
            ]
        );

        // custom_save is a runtime field — a whole-entry editor save re-overlays it.
        assert!(lib.replace(&sample("a", "Hades Renamed")).await.unwrap());
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.game_name, "Hades Renamed");
        assert!(
            e.custom_save.is_some(),
            "custom_save preserved across editor save"
        );
    }

    #[tokio::test]
    async fn set_custom_save_if_absent_is_conditional() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        let a = CustomSave {
            files: vec!["<winLocalAppData>/A".into()],
            registry: vec![],
        };
        let b = CustomSave {
            files: vec!["<winLocalAppData>/B".into()],
            registry: vec![],
        };

        // First adopt wins; a later/racing adopt can't clobber it.
        assert!(lib.set_custom_save_if_absent("a", &a).await.unwrap());
        assert!(!lib.set_custom_save_if_absent("a", &b).await.unwrap());
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(
            e.custom_save.unwrap().files,
            vec!["<winLocalAppData>/A".to_string()]
        );

        // Once cleared, it's "absent" again and can be set.
        assert!(lib.set_custom_save("a", None).await.unwrap());
        assert!(lib.set_custom_save_if_absent("a", &b).await.unwrap());
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(
            e.custom_save.unwrap().files,
            vec!["<winLocalAppData>/B".to_string()]
        );
    }

    #[tokio::test]
    async fn installed_defaults_true_for_legacy_rows() {
        // A row whose JSON predates the `installed` field must load as installed
        // (the container-level serde default uses GameEntry::default()), so an
        // upgrade doesn't grey out everyone's library.
        let lib = Library::open_in_memory().await.unwrap();
        // Insert raw JSON missing `installed`.
        let data = r#"{"id":"a","catalog_number":1,"game_name":"Hades","exe_path":"/x/h.exe"}"#;
        sqlx::query(
            "INSERT INTO games (id, catalog_number, game_name, data) VALUES ('a', 1, 'Hades', ?1)",
        )
        .bind(data)
        .execute(&lib.pool)
        .await
        .unwrap();
        let e = lib.find("a").await.unwrap().unwrap();
        assert!(
            e.installed,
            "legacy row without the field defaults to installed"
        );
    }

    #[tokio::test]
    async fn uninstall_marks_entry_and_clears_paths() {
        // "Remove from disk, keep in library": the folder is deleted but the
        // entry survives with installed=false, paths cleared, and all the
        // catalogue identity (catalog_number, playtime) intact.
        let lib: SharedLibrary = Arc::new(Library::open_in_memory().await.unwrap());

        // A real folder on disk to be wiped.
        let dir = std::env::temp_dir()
            .join("spool-uninstall-test-keep")
            .join("Hades");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("game.exe"), b"x").unwrap();

        // A unique id so the per-game run lock can't collide with other tests.
        let mut g = sample("uninst-keep", "Hades");
        g.exe_path = dir.join("game.exe").to_string_lossy().to_string();
        g.game_folder_path = Some(dir.to_string_lossy().to_string());
        g.install_size_mb = 1234.0;
        lib.insert(g).await.unwrap();
        give_playtime(&lib, "Hades", 50).await;

        uninstall_game_core(&lib, "uninst-keep").await.unwrap();

        assert!(!dir.exists(), "install folder deleted");
        let e = lib.find("uninst-keep").await.unwrap().unwrap();
        assert!(!e.installed, "entry kept but marked uninstalled");
        assert_eq!(e.game_folder_path, None);
        assert_eq!(e.exe_path, "");
        assert_eq!(e.install_size_mb, 0.0);
        assert_eq!(e.catalog_number, 1, "catalog number preserved");
        assert_eq!(e.playtime_minutes, 50, "playtime preserved");

        // cleanup
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("spool-uninstall-test-keep"));
    }

    #[tokio::test]
    async fn uninstall_errors_without_folder_and_keeps_entry_installed() {
        // No install folder → nothing to remove from disk, so uninstall errors
        // and the entry stays installed (marking it "removed from disk" when
        // nothing was deleted would be a lie — "remove from library" is right).
        let lib: SharedLibrary = Arc::new(Library::open_in_memory().await.unwrap());
        // A unique id so the per-game run lock can't collide with other tests.
        let mut g = sample("uninst-nofolder", "Hades");
        g.game_folder_path = None;
        g.exe_path = "/gone/h.exe".into();
        lib.insert(g).await.unwrap();

        assert!(uninstall_game_core(&lib, "uninst-nofolder").await.is_err());
        let e = lib.find("uninst-nofolder").await.unwrap().unwrap();
        assert!(
            e.installed,
            "entry stays installed when nothing was deleted"
        );
    }

    #[tokio::test]
    async fn uninstall_keeps_run_lock_file() {
        // The mirror of `remove_cleans_up_run_lock_file`: uninstall keeps the
        // entry (it may be reinstalled + run again), so its run-lock marker must
        // SURVIVE. Guards against a regression that swaps uninstall_game_core's
        // update_fields for a remove (which would delete the marker).
        let lib: SharedLibrary = Arc::new(Library::open_in_memory().await.unwrap());
        let dir = std::env::temp_dir()
            .join("spool-uninstall-keeps-lock")
            .join("Hades");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("game.exe"), b"x").unwrap();
        let id = "uninst-keeps-lock-d41e";
        let mut g = sample(id, "Hades");
        g.exe_path = dir.join("game.exe").to_string_lossy().to_string();
        g.game_folder_path = Some(dir.to_string_lossy().to_string());
        lib.insert(g).await.unwrap();

        uninstall_game_core(&lib, id).await.unwrap();

        // wipe_install_files acquires the run lock, materialising the marker; it
        // must still be present (uninstall uses update_fields, not remove).
        let lock_path = crate::paths::run_lock_file(id);
        assert!(
            lock_path.exists(),
            "uninstall keeps the run-lock marker for reinstall/re-run"
        );
        assert!(lib.find(id).await.unwrap().is_some(), "entry kept");

        let _ = std::fs::remove_file(&lock_path);
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("spool-uninstall-keeps-lock"));
    }

    #[tokio::test]
    async fn find_reusable_entry_prefers_steam_id_and_gates_on_uninstalled() {
        let lib = Library::open_in_memory().await.unwrap();

        // Installed entry with the same steam id must NOT be reused.
        let mut installed = sample("inst", "Hades");
        installed.steam_id = Some(1145360);
        lib.insert(installed).await.unwrap();
        assert!(
            lib.find_reusable_entry(Some(1145360), "Hades")
                .await
                .unwrap()
                .is_none(),
            "an installed entry is never offered for reuse"
        );

        // Uninstalled entry, different name, same steam id → matched by steam id.
        let mut uninst = sample("uninst", "Hades Classic");
        uninst.steam_id = Some(1145360);
        uninst.installed = false;
        lib.insert(uninst).await.unwrap();
        let hit = lib
            .find_reusable_entry(Some(1145360), "Hades")
            .await
            .unwrap();
        assert_eq!(hit.unwrap().id, "uninst", "matched by steam id over name");

        // Name fallback when no steam id given.
        let mut named = sample("named", "Celeste");
        named.installed = false;
        lib.insert(named).await.unwrap();
        let hit = lib.find_reusable_entry(None, "Celeste").await.unwrap();
        assert_eq!(hit.unwrap().id, "named");

        // No match → None.
        assert!(lib
            .find_reusable_entry(None, "Nonexistent")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn find_reusable_entry_name_fallback_rejects_steam_id_conflict() {
        let lib = Library::open_in_memory().await.unwrap();
        // Uninstalled "Doom" with a steam id.
        let mut doom = sample("doom", "Doom");
        doom.installed = false;
        doom.steam_id = Some(379720);
        lib.insert(doom).await.unwrap();

        // A *different* game also named "Doom" with a different steam id must
        // NOT reuse it (would merge two distinct games into one entry).
        assert!(
            lib.find_reusable_entry(Some(2371630), "Doom")
                .await
                .unwrap()
                .is_none(),
            "name match rejected on a positive steam-id conflict"
        );
        // No requested steam id → no conflict → reuse by name.
        assert_eq!(
            lib.find_reusable_entry(None, "Doom")
                .await
                .unwrap()
                .unwrap()
                .id,
            "doom"
        );
        // An untracked uninstalled entry (no steam id) is still reusable by name
        // even when the request carries a steam id (no positive conflict).
        let mut quake = sample("quake", "Quake");
        quake.installed = false;
        quake.steam_id = None;
        lib.insert(quake).await.unwrap();
        assert_eq!(
            lib.find_reusable_entry(Some(2310), "Quake")
                .await
                .unwrap()
                .unwrap()
                .id,
            "quake"
        );
    }

    #[tokio::test]
    async fn replace_preserves_installed_flag() {
        // A stale open editor saving must not resurrect an uninstalled game:
        // `installed` is a runtime field re-overlaid by replace().
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        // Mark uninstalled out-of-band (as uninstall_game_core does).
        lib.update_fields("a", &[("installed", json!(false))])
            .await
            .unwrap();

        // Editor loaded the entry while installed and saves with installed=true.
        let mut edited = sample("a", "Hades");
        edited.installed = true;
        assert!(lib.replace(&edited).await.unwrap());

        let e = lib.find("a").await.unwrap().unwrap();
        assert!(
            !e.installed,
            "live uninstalled state survives an editor save"
        );
    }

    #[tokio::test]
    async fn replace_keeps_cleared_install_paths_when_uninstalled() {
        // A stale editor opened while the game was installed must not resurrect
        // exe_path / game_folder_path on save once the live row is uninstalled.
        let lib = Library::open_in_memory().await.unwrap();
        let mut g = sample("a", "Hades");
        g.exe_path = "/games/Hades/h.exe".into();
        g.game_folder_path = Some("/games/Hades".into());
        lib.insert(g).await.unwrap();
        // Live row uninstalled out-of-band: installed=false, paths cleared.
        lib.update_fields(
            "a",
            &[
                ("installed", json!(false)),
                ("game_folder_path", Value::Null),
                ("exe_path", json!("")),
            ],
        )
        .await
        .unwrap();

        // Editor saves its stale (installed-era) snapshot.
        let mut stale = sample("a", "Hades");
        stale.installed = true;
        stale.exe_path = "/games/Hades/h.exe".into();
        stale.game_folder_path = Some("/games/Hades".into());
        assert!(lib.replace(&stale).await.unwrap());

        let e = lib.find("a").await.unwrap().unwrap();
        assert!(!e.installed, "stays uninstalled");
        assert_eq!(e.exe_path, "", "cleared exe_path not resurrected");
        assert_eq!(
            e.game_folder_path, None,
            "cleared game_folder_path not resurrected"
        );
    }

    #[tokio::test]
    async fn replace_applies_path_edits_for_installed_game() {
        // For an installed game the editor's browse-button path edits must still
        // win — the uninstalled-path guard only applies when the live row is off.
        let lib = Library::open_in_memory().await.unwrap();
        let mut g = sample("a", "Hades");
        g.exe_path = "/old/h.exe".into();
        g.game_folder_path = Some("/old".into());
        lib.insert(g).await.unwrap();

        let mut edited = sample("a", "Hades");
        edited.exe_path = "/new/h.exe".into();
        edited.game_folder_path = Some("/new".into());
        assert!(lib.replace(&edited).await.unwrap());

        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(
            e.exe_path, "/new/h.exe",
            "editor path edit applies when installed"
        );
        assert_eq!(e.game_folder_path.as_deref(), Some("/new"));
    }

    #[tokio::test]
    async fn replace_keeps_every_runtime_field_of_the_live_row() {
        // The editor's copy holds defaults for every out-of-band field; none
        // of the live values may be lost, whatever its type.
        let lib = Library::open_in_memory().await.unwrap();
        let when = DateTime::parse_from_rfc3339("2026-03-04T05:06:07Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut live = sample("a", "Hades");
        live.last_played_at = Some(when);
        live.playtime_minutes = 77;
        live.save_backup_count = 4;
        live.save_last_backed_up_at = Some(when);
        live.save_backup_size_mb = 3.5;
        live.sync_badge = Some("cloud-newer".into());
        live.cloud_sync_baseline = Some("backup-1".into());
        live.save_last_backer_device = Some("Deck".into());
        live.save_cloud_revision_at = Some(when);
        live.accent_color = Some("#112233".into());
        live.install_size_mb = 900.0;
        live.cover_image_path = Some("c.jpg".into());
        live.hero_image_path = Some("h.jpg".into());
        live.custom_save = Some(CustomSave {
            files: vec!["<base>/Saves".into()],
            registry: vec![],
        });
        live.manifest_override = Some(ManifestOverride {
            excluded_tags: vec!["config".into()],
            excluded_paths: vec![],
        });
        live.steam_id = Some(1145360);
        live.metadata_fetched = true;
        live.installed = true;
        live.steam_app_id = Some(42);
        lib.insert(live.clone()).await.unwrap();

        let mut stale = sample("a", "Hades");
        stale.installed = true;
        assert!(lib.replace(&stale).await.unwrap());

        let after = serde_json::to_value(lib.find("a").await.unwrap().unwrap()).unwrap();
        let before = serde_json::to_value(&live).unwrap();
        for f in RUNTIME_FIELDS {
            assert_eq!(after[f], before[f], "runtime field {f} was clobbered");
        }
    }

    #[tokio::test]
    async fn replace_of_a_missing_id_matches_nothing() {
        let lib = Library::open_in_memory().await.unwrap();
        assert!(!lib.replace(&sample("ghost", "Nope")).await.unwrap());
        assert_eq!(lib.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn update_fields_edge_cases() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        // Nothing to write, and an unknown id, both report no match.
        assert!(!lib.update_fields("a", &[]).await.unwrap());
        assert!(!lib
            .update_fields("ghost", &[("sync_badge", json!("synced"))])
            .await
            .unwrap());

        // Several fields in one call, with a null clearing an Option.
        lib.set_sync_badge("a", "synced").await.unwrap();
        lib.update_fields(
            "a",
            &[
                ("sync_badge", Value::Null),
                ("genres", json!(["Roguelike", "Action"])),
                ("playtime_minutes", json!(12)),
            ],
        )
        .await
        .unwrap();
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.sync_badge, None);
        assert_eq!(e.genres, vec!["Roguelike", "Action"]);
        assert_eq!(e.playtime_minutes, 12);
    }

    #[tokio::test]
    async fn backup_stats_without_a_size_keep_the_recorded_size() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        lib.record_backup_stats("a", 2, Some(Utc::now()), Some(8.0))
            .await
            .unwrap();
        lib.record_backup_stats("a", 3, None, None).await.unwrap();
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.save_backup_count, 3);
        assert_eq!(e.save_backup_size_mb, 8.0);
        assert_eq!(e.save_last_backed_up_at, None, "None clears the timestamp");
    }

    #[tokio::test]
    async fn set_art_only_touches_the_given_fields() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        lib.set_art("a", Some("c.jpg"), Some("h.jpg"), Some("#fff"))
            .await
            .unwrap();
        lib.set_art("a", None, Some("h2.jpg"), None).await.unwrap();
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.cover_image_path.as_deref(), Some("c.jpg"));
        assert_eq!(e.hero_image_path.as_deref(), Some("h2.jpg"));
        assert_eq!(e.accent_color.as_deref(), Some("#fff"));
        // No fields at all is a no-op, not an error.
        assert!(!lib.set_art("a", None, None, None).await.unwrap());
    }

    #[tokio::test]
    async fn conditional_backfill_writes_only_when_expected() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();

        assert!(lib.set_accent_if_empty("a", "#111").await.unwrap());
        assert!(!lib.set_accent_if_empty("a", "#222").await.unwrap());
        assert!(!lib.replace_accent("a", "#999", "#333").await.unwrap());
        assert!(lib.replace_accent("a", "#111", "#333").await.unwrap());
        assert_eq!(
            lib.find("a")
                .await
                .unwrap()
                .unwrap()
                .accent_color
                .as_deref(),
            Some("#333")
        );

        assert!(lib.set_install_size_if_empty("a", 10.0).await.unwrap());
        assert!(!lib.set_install_size_if_empty("a", 99.0).await.unwrap());
        assert_eq!(lib.find("a").await.unwrap().unwrap().install_size_mb, 10.0);
    }

    #[tokio::test]
    async fn recompute_playtime_does_not_bump_version_when_unchanged() {
        // The startup fold calls this for every game; a same-value write would
        // make every start emit library:changed.
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        lib.insert_session(&session("d:1", "d", "Hades", "2026-05-01T10:00:00Z", 10))
            .await
            .unwrap();
        assert!(lib.recompute_playtime("Hades").await.unwrap());
        let v = lib.version().await.unwrap();
        assert!(!lib.recompute_playtime("Hades").await.unwrap());
        assert_eq!(lib.version().await.unwrap(), v);
        assert_eq!(lib.recompute_all_playtime().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn recompute_playtime_floors_to_minutes_and_updates_every_matching_row() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();
        lib.insert(sample("b", "Hades")).await.unwrap();
        lib.insert(sample("c", "Celeste")).await.unwrap();
        let mut s = session("d:1", "d", "Hades", "2026-05-01T10:00:00Z", 1);
        s.duration_secs = 119;
        lib.insert_session(&s).await.unwrap();

        assert_eq!(
            lib.recompute_all_playtime().await.unwrap(),
            1,
            "counts games, not rows"
        );
        assert_eq!(lib.find("a").await.unwrap().unwrap().playtime_minutes, 1);
        assert_eq!(lib.find("b").await.unwrap().unwrap().playtime_minutes, 1);
        assert_eq!(lib.find("c").await.unwrap().unwrap().playtime_minutes, 0);
    }

    #[tokio::test]
    async fn insert_keeps_an_explicit_catalog_number() {
        let lib = Library::open_in_memory().await.unwrap();
        let mut g = sample("a", "Hades");
        g.catalog_number = 40;
        assert_eq!(lib.insert(g).await.unwrap().catalog_number, 40);
        let next = lib.insert(sample("b", "Celeste")).await.unwrap();
        assert_eq!(next.catalog_number, 41);
    }

    #[test]
    fn backfill_catalog_numbers_keeps_existing_and_fills_gaps_above_max() {
        let mut v = vec![sample("a", "A"), sample("b", "B"), sample("c", "C")];
        v[1].catalog_number = 7;
        backfill_catalog_numbers(&mut v);
        let nums: Vec<u32> = v.iter().map(|e| e.catalog_number).collect();
        assert_eq!(nums, vec![8, 7, 9]);
        let mut empty: Vec<GameEntry> = vec![];
        backfill_catalog_numbers(&mut empty);
    }

    #[test]
    fn dedup_preserve_order_keeps_first_occurrence() {
        let v: Vec<String> = ["b", "a", "b", "c", "a"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(dedup_preserve_order(&v), vec!["b", "a", "c"]);
    }

    #[test]
    fn unsafe_delete_target_rejects_ancestors_of_the_data_dir() {
        let data = paths::app_data_dir();
        assert!(is_unsafe_delete_target(&data));
        if let Some(parent) = data.parent() {
            assert!(is_unsafe_delete_target(parent));
        }
        // A sibling that merely shares a name prefix is not an ancestor.
        let sibling = std::path::PathBuf::from(format!("{}-games/Hades", data.display()));
        assert!(!is_unsafe_delete_target(&sibling));
    }

    #[tokio::test]
    async fn replace_survives_legacy_row_missing_installed() {
        // A row written before `installed` existed lacks the key. The editor
        // save (replace) must NOT write `"installed":null` into it — a
        // non-Option `bool` can't deserialize null, which would corrupt the row
        // (and break list() for the whole library). The COALESCE fallback uses
        // the incoming entry's value instead, and a present-null Option
        // (`sync_badge`) must still be preserved as null, not clobbered.
        let lib = Library::open_in_memory().await.unwrap();
        let data = r#"{"id":"a","catalog_number":1,"game_name":"Hades","exe_path":"/x/h.exe","playtime_minutes":42,"sync_badge":null}"#;
        sqlx::query(
            "INSERT INTO games (id, catalog_number, game_name, data) VALUES ('a', 1, 'Hades', ?1)",
        )
        .bind(data)
        .execute(&lib.pool)
        .await
        .unwrap();

        // Editor loads + renames + saves (its GameEntry carries installed=true).
        assert!(lib.replace(&sample("a", "Hades Renamed")).await.unwrap());

        // Row still deserializes — no `"installed":null` corruption.
        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.game_name, "Hades Renamed");
        assert!(
            e.installed,
            "missing-key installed falls back to incoming true, not null"
        );
        assert_eq!(e.playtime_minutes, 42, "present runtime field preserved");
        assert_eq!(
            e.sync_badge, None,
            "present-null Option preserved, not clobbered"
        );
    }
}

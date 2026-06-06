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
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

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
            accent_color: None,
            sync_badge: None,
            cloud_sync_baseline: None,
            save_last_backer_device: None,
            save_cloud_revision_at: None,
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
];

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
        ];
        for sql in stmts {
            sqlx::query(sql).execute(&self.pool).await?;
        }
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

    /// All games, ordered by catalog number (their stable shelf order).
    pub async fn list(&self) -> AppResult<Vec<GameEntry>> {
        let rows = sqlx::query("SELECT data FROM games ORDER BY catalog_number")
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
        let row = sqlx::query("SELECT data FROM games WHERE id = ?1")
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
    pub async fn find_id_by_name(&self, name: &str) -> AppResult<Option<String>> {
        let row = sqlx::query("SELECT id FROM games WHERE game_name = ?1 LIMIT 1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("id")))
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
        sqlx::query("INSERT INTO games (id, catalog_number, game_name, data) VALUES (?1, ?2, ?3, ?4)")
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
        // Build json_set(?1, '$.f', json_extract(data,'$.f'), …) nesting so the
        // incoming blob (?1) keeps the existing row's runtime field values.
        let mut expr = "?1".to_string();
        for f in RUNTIME_FIELDS {
            expr = format!("json_set({expr}, '$.{f}', json_extract(data, '$.{f}'))");
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

    /// Removes an entry by id. Returns whether a row was deleted.
    pub async fn remove(&self, id: &str) -> AppResult<bool> {
        let res = sqlx::query("DELETE FROM games WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
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

    /// Records a finished play session: adds `minutes` to playtime and sets
    /// `last_played_at`. The increment is done in SQL so two processes can't
    /// lose each other's minutes.
    pub async fn bump_session(
        &self,
        id: &str,
        last_played: DateTime<Utc>,
        minutes: i32,
    ) -> AppResult<bool> {
        let sql = "UPDATE games SET data = json_set(
                data,
                '$.playtime_minutes', COALESCE(json_extract(data, '$.playtime_minutes'), 0) + ?2,
                '$.last_played_at', json(?3)
             ) WHERE id = ?1";
        let res = sqlx::query(sql)
            .bind(id)
            .bind(minutes as i64)
            .bind(serde_json::to_string(&last_played)?)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
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
        self.update_fields(id, &[("sync_badge", json!(badge))]).await
    }

    /// Records the cloud-sync merge-base for fast-forward vs. divergence
    /// detection.
    pub async fn set_cloud_baseline(&self, id: &str, tip: &str) -> AppResult<bool> {
        self.update_fields(id, &[("cloud_sync_baseline", json!(tip))])
            .await
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
/// `"A BC"`. Non-ASCII characters are dropped to avoid codepage issues in
/// legacy non-Unicode tools (some launchers / Inno Setup scripts).
/// Whitespace runs are then collapsed.
pub fn make_safe_filename(name: &str) -> String {
    const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    // Pass 1: ASCII-only, invalid-or-control chars become spaces.
    let stage: String = name
        .chars()
        .filter(|c| c.is_ascii())
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

/// Adds a new game. Assigns id/catalog/timestamps server-side; persists;
/// emits `library:changed` so any open windows can refresh.
#[tauri::command]
pub async fn add_game(
    state: State<'_, SharedLibrary>,
    app: AppHandle,
    new_game: NewGame,
) -> AppResult<GameEntry> {
    // Seed the per-entry Run-As-Admin toggle from the Windows AppCompatFlags
    // registry so an exe the OS already flags as "always run as administrator"
    // imports with the toggle on (no-op / false on non-Windows). Launches honour
    // the registry at runtime regardless, but reflecting it on the entry keeps
    // the editor toggle truthful instead of showing "off" for an elevated exe.
    let run_as_admin = crate::registry::run_as_admin_in_registry(&new_game.exe_path);
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
        game_folder_path: new_game.game_folder_path,
        wine_prefix_path: new_game.wine_prefix_path,
        proton_version_path: new_game.proton_version_path,
        // Newly added games are shared on the LAN by default; the user can
        // turn this off per-game in the editor. Sharing only actually
        // streams when game_folder_path is set (auto-detected on add).
        lan_shared: true,
        ..GameEntry::default()
    };
    let entry = state.insert(entry).await?;
    if let Err(e) = app.emit("library:changed", &entry.id) {
        tracing::warn!(error = %e, "failed to emit library:changed after add_game");
    }

    // Kick off cover-art + hero banner fetches. Non-blocking — the user sees
    // the new card immediately and both images land a moment later via a single
    // library:changed emit. One sgdb game-id lookup feeds both downloads.
    let app_for_art = app.clone();
    let id_for_art = entry.id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) =
            crate::steamgriddb::fetch_and_save_cover_and_hero(&app_for_art, &id_for_art).await
        {
            tracing::warn!(game_id = %id_for_art, error = %e, "cover/hero fetch failed");
        }
    });

    // Fetch Steam Store metadata (description, developer, publisher,
    // genres, release date) in parallel. Best-effort and only fills
    // empty fields — a no-op when the game has no steam_id.
    let app_for_meta = app.clone();
    let id_for_meta = entry.id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::metadata::fetch_and_save_metadata(&app_for_meta, &id_for_meta).await {
            tracing::warn!(game_id = %id_for_meta, error = %e, "metadata fetch failed");
        }
    });

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
    if !state.replace(&entry).await? {
        return Err(AppError::Other(format!(
            "game with id {} not found",
            entry.id
        )));
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
    let removed = state.remove(&id).await?;
    if removed {
        if let Err(e) = app.emit("library:changed", &id) {
            tracing::warn!(error = %e, "failed to emit library:changed after remove_game");
        }
    }
    Ok(removed)
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
    delete_game_core(state.inner(), &id).await?;
    if let Err(e) = app.emit("library:changed", &id) {
        tracing::warn!(error = %e, "failed to emit library:changed after delete_game_from_disk");
    }
    Ok(())
}

/// Folder-delete + entry-removal shared by the [`delete_game_from_disk`]
/// command and the Decky plugin server's `DELETE /games/:id`. Does not emit
/// `library:changed` — the caller does that where a Tauri `AppHandle` exists.
pub async fn delete_game_core(library: &SharedLibrary, id: &str) -> AppResult<()> {
    // Capture the folder + prefix paths before any blocking IO. The Proton
    // prefix is only ever managed on Linux, so its path is resolved (and later
    // deleted) on Linux alone — a populated `wine_prefix_path` override on
    // Windows/macOS must never be recurse-deleted.
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

    // Best-effort Proton prefix cleanup (Linux only) — never aborts the
    // removal. A missing prefix (e.g. a never-launched game) is a no-op.
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

    // Folder gone (or already absent) — now forget the entry.
    library.remove(id).await?;
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

    #[test]
    fn make_safe_filename_handles_basics() {
        assert_eq!(make_safe_filename("Hades II"), "Hades II");
        assert_eq!(make_safe_filename("Game: Bad/Chars?"), "Game Bad Chars");
        assert_eq!(make_safe_filename("  many   spaces  "), "many spaces");
        assert_eq!(make_safe_filename(""), "Game");
        assert_eq!(make_safe_filename("...."), "Game");
    }

    #[test]
    fn make_safe_filename_strips_non_ascii() {
        // Non-ASCII characters get dropped — same behaviour as the C#
        // version, intended to keep legacy non-Unicode tools happy.
        assert_eq!(make_safe_filename("Tëst Gämé"), "Tst Gm");
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
        assert_eq!(lib.find_id_by_name("Celeste").await.unwrap().as_deref(), Some("b"));
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
    async fn targeted_update_does_not_clobber_other_fields() {
        // A backup-stats write must not lose a concurrent playtime bump — the
        // whole point of the SQLite move. Simulate the two writes interleaving.
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert(sample("a", "Hades")).await.unwrap();

        lib.bump_session("a", Utc::now(), 30).await.unwrap();
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
        lib.bump_session("a", Utc::now(), 45).await.unwrap();

        // Editor loaded the entry before the session and saves a renamed copy
        // whose playtime is still 0.
        let mut edited = sample("a", "Hades Renamed");
        edited.playtime_minutes = 0;
        assert!(lib.replace(&edited).await.unwrap());

        let e = lib.find("a").await.unwrap().unwrap();
        assert_eq!(e.game_name, "Hades Renamed"); // editor change applied
        assert_eq!(e.playtime_minutes, 45); // runtime field preserved
        assert_eq!(lib.find_id_by_name("Hades Renamed").await.unwrap().as_deref(), Some("a"));
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

    fn session(id: &str, device: &str, game: &str, start: &str, mins: i64) -> PlaySession {
        let started = DateTime::parse_from_rfc3339(start).unwrap().with_timezone(&Utc);
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
        lib.insert_session(&session("deck:2", "deck", "Hades", "2026-05-02T10:00:00Z", 10)).await.unwrap();
        lib.insert_session(&session("deck:1", "deck", "Hades", "2026-05-01T10:00:00Z", 20)).await.unwrap();
        lib.insert_session(&session("pc:1", "pc", "Celeste", "2026-05-01T12:00:00Z", 5)).await.unwrap();

        let hades = lib.list_sessions(Some("Hades")).await.unwrap();
        assert_eq!(hades.len(), 2);
        assert_eq!(hades[0].session_id, "deck:1", "oldest first");
        assert_eq!(hades[1].session_id, "deck:2");

        assert_eq!(lib.list_sessions(None).await.unwrap().len(), 3, "all games");
    }

    #[tokio::test]
    async fn sessions_for_device_scopes_to_one_device() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert_session(&session("deck:1", "deck", "Hades", "2026-05-01T10:00:00Z", 20)).await.unwrap();
        lib.insert_session(&session("pc:1", "pc", "Hades", "2026-05-01T12:00:00Z", 5)).await.unwrap();
        let deck = lib.sessions_for_device("deck").await.unwrap();
        assert_eq!(deck.len(), 1);
        assert_eq!(deck[0].device_id, "deck");
    }

    #[tokio::test]
    async fn upsert_sessions_skips_existing_and_counts_new() {
        let lib = Library::open_in_memory().await.unwrap();
        lib.insert_session(&session("deck:1", "deck", "Hades", "2026-05-01T10:00:00Z", 20)).await.unwrap();
        // Folding a peer batch that re-includes deck:1 only adds the new rows.
        let batch = [
            session("deck:1", "deck", "Hades", "2026-05-01T10:00:00Z", 20),
            session("pc:1", "pc", "Hades", "2026-05-02T10:00:00Z", 15),
            session("pc:2", "pc", "Celeste", "2026-05-03T10:00:00Z", 5),
        ];
        assert_eq!(lib.upsert_sessions(&batch).await.unwrap(), 2, "only the two new rows");
        assert_eq!(lib.list_sessions(None).await.unwrap().len(), 3);
    }
}

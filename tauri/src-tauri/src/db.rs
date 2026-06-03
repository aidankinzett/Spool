//! SQLite connection pool for the game library.
//!
//! Spool runs several processes against the same data at once — the tray-resident
//! GUI, a per-launch attached `spool --run … --attached` instance, and (in Game
//! Mode) the Decky `spool --headless-server`. The old whole-file `library.json`
//! rewrites lost updates when two of them wrote at once. SQLite in WAL mode gives
//! many readers + one writer across processes, with OS file locks + a busy
//! timeout serialising writers, and lets writes target individual columns so two
//! processes touching different fields no longer clobber each other.
//!
//! This module owns the pool and the schema migrations. The query layer that
//! maps rows ↔ `GameEntry` lands in later migration steps (see
//! `docs/sqlite-migration-plan.md`); for now the pool is created, migrated, and
//! placed in Tauri state but nothing reads from it yet.

use crate::error::{AppError, AppResult};
use crate::library::GameEntry;
use crate::paths;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous,
};
use sqlx::{Row, SqlitePool};
use std::time::Duration;

/// Column list for the `games` table, in a fixed order shared by the INSERT
/// statement and the [`bind_game`] binder so the two never drift.
const GAME_COLUMNS: &str = "\
    id, catalog_number, game_name, exe_path, safe_name, \
    cover_image_path, hero_image_path, added_at, last_played_at, \
    launcher_exe_path, game_folder_path, run_as_admin, \
    use_proton, proton_version_path, wine_prefix_path, launch_args, \
    description, developer, publisher, genres, release_date, install_size_mb, \
    playtime_minutes, lan_shared, lan_share_folder, \
    save_backup_count, save_last_backed_up_at, save_backup_size_mb, \
    install_source, lan_install_source_device_name, lan_install_source_device_id, \
    steam_id, gog_id, lutris_slug, manifest_install_dir, save_paths, accent_color, \
    sync_badge, cloud_sync_baseline";

/// 39 positional placeholders matching [`GAME_COLUMNS`].
const GAME_PLACEHOLDERS: &str =
    "?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?";

/// Binds a [`GameEntry`]'s fields onto a positional `?`-placeholder query in
/// [`GAME_COLUMNS`] order. `genres_json` / `save_paths_json` are the
/// pre-serialised JSON strings (the list fields are stored as JSON text), passed
/// in so their storage outlives the bind. Used by both `upsert_game` and the
/// bulk import so there's a single source of truth for the column order.
macro_rules! bind_game {
    ($query:expr, $e:expr, $genres_json:expr, $save_paths_json:expr) => {
        $query
            .bind($e.id.as_str())
            .bind($e.catalog_number as i64)
            .bind($e.game_name.as_str())
            .bind($e.exe_path.as_str())
            .bind($e.safe_name.as_str())
            .bind($e.cover_image_path.as_deref())
            .bind($e.hero_image_path.as_deref())
            .bind($e.added_at)
            .bind($e.last_played_at)
            .bind($e.launcher_exe_path.as_deref())
            .bind($e.game_folder_path.as_deref())
            .bind($e.run_as_admin)
            .bind($e.use_proton)
            .bind($e.proton_version_path.as_deref())
            .bind($e.wine_prefix_path.as_deref())
            .bind($e.launch_args.as_deref())
            .bind($e.description.as_str())
            .bind($e.developer.as_str())
            .bind($e.publisher.as_str())
            .bind($genres_json.as_str())
            .bind($e.release_date)
            .bind($e.install_size_mb)
            .bind($e.playtime_minutes)
            .bind($e.lan_shared)
            .bind($e.lan_share_folder.as_deref())
            .bind($e.save_backup_count)
            .bind($e.save_last_backed_up_at)
            .bind($e.save_backup_size_mb)
            .bind($e.install_source.as_str())
            .bind($e.lan_install_source_device_name.as_deref())
            .bind($e.lan_install_source_device_id.as_deref())
            // steam_id / gog_id are u64 in Rust; SQLite INTEGER is i64. Real
            // Steam/GOG ids fit, so the lossless cast round-trips.
            .bind($e.steam_id.map(|v| v as i64))
            .bind($e.gog_id.map(|v| v as i64))
            .bind($e.lutris_slug.as_deref())
            .bind($e.manifest_install_dir.as_deref())
            .bind($save_paths_json.as_str())
            .bind($e.accent_color.as_deref())
            .bind($e.sync_badge.as_deref())
            .bind($e.cloud_sync_baseline.as_deref())
    };
}

/// Embedded migrations from `tauri/src-tauri/migrations/`. The `migrate!` macro
/// bakes the SQL files into the binary at compile time (it reads the directory at
/// build time but needs no live database), so a fresh install creates the schema
/// from the binary alone.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Handle to the library database. Clones are cheap (the inner `SqlitePool` is
/// reference-counted) so this can be stored in Tauri state and cloned into
/// spawned tasks.
#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Opens (creating if absent) the library database, applies the PRAGMAs that
    /// make cross-process access safe, and runs pending migrations. Retries a few
    /// times with a short backoff: several Spool processes can start at once
    /// (tray GUI + a launch process), and the loser of a first-launch migration
    /// race can hit a transient `SQLITE_BUSY` while the winner holds the write
    /// lock. Retrying rides that out instead of leaving a process with no db.
    pub async fn init() -> AppResult<Self> {
        const ATTEMPTS: u32 = 4;
        let mut last_err = None;
        for attempt in 1..=ATTEMPTS {
            match Self::init_once().await {
                Ok(db) => return Ok(db),
                Err(e) => {
                    if attempt < ATTEMPTS {
                        tracing::warn!(attempt, error = %e, "library.db open failed; retrying");
                        tokio::time::sleep(Duration::from_millis(150 * attempt as u64)).await;
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| AppError::Other("library.db init failed".to_string())))
    }

    /// Single open + migrate attempt. See [`Db::init`] for the retry wrapper.
    async fn init_once() -> AppResult<Self> {
        let path = paths::library_db();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Other(format!("create data dir for library.db: {e}")))?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            // WAL: concurrent readers don't block the single writer, and the
            // write-ahead log is shared across processes on local disk. (Never
            // point this at a network/rclone path — WAL relies on local locking.)
            .journal_mode(SqliteJournalMode::Wal)
            // Wait for a contended write lock instead of failing immediately with
            // SQLITE_BUSY — the writes here are tiny, so 5 s is ample headroom for
            // a second process to finish.
            .busy_timeout(Duration::from_secs(5))
            // NORMAL is the recommended durability level under WAL: safe against
            // application crashes, only at risk on a full OS/power loss.
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| AppError::Other(format!("open library.db: {e}")))?;

        MIGRATOR
            .run(&pool)
            .await
            .map_err(|e| AppError::Other(format!("run library.db migrations: {e}")))?;

        tracing::info!(path = %path.display(), "library.db ready");
        Ok(Self { pool })
    }

    /// Number of rows in `games`.
    pub async fn count_games(&self) -> AppResult<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM games")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Other(format!("count games: {e}")))
    }

    /// Insert-or-replace one game by `id` (full-row write). Correct for user
    /// actions on a single game (add/edit); later steps add field-level setters
    /// for the concurrently-written stats so two processes don't clobber.
    #[allow(dead_code)] // first writer call sites land in a later step
    pub async fn upsert_game(&self, e: &GameEntry) -> AppResult<()> {
        let genres = serde_json::to_string(&e.genres).unwrap_or_else(|_| "[]".to_string());
        let save_paths = serde_json::to_string(&e.save_paths).unwrap_or_else(|_| "[]".to_string());
        let sql =
            format!("INSERT OR REPLACE INTO games ({GAME_COLUMNS}) VALUES ({GAME_PLACEHOLDERS})");
        bind_game!(sqlx::query(&sql), e, genres, save_paths)
            .execute(&self.pool)
            .await
            .map_err(|err| AppError::Other(format!("upsert game {}: {err}", e.id)))?;
        Ok(())
    }

    /// One-shot import of `library.json` into the database: if `games` is empty
    /// and `entries` is non-empty, insert every entry inside one transaction and
    /// return the count. A populated table is left untouched (returns 0), so this
    /// is safe to call on every startup — it only does work the first launch
    /// after the db is introduced.
    ///
    /// The empty-table guard is a best-effort "skip the redundant work" check,
    /// not a cross-process mutex: two processes racing on first launch could both
    /// observe an empty table and both run the insert set. That's *outcome*-safe
    /// rather than mutually exclusive — every entry carries a stable `id` and
    /// `INSERT OR REPLACE` keyed on it converges to the same row set regardless of
    /// who writes last (WAL serialises the writers, so no corruption). The second
    /// importer just does wasted work.
    ///
    /// `entries` is expected to already have catalog numbers assigned — today the
    /// caller gets them from `Library::load`, which backfills legacy entries
    /// before this runs. When the JSON loader is removed (plan step 6) the
    /// importer must own that backfill, or imported rows would get `0`.
    pub async fn import_if_empty(&self, entries: &[GameEntry]) -> AppResult<usize> {
        if entries.is_empty() || self.count_games().await? > 0 {
            return Ok(0);
        }
        let sql =
            format!("INSERT OR REPLACE INTO games ({GAME_COLUMNS}) VALUES ({GAME_PLACEHOLDERS})");
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Other(format!("begin import tx: {e}")))?;
        for e in entries {
            let genres = serde_json::to_string(&e.genres).unwrap_or_else(|_| "[]".to_string());
            let save_paths =
                serde_json::to_string(&e.save_paths).unwrap_or_else(|_| "[]".to_string());
            bind_game!(sqlx::query(&sql), e, genres, save_paths)
                .execute(&mut *tx)
                .await
                .map_err(|err| AppError::Other(format!("import game {}: {err}", e.id)))?;
        }
        tx.commit()
            .await
            .map_err(|e| AppError::Other(format!("commit import tx: {e}")))?;
        Ok(entries.len())
    }

    /// All games, ordered by catalog number (≈ add order — the order the
    /// JSON-backed `Vec` preserved). The inverse of `import`/`upsert`.
    #[allow(dead_code)] // call sites flip to this in the next step
    pub async fn list_games(&self) -> AppResult<Vec<GameEntry>> {
        let sql = format!("SELECT {GAME_COLUMNS} FROM games ORDER BY catalog_number");
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Other(format!("list games: {e}")))?;
        rows.iter().map(row_to_entry).collect()
    }

    /// One game by id, or `None` if absent.
    #[allow(dead_code)] // call sites flip to this in the next step
    pub async fn find(&self, id: &str) -> AppResult<Option<GameEntry>> {
        let sql = format!("SELECT {GAME_COLUMNS} FROM games WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AppError::Other(format!("find game {id}: {e}")))?;
        row.as_ref().map(row_to_entry).transpose()
    }

    /// First game whose `game_name` matches exactly, or `None`. Names aren't
    /// unique; ties resolve to the lowest catalog number for a stable result
    /// (mirrors the old `Vec`'s first-match-by-insertion-order behaviour).
    #[allow(dead_code)] // call sites flip to this in the next step
    pub async fn find_by_name(&self, name: &str) -> AppResult<Option<GameEntry>> {
        let sql = format!(
            "SELECT {GAME_COLUMNS} FROM games WHERE game_name = ? ORDER BY catalog_number LIMIT 1"
        );
        let row = sqlx::query(&sql)
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AppError::Other(format!("find game by name {name}: {e}")))?;
        row.as_ref().map(row_to_entry).transpose()
    }

    /// Borrow the underlying pool for queries.
    #[allow(dead_code)] // wired into state now; first readers land in a later step
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Maps a `games` row back into a [`GameEntry`] — the inverse of [`bind_game`].
/// Column order is irrelevant here (lookups are by name), but every column the
/// binder writes must be read back, so the two evolve together. The add-then-read
/// identity test guards against drift.
fn row_to_entry(row: &SqliteRow) -> AppResult<GameEntry> {
    // List fields are stored as JSON text; parse back to Vec<String>.
    let genres_json: String = try_col(row, "genres")?;
    let save_paths_json: String = try_col(row, "save_paths")?;
    let genres: Vec<String> = serde_json::from_str(&genres_json)
        .map_err(|e| AppError::Other(format!("decode genres json: {e}")))?;
    let save_paths: Vec<String> = serde_json::from_str(&save_paths_json)
        .map_err(|e| AppError::Other(format!("decode save_paths json: {e}")))?;

    Ok(GameEntry {
        id: try_col(row, "id")?,
        catalog_number: try_col::<i64>(row, "catalog_number")? as u32,
        game_name: try_col(row, "game_name")?,
        exe_path: try_col(row, "exe_path")?,
        safe_name: try_col(row, "safe_name")?,
        cover_image_path: try_col(row, "cover_image_path")?,
        hero_image_path: try_col(row, "hero_image_path")?,
        added_at: try_col(row, "added_at")?,
        last_played_at: try_col(row, "last_played_at")?,
        launcher_exe_path: try_col(row, "launcher_exe_path")?,
        game_folder_path: try_col(row, "game_folder_path")?,
        run_as_admin: try_col(row, "run_as_admin")?,
        use_proton: try_col(row, "use_proton")?,
        proton_version_path: try_col(row, "proton_version_path")?,
        wine_prefix_path: try_col(row, "wine_prefix_path")?,
        launch_args: try_col(row, "launch_args")?,
        description: try_col(row, "description")?,
        developer: try_col(row, "developer")?,
        publisher: try_col(row, "publisher")?,
        genres,
        release_date: try_col(row, "release_date")?,
        install_size_mb: try_col(row, "install_size_mb")?,
        playtime_minutes: try_col(row, "playtime_minutes")?,
        lan_shared: try_col(row, "lan_shared")?,
        lan_share_folder: try_col(row, "lan_share_folder")?,
        save_backup_count: try_col(row, "save_backup_count")?,
        save_last_backed_up_at: try_col(row, "save_last_backed_up_at")?,
        save_backup_size_mb: try_col(row, "save_backup_size_mb")?,
        install_source: try_col(row, "install_source")?,
        lan_install_source_device_name: try_col(row, "lan_install_source_device_name")?,
        lan_install_source_device_id: try_col(row, "lan_install_source_device_id")?,
        // i64 ↔ u64: symmetric with the bind-side cast (real ids fit in i64).
        steam_id: try_col::<Option<i64>>(row, "steam_id")?.map(|v| v as u64),
        gog_id: try_col::<Option<i64>>(row, "gog_id")?.map(|v| v as u64),
        lutris_slug: try_col(row, "lutris_slug")?,
        manifest_install_dir: try_col(row, "manifest_install_dir")?,
        save_paths,
        accent_color: try_col(row, "accent_color")?,
        sync_badge: try_col(row, "sync_badge")?,
        cloud_sync_baseline: try_col(row, "cloud_sync_baseline")?,
    })
}

/// `row.try_get` with the column name folded into the error for a useful message
/// when a column type doesn't decode.
fn try_col<'r, T>(row: &'r SqliteRow, col: &str) -> AppResult<T>
where
    T: sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>,
{
    row.try_get::<T, _>(col)
        .map_err(|e| AppError::Other(format!("decode column {col}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    /// The embedded migrations apply cleanly against a fresh database and create
    /// the `games` table. Uses an in-memory pool so it doesn't depend on the
    /// app-data path. Guards against a malformed migration SQL file.
    #[tokio::test]
    async fn migrations_apply_and_create_games_table() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory db");
        MIGRATOR.run(&pool).await.expect("run migrations");

        // Table exists and is queryable.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games")
            .fetch_one(&pool)
            .await
            .expect("query games");
        assert_eq!(count, 0);
    }

    /// Build a `Db` over a fresh in-memory database for tests.
    async fn memory_db() -> Db {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory db");
        MIGRATOR.run(&pool).await.expect("run migrations");
        Db { pool }
    }

    #[tokio::test]
    async fn import_if_empty_inserts_then_noops() {
        let db = memory_db().await;

        let mut a = GameEntry {
            id: "id-a".to_string(),
            game_name: "Hades".to_string(),
            catalog_number: 1,
            genres: vec!["Action".to_string(), "Roguelike".to_string()],
            save_paths: vec!["%APPDATA%/Hades".to_string()],
            steam_id: Some(1145360),
            run_as_admin: true,
            install_size_mb: 12.5,
            ..GameEntry::default()
        };
        a.added_at = Some(chrono::Utc::now());
        let b = GameEntry {
            id: "id-b".to_string(),
            game_name: "Celeste".to_string(),
            catalog_number: 2,
            ..GameEntry::default()
        };

        let n = db.import_if_empty(&[a.clone(), b]).await.expect("import");
        assert_eq!(n, 2);
        assert_eq!(db.count_games().await.unwrap(), 2);

        // Re-import is a no-op once the table is populated.
        let again = db.import_if_empty(&[a.clone()]).await.expect("reimport");
        assert_eq!(again, 0);
        assert_eq!(db.count_games().await.unwrap(), 2);

        // Tricky fields round-trip: JSON list column, u64 id, bool.
        let (genres, steam_id, run_as_admin): (String, i64, bool) =
            sqlx::query_as("SELECT genres, steam_id, run_as_admin FROM games WHERE id = ?")
                .bind("id-a")
                .fetch_one(db.pool())
                .await
                .expect("fetch id-a");
        assert_eq!(genres, r#"["Action","Roguelike"]"#);
        assert_eq!(steam_id, 1145360);
        assert!(run_as_admin);
    }

    /// A fully-populated entry survives upsert → read with every field intact.
    /// This is the guard against `bind_game!` / `row_to_entry` column drift: if a
    /// column is bound but not read back (or vice-versa), the round trip breaks.
    #[tokio::test]
    async fn upsert_then_read_round_trips_all_fields() {
        let db = memory_db().await;

        // Fixed, whole-second timestamp — avoids sub-second precision flakiness
        // in the TEXT round trip.
        let ts = DateTime::parse_from_rfc3339("2026-06-04T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let original = GameEntry {
            id: "round-trip".to_string(),
            catalog_number: 7,
            game_name: "Hollow Knight".to_string(),
            exe_path: "C:/Games/HK/hollow_knight.exe".to_string(),
            safe_name: "Hollow Knight".to_string(),
            cover_image_path: Some("covers/hk.png".to_string()),
            hero_image_path: None,
            added_at: Some(ts),
            last_played_at: Some(ts),
            launcher_exe_path: None,
            game_folder_path: Some("C:/Games/HK".to_string()),
            run_as_admin: true,
            use_proton: false,
            proton_version_path: None,
            wine_prefix_path: Some("/home/u/.prefix".to_string()),
            launch_args: Some("--windowed".to_string()),
            description: "Explore a vast ruined kingdom.".to_string(),
            developer: "Team Cherry".to_string(),
            publisher: "Team Cherry".to_string(),
            genres: vec!["Metroidvania".to_string(), "Souls-like".to_string()],
            release_date: Some(ts),
            install_size_mb: 9000.5,
            playtime_minutes: 1234,
            lan_shared: true,
            lan_share_folder: Some("C:/Games/HK".to_string()),
            save_backup_count: 3,
            save_last_backed_up_at: Some(ts),
            save_backup_size_mb: 4.25,
            install_source: "lan".to_string(),
            lan_install_source_device_name: Some("deck".to_string()),
            lan_install_source_device_id: Some("dev-1".to_string()),
            steam_id: Some(367520),
            gog_id: None,
            lutris_slug: Some("hollow-knight".to_string()),
            manifest_install_dir: Some("Hollow Knight".to_string()),
            save_paths: vec!["%USERPROFILE%/AppData/.../HK".to_string()],
            accent_color: Some("#2b3a67".to_string()),
            sync_badge: Some("synced".to_string()),
            cloud_sync_baseline: Some("backup-2026".to_string()),
        };

        db.upsert_game(&original).await.expect("upsert");
        let read_back = db
            .find("round-trip")
            .await
            .expect("find")
            .expect("present");
        assert_eq!(read_back, original);

        // An entry that leaves the optional/empty fields at their defaults also
        // round-trips (None columns, empty Vecs → '[]').
        let minimal = GameEntry {
            id: "minimal".to_string(),
            game_name: "Bare".to_string(),
            ..GameEntry::default()
        };
        db.upsert_game(&minimal).await.expect("upsert minimal");
        let read_min = db.find("minimal").await.expect("find").expect("present");
        assert_eq!(read_min, minimal);
    }

    #[tokio::test]
    async fn list_games_orders_by_catalog_and_find_by_name_picks_lowest() {
        let db = memory_db().await;
        let mk = |id: &str, cat: u32, name: &str| GameEntry {
            id: id.to_string(),
            catalog_number: cat,
            game_name: name.to_string(),
            ..GameEntry::default()
        };
        // Insert out of catalog order; duplicate name across two entries.
        db.upsert_game(&mk("c", 3, "Dup")).await.unwrap();
        db.upsert_game(&mk("a", 1, "Dup")).await.unwrap();
        db.upsert_game(&mk("b", 2, "Solo")).await.unwrap();

        let list = db.list_games().await.unwrap();
        assert_eq!(
            list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"],
            "ordered by catalog_number"
        );

        // Duplicate name resolves to the lowest catalog number.
        let dup = db.find_by_name("Dup").await.unwrap().expect("present");
        assert_eq!(dup.id, "a");
        assert!(db.find_by_name("Nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upsert_game_replaces_by_id() {
        let db = memory_db().await;
        let mut e = GameEntry {
            id: "g1".to_string(),
            game_name: "Original".to_string(),
            ..GameEntry::default()
        };
        db.upsert_game(&e).await.expect("insert");
        e.game_name = "Renamed".to_string();
        db.upsert_game(&e).await.expect("update");

        assert_eq!(db.count_games().await.unwrap(), 1);
        let name: String = sqlx::query_scalar("SELECT game_name FROM games WHERE id = ?")
            .bind("g1")
            .fetch_one(db.pool())
            .await
            .expect("fetch name");
        assert_eq!(name, "Renamed");
    }
}

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
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::SqlitePool;
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
    /// Opens (creating if absent) the library database at
    /// `%LOCALAPPDATA%\Spool\library.db`, applies the PRAGMAs that make
    /// cross-process access safe, and runs pending migrations.
    pub async fn init() -> AppResult<Self> {
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
    /// after the db is introduced. `INSERT OR REPLACE` keyed on `id` makes a
    /// concurrent double-import (two processes racing on first launch) idempotent.
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

    /// Borrow the underlying pool for queries.
    #[allow(dead_code)] // wired into state now; first readers land in a later step
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

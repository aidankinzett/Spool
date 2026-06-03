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
use crate::paths;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::SqlitePool;
use std::time::Duration;

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
}

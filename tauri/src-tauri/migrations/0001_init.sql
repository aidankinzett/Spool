-- Initial schema for the game library.
--
-- One row per game, mirroring the `GameEntry` struct in `library.rs`. This is
-- the SQLite replacement for `library.json`; see `docs/sqlite-migration-plan.md`
-- for why (multi-process write safety: the tray GUI, attached `--run` launches,
-- and the Decky headless-server can all write concurrently).
--
-- Type mapping notes:
--   * Booleans are stored as INTEGER 0/1.
--   * `DateTime<Utc>` is stored as RFC3339 TEXT (sqlx maps this natively).
--   * List fields (`genres`, `save_paths`) are stored as JSON TEXT — opaque for
--     now, queryable via SQLite's json_* functions later if a feature needs it.
--   * `steam_id` / `gog_id` are u64 in Rust but stored as INTEGER (i64). Real
--     Steam/GOG ids fit comfortably in i64; the row<->struct mapping handles the
--     u64<->i64 conversion.
--
-- Defaults match `GameEntry::default()` so an INSERT that omits a column lands on
-- the same value the serde `#[serde(default)]` path produced.

CREATE TABLE IF NOT EXISTS games (
    id                              TEXT    PRIMARY KEY NOT NULL,
    catalog_number                  INTEGER NOT NULL DEFAULT 0,
    game_name                       TEXT    NOT NULL DEFAULT '',
    exe_path                        TEXT    NOT NULL DEFAULT '',
    safe_name                       TEXT    NOT NULL DEFAULT '',

    cover_image_path                TEXT,
    hero_image_path                 TEXT,

    added_at                        TEXT,
    last_played_at                  TEXT,

    launcher_exe_path               TEXT,
    game_folder_path                TEXT,

    run_as_admin                    INTEGER NOT NULL DEFAULT 0,

    -- Proton / Linux launch (inert on Windows)
    use_proton                      INTEGER NOT NULL DEFAULT 0,
    proton_version_path             TEXT,
    wine_prefix_path                TEXT,
    launch_args                     TEXT,

    -- Metadata
    description                     TEXT    NOT NULL DEFAULT '',
    developer                       TEXT    NOT NULL DEFAULT '',
    publisher                       TEXT    NOT NULL DEFAULT '',
    genres                          TEXT    NOT NULL DEFAULT '[]',
    release_date                    TEXT,
    install_size_mb                 REAL    NOT NULL DEFAULT 0,

    -- Play tracking
    playtime_minutes                INTEGER NOT NULL DEFAULT 0,

    -- LAN sharing
    lan_shared                      INTEGER NOT NULL DEFAULT 0,
    lan_share_folder                TEXT,

    -- Save backup stats (updated by the run workflow)
    save_backup_count               INTEGER NOT NULL DEFAULT 0,
    save_last_backed_up_at          TEXT,
    save_backup_size_mb             REAL    NOT NULL DEFAULT 0,

    install_source                  TEXT    NOT NULL DEFAULT 'manual',
    lan_install_source_device_name  TEXT,
    lan_install_source_device_id    TEXT,

    -- Manifest-derived metadata
    steam_id                        INTEGER,
    gog_id                          INTEGER,
    lutris_slug                     TEXT,
    manifest_install_dir            TEXT,
    save_paths                      TEXT    NOT NULL DEFAULT '[]',
    accent_color                    TEXT,

    -- Cross-device save-sync
    sync_badge                      TEXT,
    cloud_sync_baseline             TEXT
);

-- `find_game_id_by_name` looks games up by exact name; the UI sorts/badges by
-- catalog number. Index both.
CREATE INDEX IF NOT EXISTS idx_games_game_name      ON games (game_name);
CREATE INDEX IF NOT EXISTS idx_games_catalog_number ON games (catalog_number);

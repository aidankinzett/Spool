import Database from "better-sqlite3";
import path from "node:path";

const dbPath = process.env.DATABASE_PATH ?? path.join(process.cwd(), "ludusavi.db");
export const db = new Database(dbPath);

db.pragma("journal_mode = WAL");
db.pragma("foreign_keys = ON");

db.exec(`
  CREATE TABLE IF NOT EXISTS users (
    id         TEXT PRIMARY KEY,
    username   TEXT UNIQUE NOT NULL,
    api_key    TEXT UNIQUE NOT NULL,
    created_at TEXT NOT NULL
  );

  CREATE TABLE IF NOT EXISTS locks (
    id             TEXT PRIMARY KEY,
    user_id        TEXT NOT NULL REFERENCES users(id),
    game_name      TEXT NOT NULL,
    device_id      TEXT NOT NULL,
    device_name    TEXT NOT NULL,
    locked_at      TEXT NOT NULL,
    last_heartbeat TEXT NOT NULL,
    UNIQUE(user_id, game_name)
  );

  CREATE TABLE IF NOT EXISTS backup_events (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    game_name   TEXT NOT NULL,
    device_id   TEXT NOT NULL,
    device_name TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    occurred_at TEXT NOT NULL
  );

  CREATE INDEX IF NOT EXISTS idx_backup_events_lookup
    ON backup_events (user_id, game_name, event_type, occurred_at DESC);

  CREATE TABLE IF NOT EXISTS game_last_played (
    user_id        TEXT NOT NULL REFERENCES users(id),
    game_name      TEXT NOT NULL,
    last_played_at TEXT NOT NULL,
    PRIMARY KEY(user_id, game_name)
  );
`);

export interface User {
  id: string;
  username: string;
  api_key: string;
  created_at: string;
}

export interface BackupEvent {
  id: string;
  user_id: string;
  game_name: string;
  device_id: string;
  device_name: string;
  event_type: "backup" | "restore";
  occurred_at: string;
}

export interface Lock {
  id: string;
  user_id: string;
  game_name: string;
  device_id: string;
  device_name: string;
  locked_at: string;
  last_heartbeat: string;
}

export interface GameLastPlayed {
  user_id: string;
  game_name: string;
  last_played_at: string;
}

export const STALE_THRESHOLD_MS = 5 * 60 * 1000;

export function isStale(lock: Lock): boolean {
  return Date.now() - new Date(lock.last_heartbeat).getTime() > STALE_THRESHOLD_MS;
}

export const queries = {
  getUserByApiKey: db.prepare<[string], User>(
    "SELECT * FROM users WHERE api_key = ?"
  ),
  getUserByUsername: db.prepare<[string], User>(
    "SELECT * FROM users WHERE username = ?"
  ),
  insertUser: db.prepare(
    "INSERT INTO users (id, username, api_key, created_at) VALUES (?, ?, ?, ?)"
  ),
  getLock: db.prepare<[string, string], Lock>(
    "SELECT * FROM locks WHERE user_id = ? AND game_name = ?"
  ),
  upsertLock: db.prepare(
    `INSERT INTO locks (id, user_id, game_name, device_id, device_name, locked_at, last_heartbeat)
     VALUES (?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(user_id, game_name) DO UPDATE SET
       id = excluded.id,
       device_id = excluded.device_id,
       device_name = excluded.device_name,
       locked_at = excluded.locked_at,
       last_heartbeat = excluded.last_heartbeat`
  ),
  updateHeartbeat: db.prepare(
    "UPDATE locks SET last_heartbeat = ? WHERE user_id = ? AND game_name = ? AND device_id = ?"
  ),
  deleteLock: db.prepare(
    "DELETE FROM locks WHERE user_id = ? AND game_name = ? AND device_id = ?"
  ),
  insertBackupEvent: db.prepare(
    `INSERT INTO backup_events (id, user_id, game_name, device_id, device_name, event_type, occurred_at)
     VALUES (?, ?, ?, ?, ?, ?, ?)`
  ),
  getLatestBackupEvent: db.prepare<[string, string], BackupEvent>(
    `SELECT * FROM backup_events
     WHERE user_id = ? AND game_name = ? AND event_type = 'backup'
     ORDER BY occurred_at DESC LIMIT 1`
  ),
  getLastPlayed: db.prepare<[string], GameLastPlayed>(
    "SELECT * FROM game_last_played WHERE user_id = ?"
  ),
  upsertLastPlayed: db.prepare(
    `INSERT INTO game_last_played (user_id, game_name, last_played_at)
     VALUES (?, ?, ?)
     ON CONFLICT(user_id, game_name) DO UPDATE SET
       last_played_at = excluded.last_played_at`
  ),
};

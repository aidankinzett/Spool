import { v4 as uuidv4 } from "uuid";
import { db, queries } from "./db.js";

// Wipe every table. The DB is a process-wide in-memory singleton (see
// vitest.config.ts), so tests call this in beforeEach to stay isolated.
export function resetDb(): void {
  db.exec(`
    DELETE FROM game_playtime;
    DELETE FROM game_last_played;
    DELETE FROM backup_events;
    DELETE FROM locks;
    DELETE FROM users;
  `);
}

export function createUser(username = "tester"): { id: string; apiKey: string } {
  const id = uuidv4();
  const apiKey = uuidv4();
  queries.insertUser.run(id, username, apiKey, new Date().toISOString());
  return { id, apiKey };
}

// Insert a lock with an arbitrary heartbeat age so staleness/takeover
// paths can be exercised without waiting real time.
export function seedLock(opts: {
  userId: string;
  gameName: string;
  deviceId: string;
  deviceName?: string;
  heartbeatAgeMs?: number;
  // When set, marks the lock suspended until `now + suspendedForMs` (negative
  // values produce an already-expired suspend marker).
  suspendedForMs?: number;
}): void {
  const ts = new Date(Date.now() - (opts.heartbeatAgeMs ?? 0)).toISOString();
  queries.upsertLock.run(
    uuidv4(),
    opts.userId,
    opts.gameName,
    opts.deviceId,
    opts.deviceName ?? opts.deviceId,
    ts,
    ts
  );
  if (opts.suspendedForMs !== undefined) {
    const suspendedUntil = new Date(Date.now() + opts.suspendedForMs).toISOString();
    queries.setSuspended.run(suspendedUntil, ts, opts.userId, opts.gameName, opts.deviceId);
  }
}

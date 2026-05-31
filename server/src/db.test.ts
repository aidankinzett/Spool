import { beforeEach, describe, expect, it } from "vitest";
import { STALE_THRESHOLD_MS, isStale, queries, sweepStaleLocks } from "./db.js";
import type { Lock } from "./db.js";
import { createUser, resetDb, seedLock } from "./test-utils.js";

function lockWithHeartbeat(ageMs: number): Lock {
  return {
    id: "lock-1",
    user_id: "user-1",
    game_name: "Hades",
    device_id: "device-1",
    device_name: "Deck",
    locked_at: new Date(Date.now() - ageMs).toISOString(),
    last_heartbeat: new Date(Date.now() - ageMs).toISOString(),
  };
}

describe("isStale", () => {
  it("treats a fresh heartbeat as live", () => {
    expect(isStale(lockWithHeartbeat(0))).toBe(false);
  });

  it("treats a heartbeat just under the threshold as live", () => {
    expect(isStale(lockWithHeartbeat(STALE_THRESHOLD_MS - 1_000))).toBe(false);
  });

  it("treats a heartbeat past the threshold as stale", () => {
    expect(isStale(lockWithHeartbeat(STALE_THRESHOLD_MS + 1_000))).toBe(true);
  });
});

describe("sweepStaleLocks", () => {
  beforeEach(() => {
    resetDb();
  });

  it("removes locks past the stale threshold and keeps fresh ones", () => {
    const user = createUser();
    seedLock({ userId: user.id, gameName: "Fresh", deviceId: "dev-a", heartbeatAgeMs: 0 });
    seedLock({
      userId: user.id,
      gameName: "Stale",
      deviceId: "dev-b",
      heartbeatAgeMs: STALE_THRESHOLD_MS + 60_000,
    });

    const removed = sweepStaleLocks();

    expect(removed).toBe(1);
    expect(queries.getLock.get(user.id, "Stale")).toBeUndefined();
    expect(queries.getLock.get(user.id, "Fresh")).toBeDefined();
  });

  it("is a no-op when no locks are stale", () => {
    const user = createUser();
    seedLock({ userId: user.id, gameName: "Fresh", deviceId: "dev-a", heartbeatAgeMs: 0 });
    expect(sweepStaleLocks()).toBe(0);
  });
});

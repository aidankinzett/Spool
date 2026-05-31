import { beforeEach, describe, expect, it } from "vitest";
import {
  STALE_THRESHOLD_MS,
  SUSPEND_GRACE_MS,
  isStale,
  isSuspended,
  queries,
  sweepStaleLocks,
} from "./db.js";
import type { Lock } from "./db.js";
import { createUser, resetDb, seedLock } from "./test-utils.js";

function lockWithHeartbeat(ageMs: number, suspendedUntil: string | null = null): Lock {
  return {
    id: "lock-1",
    user_id: "user-1",
    game_name: "Hades",
    device_id: "device-1",
    device_name: "Deck",
    locked_at: new Date(Date.now() - ageMs).toISOString(),
    last_heartbeat: new Date(Date.now() - ageMs).toISOString(),
    suspended_until: suspendedUntil,
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

  it("keeps a suspended lock live past the heartbeat threshold", () => {
    const suspendedUntil = new Date(Date.now() + SUSPEND_GRACE_MS).toISOString();
    expect(isStale(lockWithHeartbeat(STALE_THRESHOLD_MS + 60_000, suspendedUntil))).toBe(false);
  });

  it("falls back to stale once the suspend grace has expired", () => {
    const suspendedUntil = new Date(Date.now() - 1_000).toISOString();
    expect(isStale(lockWithHeartbeat(STALE_THRESHOLD_MS + 60_000, suspendedUntil))).toBe(true);
  });
});

describe("isSuspended", () => {
  it("is true while the marker is in the future", () => {
    expect(isSuspended(lockWithHeartbeat(0, new Date(Date.now() + 60_000).toISOString()))).toBe(true);
  });

  it("is false with no marker", () => {
    expect(isSuspended(lockWithHeartbeat(0))).toBe(false);
  });

  it("is false once the marker has elapsed", () => {
    expect(isSuspended(lockWithHeartbeat(0, new Date(Date.now() - 1_000).toISOString()))).toBe(false);
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

  it("does not reap a stale-heartbeat lock still inside its suspend grace", () => {
    const user = createUser();
    seedLock({
      userId: user.id,
      gameName: "Asleep",
      deviceId: "dev-c",
      heartbeatAgeMs: STALE_THRESHOLD_MS + 60_000,
      suspendedForMs: SUSPEND_GRACE_MS,
    });

    expect(sweepStaleLocks()).toBe(0);
    expect(queries.getLock.get(user.id, "Asleep")).toBeDefined();
  });

  it("reaps a suspended lock once its grace has expired", () => {
    const user = createUser();
    seedLock({
      userId: user.id,
      gameName: "DeadDeck",
      deviceId: "dev-d",
      heartbeatAgeMs: STALE_THRESHOLD_MS + 60_000,
      suspendedForMs: -1_000, // grace already elapsed
    });

    expect(sweepStaleLocks()).toBe(1);
    expect(queries.getLock.get(user.id, "DeadDeck")).toBeUndefined();
  });
});

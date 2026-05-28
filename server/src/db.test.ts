import { describe, expect, it } from "vitest";
import { STALE_THRESHOLD_MS, isStale } from "./db.js";
import type { Lock } from "./db.js";

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

import { beforeEach, describe, expect, it } from "vitest";
import { app } from "../app.js";
import { STALE_THRESHOLD_MS } from "../db.js";
import { createUser, resetDb, seedLock } from "../test-utils.js";

let apiKey: string;
let userId: string;

beforeEach(() => {
  resetDb();
  const u = createUser();
  apiKey = u.apiKey;
  userId = u.id;
});

function auth(extra: Record<string, string> = {}) {
  return { Authorization: `Bearer ${apiKey}`, ...extra };
}

function acquire(game: string, body: unknown, headers: Record<string, string> = {}) {
  return app.request(`/locks/${game}/acquire`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...auth(headers) },
    body: typeof body === "string" ? body : JSON.stringify(body),
  });
}

describe("auth gating", () => {
  it("rejects requests with no token", async () => {
    const res = await app.request("/locks/Hades");
    expect(res.status).toBe(401);
  });

  it("rejects requests with an unknown api key", async () => {
    const res = await app.request("/locks/Hades", {
      headers: { Authorization: "Bearer not-a-real-key" },
    });
    expect(res.status).toBe(401);
  });
});

describe("GET /locks/:game", () => {
  it("reports an unlocked game", async () => {
    const res = await app.request("/locks/Hades", { headers: auth() });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ locked: false });
  });

  it("reports a held lock with device info", async () => {
    seedLock({ userId, gameName: "Hades", deviceId: "deck", deviceName: "Steam Deck" });
    const res = await app.request("/locks/Hades", { headers: auth() });
    const body = await res.json();
    expect(body).toMatchObject({
      locked: true,
      device_id: "deck",
      device_name: "Steam Deck",
      stale: false,
    });
  });

  it("flags a lock past the stale threshold", async () => {
    seedLock({
      userId,
      gameName: "Hades",
      deviceId: "deck",
      heartbeatAgeMs: STALE_THRESHOLD_MS + 60_000,
    });
    const body = await (await app.request("/locks/Hades", { headers: auth() })).json();
    expect(body.stale).toBe(true);
  });
});

describe("POST /locks/:game/acquire", () => {
  it("acquires a free lock", async () => {
    const res = await acquire("Hades", { device_id: "deck", device_name: "Steam Deck" });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ acquired: true });
  });

  it("rejects a malformed JSON body", async () => {
    const res = await acquire("Hades", "{ not json");
    expect(res.status).toBe(400);
  });

  it("rejects a non-object JSON body without crashing", async () => {
    expect((await acquire("Hades", "null")).status).toBe(400);
    expect((await acquire("Hades", "42")).status).toBe(400);
    expect((await acquire("Hades", "[]")).status).toBe(400);
  });

  it("requires device_id and device_name", async () => {
    expect((await acquire("Hades", { device_id: "deck" })).status).toBe(400);
    expect((await acquire("Hades", { device_name: "Deck" })).status).toBe(400);
    expect((await acquire("Hades", { device_id: " ", device_name: " " })).status).toBe(400);
  });

  it("blocks a different device while the lock is live", async () => {
    seedLock({ userId, gameName: "Hades", deviceId: "desktop", deviceName: "PC" });
    const res = await acquire("Hades", { device_id: "deck", device_name: "Deck" });
    expect(res.status).toBe(409);
    expect(await res.json()).toMatchObject({ device_id: "desktop", device_name: "PC" });
  });

  it("lets the holding device re-acquire (refresh)", async () => {
    seedLock({ userId, gameName: "Hades", deviceId: "deck", deviceName: "Deck" });
    const res = await acquire("Hades", { device_id: "deck", device_name: "Deck" });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ acquired: true });
  });

  it("steals a stale lock from another device", async () => {
    seedLock({
      userId,
      gameName: "Hades",
      deviceId: "desktop",
      heartbeatAgeMs: STALE_THRESHOLD_MS + 60_000,
    });
    const res = await acquire("Hades", { device_id: "deck", device_name: "Deck" });
    expect(res.status).toBe(200);
  });

  it("scopes locks per user — another user's lock does not block", async () => {
    const other = createUser("other");
    seedLock({ userId: other.id, gameName: "Hades", deviceId: "desktop" });
    const res = await acquire("Hades", { device_id: "deck", device_name: "Deck" });
    expect(res.status).toBe(200);
  });

  it("blocks a suspended lock without steal, flagging it suspended", async () => {
    seedLock({
      userId,
      gameName: "Hades",
      deviceId: "desktop",
      deviceName: "PC",
      suspendedForMs: 60 * 60 * 1000,
    });
    const res = await acquire("Hades", { device_id: "deck", device_name: "Deck" });
    expect(res.status).toBe(409);
    expect(await res.json()).toMatchObject({ device_name: "PC", suspended: true });
  });

  it("steals a suspended lock when steal is set", async () => {
    seedLock({
      userId,
      gameName: "Hades",
      deviceId: "desktop",
      suspendedForMs: 60 * 60 * 1000,
    });
    const res = await acquire("Hades", { device_id: "deck", device_name: "Deck", steal: true });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ acquired: true });

    // The new holder owns it, and stealing cleared the suspend marker.
    const body = await (await app.request("/locks/Hades", { headers: auth() })).json();
    expect(body).toMatchObject({ device_id: "deck", suspended: false });
  });

  it("does not let steal override a live, actively-held lock", async () => {
    seedLock({ userId, gameName: "Hades", deviceId: "desktop", deviceName: "PC" });
    const res = await acquire("Hades", { device_id: "deck", device_name: "Deck", steal: true });
    expect(res.status).toBe(409);
  });
});

describe("POST /locks/:game/suspend", () => {
  it("marks a held lock suspended and keeps it live past the stale threshold", async () => {
    seedLock({
      userId,
      gameName: "Hades",
      deviceId: "deck",
      heartbeatAgeMs: STALE_THRESHOLD_MS + 60_000,
    });
    const res = await app.request("/locks/Hades/suspend", {
      method: "POST",
      headers: auth({ "X-Device-Id": "deck" }),
    });
    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ ok: true });

    const body = await (await app.request("/locks/Hades", { headers: auth() })).json();
    expect(body).toMatchObject({ stale: false, suspended: true });
  });

  it("requires the X-Device-Id header", async () => {
    const res = await app.request("/locks/Hades/suspend", { method: "POST", headers: auth() });
    expect(res.status).toBe(400);
  });

  it("404s when the device holds no lock", async () => {
    const res = await app.request("/locks/Hades/suspend", {
      method: "POST",
      headers: auth({ "X-Device-Id": "ghost" }),
    });
    expect(res.status).toBe(404);
  });
});

describe("POST /locks/:game/release", () => {
  it("releases a lock held by the requesting device", async () => {
    seedLock({ userId, gameName: "Hades", deviceId: "deck" });
    const res = await app.request("/locks/Hades/release", {
      method: "POST",
      headers: auth({ "X-Device-Id": "deck" }),
    });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ released: true });
  });

  it("requires the X-Device-Id header", async () => {
    const res = await app.request("/locks/Hades/release", { method: "POST", headers: auth() });
    expect(res.status).toBe(400);
  });

  it("404s when the device holds no lock", async () => {
    seedLock({ userId, gameName: "Hades", deviceId: "deck" });
    const res = await app.request("/locks/Hades/release", {
      method: "POST",
      headers: auth({ "X-Device-Id": "desktop" }),
    });
    expect(res.status).toBe(404);
  });
});

describe("POST /locks/:game/heartbeat", () => {
  it("refreshes the heartbeat of a held lock", async () => {
    seedLock({
      userId,
      gameName: "Hades",
      deviceId: "deck",
      heartbeatAgeMs: STALE_THRESHOLD_MS + 60_000,
    });
    const beat = await app.request("/locks/Hades/heartbeat", {
      method: "POST",
      headers: auth({ "X-Device-Id": "deck" }),
    });
    expect(beat.status).toBe(200);

    const body = await (await app.request("/locks/Hades", { headers: auth() })).json();
    expect(body.stale).toBe(false);
  });

  it("requires the X-Device-Id header", async () => {
    const res = await app.request("/locks/Hades/heartbeat", { method: "POST", headers: auth() });
    expect(res.status).toBe(400);
  });

  it("404s when the device holds no lock", async () => {
    const res = await app.request("/locks/Hades/heartbeat", {
      method: "POST",
      headers: auth({ "X-Device-Id": "ghost" }),
    });
    expect(res.status).toBe(404);
  });
});

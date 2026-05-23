import { Hono } from "hono";
import { v4 as uuidv4 } from "uuid";
import { queries, isStale, type Lock } from "../db.js";
import { requireAuth, type AuthEnv } from "../middleware/auth.js";

export const locksRouter = new Hono<AuthEnv>();

locksRouter.use("/*", requireAuth);

locksRouter.get("/:gameName", (c) => {
  const user = c.get("user");
  const gameName = decodeURIComponent(c.req.param("gameName"));

  const lock = queries.getLock.get(user.id, gameName);

  if (!lock) {
    return c.json({ locked: false });
  }

  const stale = isStale(lock);
  return c.json({
    locked: true,
    device_id: lock.device_id,
    device_name: lock.device_name,
    locked_at: lock.locked_at,
    stale,
  });
});

locksRouter.post("/:gameName/acquire", async (c) => {
  const user = c.get("user");
  const gameName = decodeURIComponent(c.req.param("gameName"));

  let body: { device_id?: string; device_name?: string };
  try {
    body = await c.req.json();
  } catch {
    return c.json({ error: "Invalid JSON body" }, 400);
  }

  const deviceId = body.device_id?.trim();
  const deviceName = body.device_name?.trim();
  if (!deviceId || !deviceName) {
    return c.json({ error: "device_id and device_name are required" }, 400);
  }

  const existing: Lock | undefined = queries.getLock.get(user.id, gameName);

  if (existing && existing.device_id !== deviceId && !isStale(existing)) {
    return c.json(
      {
        error: "Lock held by another device",
        device_id: existing.device_id,
        device_name: existing.device_name,
        locked_at: existing.locked_at,
      },
      409
    );
  }

  const now = new Date().toISOString();
  queries.upsertLock.run(uuidv4(), user.id, gameName, deviceId, deviceName, now, now);

  return c.json({ acquired: true });
});

locksRouter.post("/:gameName/release", (c) => {
  const user = c.get("user");
  const gameName = decodeURIComponent(c.req.param("gameName"));

  const deviceId = c.req.header("X-Device-Id") ?? "";
  if (!deviceId) {
    return c.json({ error: "X-Device-Id header required" }, 400);
  }

  const info = queries.deleteLock.run(user.id, gameName, deviceId);
  if (info.changes === 0) {
    return c.json({ error: "No lock held by this device" }, 404);
  }

  return c.json({ released: true });
});

locksRouter.post("/:gameName/heartbeat", (c) => {
  const user = c.get("user");
  const gameName = decodeURIComponent(c.req.param("gameName"));

  const deviceId = c.req.header("X-Device-Id") ?? "";
  if (!deviceId) {
    return c.json({ error: "X-Device-Id header required" }, 400);
  }

  const now = new Date().toISOString();
  const info = queries.updateHeartbeat.run(now, user.id, gameName, deviceId);
  if (info.changes === 0) {
    return c.json({ error: "No lock held by this device" }, 404);
  }

  return c.json({ ok: true });
});

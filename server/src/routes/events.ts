import { Hono } from "hono";
import { v4 as uuidv4 } from "uuid";
import { queries } from "../db.js";
import { requireAuth, type AuthEnv } from "../middleware/auth.js";

export const eventsRouter = new Hono<AuthEnv>();

eventsRouter.use("/*", requireAuth);

function recordEvent(
  userId: string,
  gameName: string,
  deviceId: string,
  deviceName: string,
  eventType: "backup" | "restore"
) {
  const now = new Date().toISOString();
  queries.insertBackupEvent.run(
    uuidv4(),
    userId,
    gameName,
    deviceId,
    deviceName,
    eventType,
    now
  );
}

eventsRouter.post("/:gameName/backup", (c) => {
  const user = c.get("user");
  const gameName = decodeURIComponent(c.req.param("gameName"));
  const deviceId = c.req.header("X-Device-Id") ?? "";
  const deviceName = c.req.header("X-Device-Name") ?? deviceId;

  if (!deviceId) {
    return c.json({ error: "X-Device-Id header required" }, 400);
  }

  recordEvent(user.id, gameName, deviceId, deviceName, "backup");
  return c.json({ recorded: true });
});

eventsRouter.post("/:gameName/restore", (c) => {
  const user = c.get("user");
  const gameName = decodeURIComponent(c.req.param("gameName"));
  const deviceId = c.req.header("X-Device-Id") ?? "";
  const deviceName = c.req.header("X-Device-Name") ?? deviceId;

  if (!deviceId) {
    return c.json({ error: "X-Device-Id header required" }, 400);
  }

  recordEvent(user.id, gameName, deviceId, deviceName, "restore");
  return c.json({ recorded: true });
});

eventsRouter.get("/:gameName/latest-backup", (c) => {
  const user = c.get("user");
  const gameName = decodeURIComponent(c.req.param("gameName"));

  const event = queries.getLatestBackupEvent.get(user.id, gameName);
  if (!event) {
    return c.json({ found: false });
  }

  return c.json({
    found: true,
    device_id: event.device_id,
    device_name: event.device_name,
    occurred_at: event.occurred_at,
  });
});

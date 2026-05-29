import { Hono } from "hono";
import fs from "node:fs";
import path from "node:path";
import { requireAuth, type AuthEnv } from "../middleware/auth.js";

// Client-facing: hands an authenticated account the connection details for the
// self-hosted WebDAV save store (served by the companion rclone container).
// The client feeds these straight into `ludusavi cloud set webdav`.
//
// Storage is opt-in per server: if the admin hasn't set WEBDAV_PUBLIC_URL there
// is no reachable WebDAV endpoint to advertise, so we report 404 and the client
// shows "this server doesn't have save storage enabled".
export const storageRouter = new Hono<AuthEnv>();

storageRouter.use("/*", requireAuth);

const savesDir = (): string => process.env.SAVES_DIR ?? "/data/saves";

storageRouter.get("/", (c) => {
  const webdavUrl = (process.env.WEBDAV_PUBLIC_URL ?? "").trim();
  if (!webdavUrl) {
    return c.json({ error: "Save storage is not enabled on this server" }, 404);
  }

  const user = c.get("user");

  // Pre-create the account's jail dir on the shared volume so the very first
  // PROPFIND/upload from rclone has somewhere to land. Best-effort: the local
  // backend also creates parents on write, so a failure here isn't fatal.
  try {
    fs.mkdirSync(path.join(savesDir(), user.id), { recursive: true });
  } catch {
    // ignore — surfaced later as a transfer error if the dir is truly unwritable
  }

  // username + api key double as the WebDAV basic-auth credentials; the rclone
  // auth-proxy validates them back against this server (see internal router).
  return c.json({
    webdav_url: webdavUrl,
    username: user.username,
    password: user.api_key,
    base_path: "ludusavi-backup",
    provider: "other",
  });
});

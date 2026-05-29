import { Hono } from "hono";
import { queries } from "../db.js";

// Internal endpoints called by the companion rclone container, not by clients.
// They're served on the same port as everything else, so they're protected by
// a shared secret header rather than by network isolation alone.
export const internalRouter = new Hono();

// rclone `--auth-proxy` contract: rclone runs a helper that pipes us the login
// `{ user, pass }` and expects, on success, an rclone backend spec describing
// where that login may read/write. We validate the credentials against the
// accounts table (pass == api_key, user == username) and return a `local`
// backend jailed to the account's own directory — giving per-account isolation
// for free without rclone needing any per-user config.
internalRouter.post("/webdav-auth", async (c) => {
  const secret = (process.env.WEBDAV_AUTH_SECRET ?? process.env.ADMIN_SECRET ?? "").trim();
  if (!secret || c.req.header("X-Internal-Secret") !== secret) {
    return c.json({ error: "Forbidden" }, 403);
  }

  let body: { user?: string; pass?: string };
  try {
    body = await c.req.json();
  } catch {
    return c.json({ error: "Invalid JSON body" }, 400);
  }

  const username = (body.user ?? "").trim();
  const apiKey = body.pass ?? "";
  const account = queries.getUserByApiKey.get(apiKey);
  if (!account || account.username !== username) {
    return c.json({ error: "Invalid credentials" }, 401);
  }

  const savesDir = process.env.SAVES_DIR ?? "/data/saves";
  return c.json({ type: "local", _root: `${savesDir}/${account.id}` });
});

import { Hono } from "hono";
import { queries } from "../db.js";
import { requireAuth, type AuthEnv } from "../middleware/auth.js";

export const lastPlayedRouter = new Hono<AuthEnv>();

lastPlayedRouter.use("/*", requireAuth);

lastPlayedRouter.get("/", (c) => {
  const user = c.get("user");
  const records = queries.getLastPlayed.all(user.id);
  return c.json(
    records.map((r) => ({
      game_name: r.game_name,
      last_played_at: r.last_played_at,
    }))
  );
});

lastPlayedRouter.post("/", async (c) => {
  const user = c.get("user");
  let body: { game_name?: string; last_played_at?: string };
  try {
    body = await c.req.json();
  } catch {
    return c.json({ error: "Invalid JSON body" }, 400);
  }

  const gameName = body.game_name?.trim();
  const lastPlayedAt = body.last_played_at?.trim();
  if (!gameName || !lastPlayedAt) {
    return c.json({ error: "game_name and last_played_at are required" }, 400);
  }

  queries.upsertLastPlayed.run(user.id, gameName, lastPlayedAt);
  return c.json({ updated: true });
});

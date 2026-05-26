import { Hono } from "hono";
import { queries } from "../db.js";
import { requireAuth, type AuthEnv } from "../middleware/auth.js";

export const playtimeRouter = new Hono<AuthEnv>();

playtimeRouter.use("/*", requireAuth);

playtimeRouter.get("/", (c) => {
  const user = c.get("user");
  const records = queries.getAllPlaytime.all(user.id);
  return c.json(
    records.map((r) => ({
      game_name: r.game_name,
      total_minutes: r.total_minutes,
    }))
  );
});

playtimeRouter.post("/:gameName", async (c) => {
  const user = c.get("user");
  const gameName = decodeURIComponent(c.req.param("gameName"));

  let body: { delta_minutes?: unknown };
  try {
    body = await c.req.json();
  } catch {
    return c.json({ error: "Invalid JSON body" }, 400);
  }

  const delta = body.delta_minutes;
  if (typeof delta !== "number" || !Number.isInteger(delta) || delta <= 0) {
    return c.json({ error: "delta_minutes must be a positive integer" }, 400);
  }

  queries.addPlaytimeDelta.run(user.id, gameName, delta);
  return c.json({ recorded: true });
});

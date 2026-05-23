import { Hono } from "hono";
import { v4 as uuidv4 } from "uuid";
import { queries } from "../db.js";

export const authRouter = new Hono();

authRouter.post("/register", async (c) => {
  const adminSecret = process.env.ADMIN_SECRET ?? "";
  const provided = c.req.header("X-Admin-Secret") ?? "";

  if (!adminSecret || provided !== adminSecret) {
    return c.json({ error: "Forbidden" }, 403);
  }

  let body: { username?: string };
  try {
    body = await c.req.json();
  } catch {
    return c.json({ error: "Invalid JSON body" }, 400);
  }

  const username = body.username?.trim();
  if (!username) {
    return c.json({ error: "username is required" }, 400);
  }

  const existing = queries.getUserByUsername.get(username);
  if (existing) {
    return c.json({ error: "Username already taken" }, 409);
  }

  const id = uuidv4();
  const apiKey = uuidv4();
  const now = new Date().toISOString();

  queries.insertUser.run(id, username, apiKey, now);

  return c.json({ api_key: apiKey }, 201);
});

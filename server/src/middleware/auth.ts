import { createMiddleware } from "hono/factory";
import { queries, type User } from "../db.js";

export type AuthEnv = { Variables: { user: User } };

export const requireAuth = createMiddleware<AuthEnv>(async (c, next) => {
  const header = c.req.header("Authorization") ?? "";
  const token = header.startsWith("Bearer ") ? header.slice(7) : "";

  if (!token) {
    return c.json({ error: "Missing authorization token" }, 401);
  }

  const user = queries.getUserByApiKey.get(token);
  if (!user) {
    return c.json({ error: "Invalid API key" }, 401);
  }

  c.set("user", user);
  await next();
});

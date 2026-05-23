import { serve } from "@hono/node-server";
import { Hono } from "hono";
import { authRouter } from "./routes/auth.js";
import { locksRouter } from "./routes/locks.js";

const app = new Hono();

app.route("/auth", authRouter);
app.route("/locks", locksRouter);

app.get("/health", (c) => c.json({ ok: true }));

const port = parseInt(process.env.PORT ?? "3000", 10);
serve({ fetch: app.fetch, port }, () => {
  console.log(`ludusavi-wrap lock server listening on :${port}`);
});

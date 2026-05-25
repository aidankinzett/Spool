import { serve } from "@hono/node-server";
import { Hono } from "hono";
import { authRouter } from "./routes/auth.js";
import { locksRouter } from "./routes/locks.js";
import { eventsRouter } from "./routes/events.js";

const app = new Hono();

app.route("/auth", authRouter);
app.route("/locks", locksRouter);
app.route("/events", eventsRouter);

const serverVersion = (process.env.APP_VERSION ?? "dev").replace(/^v/, "");

app.get("/health", (c) => c.json({ ok: true, version: serverVersion }));

const port = parseInt(process.env.PORT ?? "47633", 10);
serve({ fetch: app.fetch, port }, () => {
  console.log(`spool lock server listening on :${port}`);
});

import { Hono } from "hono";
import { authRouter } from "./routes/auth.js";
import { locksRouter } from "./routes/locks.js";
import { eventsRouter } from "./routes/events.js";
import { lastPlayedRouter } from "./routes/last-played.js";
import { playtimeRouter } from "./routes/playtime.js";
import { storageRouter } from "./routes/storage.js";
import { internalRouter } from "./routes/internal.js";

export const app = new Hono();

app.route("/auth", authRouter);
app.route("/locks", locksRouter);
app.route("/events", eventsRouter);
app.route("/last-played", lastPlayedRouter);
app.route("/playtime", playtimeRouter);
app.route("/storage", storageRouter);
app.route("/internal", internalRouter);

const serverVersion = (process.env.APP_VERSION ?? "dev").replace(/^v/, "");

app.get("/health", (c) => c.json({ ok: true, version: serverVersion }));

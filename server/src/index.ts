import { serve } from "@hono/node-server";
import { app } from "./app.js";
import { STALE_THRESHOLD_MS, sweepStaleLocks } from "./db.js";

const port = parseInt(process.env.PORT ?? "47633", 10);
serve({ fetch: app.fetch, port }, () => {
  console.log(`spool lock server listening on :${port}`);
});

// Periodically reap locks whose heartbeat has gone stale — the safety net for
// clients killed before they could release (SteamOS force-close, a crash, or a
// failed release call). `acquire` already treats a stale lock as free; this just
// keeps the table from accumulating dead rows. Runs on the stale-threshold
// cadence and is `unref`'d so it never keeps the process alive on its own.
// Lives here (not app.ts) so importing the app in tests doesn't spawn a timer.
const staleSweep = setInterval(() => {
  try {
    const removed = sweepStaleLocks();
    if (removed > 0) {
      console.log(`reaped ${removed} stale lock(s)`);
    }
  } catch (err) {
    console.error("stale-lock sweep failed:", err);
  }
}, STALE_THRESHOLD_MS);
staleSweep.unref();

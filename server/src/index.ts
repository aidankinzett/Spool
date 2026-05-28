import { serve } from "@hono/node-server";
import { app } from "./app.js";

const port = parseInt(process.env.PORT ?? "47633", 10);
serve({ fetch: app.fetch, port }, () => {
  console.log(`spool lock server listening on :${port}`);
});

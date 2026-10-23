// Reloads the plugin's frontend on a running Decky Loader without restarting
// plugin_loader, by calling DeckyPluginLoader.importPlugin over Steam's CEF
// remote debugger. Used by `bun run deploy:dev` (scripts/deploy-dev.sh) after
// it copies dist/ across.
// Only the frontend (dist/index.js) is reloaded; backend (main.py) changes
// still need `bun run deploy`.
//
// Usage: node scripts/reload-frontend.mjs [host] [cdp-port]
const [host = "192.168.86.113", port = "8081"] = process.argv.slice(2);
const PLUGIN = "Spool";

const targets = await (await fetch(`http://${host}:${port}/json/list`)).json();
const shared = targets.find((t) => t.title === "SharedJSContext");
if (!shared) {
  console.error(`No SharedJSContext target at ${host}:${port}. Is Steam running in Game Mode with CEF debugging on?`);
  process.exit(1);
}

const expression = `(async () => {
  const L = DeckyPluginLoader;
  const p = L.plugins.find((p) => p.name === ${JSON.stringify(PLUGIN)});
  if (!p) throw new Error("plugin not loaded");
  await L.importPlugin(p.name, p.version, p.loadType);
  return p.version;
})()`;

const ws = new WebSocket(shared.webSocketDebuggerUrl);
ws.onopen = () =>
  ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }));
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id !== 1) return;
  const err = m.result?.exceptionDetails;
  if (err) {
    console.error("Reload failed:", err.exception?.description ?? err.text);
    process.exit(1);
  }
  console.log(`Reloaded ${PLUGIN} (v${m.result.result.value})`);
  ws.close();
};

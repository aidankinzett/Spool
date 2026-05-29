# Decky Forced-Close Backup Plugin — Implementation Plan (Sub-project B)

> **For agentic workers:** implement task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Each task ends green (builds / type-checks) and is committed separately.

**Goal:** A Decky Loader plugin in `decky/` that, on a game **stop** event whose `unAppID` matches an un-backed-up Spool `active-session.json`, spawns `spool --backup "<game>"` outside Steam's killed process tree — closing the forced-"Exit Game" backup gap with no double-backup. Consumes Sub-project A's CLI + session-record contract; **no Rust changes**.

**Architecture:** Frontend registers `SteamClient.GameSessions.RegisterForAppLifetimeNotifications` at plugin load → on `bRunning === false` calls backend `on_app_stop(unAppID)`. Backend (runs as the `deck` user — no `_root` flag) reads `active-session.json`, matches appid + `backed_up === false`, and `subprocess.Popen`s `spool --backup` detached. Path resolution (session file + spool command) is autodetected with a settings override.

**Tech stack:** TypeScript/React (`@decky/api`, `@decky/ui`, `@decky/rollup`, pnpm v9), Python 3 backend (`decky` module). See the design doc for the full contract: `docs/superpowers/specs/2026-05-30-decky-forced-close-backup-design.md`.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `decky/plugin.json` | Manifest (name, author, `flags` *without* `_root`, `api_version: 1`, `publish`) |
| `decky/package.json` | Deps + `build`/`watch` scripts (pnpm) |
| `decky/rollup.config.js` | `export { default } from "@decky/rollup"` |
| `decky/tsconfig.json` | Frontend TS config |
| `decky/main.py` | Python backend: `class Plugin` + pure helpers |
| `decky/src/index.tsx` | Frontend: `definePlugin`, lifecycle hook, QAM panel |
| `decky/tests/test_backend.py` | Pure-helper unit tests (no Decky runtime) |
| `decky/.gitignore` | `node_modules/`, `dist/`, `*.log` |
| `decky/README.md` | Install/setup (requires Decky Loader) |
| `.github/workflows/decky.yml` | CI: pnpm build + python tests + zip artifact |

Conventions: keep matching/path logic in pure functions; the async `Plugin` methods are thin wrappers. Frontend forwards all stops; backend does the matching.

---

## Task 1: Scaffold the plugin (builds + loads)

**Files:** `decky/plugin.json`, `decky/package.json`, `decky/rollup.config.js`, `decky/tsconfig.json`, `decky/main.py`, `decky/src/index.tsx`, `decky/.gitignore`, `decky/README.md`

- [ ] **Step 1: Manifest** — `plugin.json`:
  ```json
  {
    "name": "Spool Backup",
    "author": "Aidan Kinzett",
    "flags": [],
    "api_version": 1,
    "publish": {
      "tags": ["save-management", "utility"],
      "description": "Forced-close save-backup safety net for Spool-managed games.",
      "image": "https://opengraph.githubassets.com/1/aidankinzett/Spool"
    }
  }
  ```
  Note: **no `_root`** → backend runs as the `deck` user (design decision).

- [ ] **Step 2: package.json** — mirror the current template:
  ```json
  {
    "name": "spool-backup",
    "version": "0.1.0",
    "type": "module",
    "scripts": { "build": "rollup -c", "watch": "rollup -c -w" },
    "dependencies": { "@decky/api": "^1.1.3", "react-icons": "^5.3.0", "tslib": "^2.7.0" },
    "devDependencies": {
      "@decky/ui": "^4.11.0", "@decky/rollup": "^1.0.2", "rollup": "^4.53.3",
      "typescript": "^5.6.2", "@types/react": "19.1.1", "@types/react-dom": "19.1.1"
    },
    "pnpm": { "peerDependencyRules": { "ignoreMissing": ["react", "react-dom"] } }
  }
  ```
  (Pin to whatever `decky-plugin-template@main` currently ships — verify versions when scaffolding.)

- [ ] **Step 3: rollup.config.js / tsconfig.json** — copy from the template verbatim (`rollup.config.js` re-exports `@decky/rollup`).

- [ ] **Step 4: Backend skeleton** — `main.py`:
  ```python
  import decky

  class Plugin:
      async def _main(self):
          decky.logger.info("Spool Backup loaded")
      async def _unload(self):
          decky.logger.info("Spool Backup unloaded")
      async def _uninstall(self):
          pass
  ```

- [ ] **Step 5: Frontend skeleton** — `src/index.tsx` with `definePlugin` returning a minimal QAM `PanelSection` ("Spool Backup safety net active") and an icon. No lifecycle hook yet.

- [ ] **Step 6: Build**
  Run: `cd decky && pnpm install && pnpm build`
  Expected: `dist/index.js` produced, no TS errors.

- [ ] **Step 7: Load test (hardware/VM with Decky)** — copy/symlink `decky/` into `~/homebrew/plugins/spool-backup`, restart Decky; the panel appears in the QAM. (Document the dev-deploy command in README.)

- [ ] **Step 8: Commit** — `feat(decky): scaffold Spool Backup plugin skeleton`

---

## Task 2: Backend matching + spawn (pure-testable)

**Files:** `decky/main.py`, `decky/tests/test_backend.py`

- [ ] **Step 1: Failing tests** — `tests/test_backend.py`, importing pure helpers:
  - `match_session(rec, appid)` → True only when `rec["steam_appid"] == appid and not rec["backed_up"]`.
  - `resolve_spool_cmd(settings, home, exists_fn)` → returns configured override → launcher script → `spool`.
  - `read_session(path)` → parses JSON, returns `None` on missing/invalid.
  Use a temp dir + a sample `active-session.json` fixture.

- [ ] **Step 2: Implement the pure helpers** in `main.py` (module-level functions, no `decky` import needed at call sites so tests run without the runtime — guard the `import decky` so tests can stub it, or keep helpers in a separate `lib.py` imported by both).

- [ ] **Step 3: `on_app_stop` + `backup_now`** async methods that call the helpers and `subprocess.Popen([cmd, "--backup", game], start_new_session=True, stdout=logfile, stderr=STDOUT)`. Log every branch (no-op reasons included) via `decky.logger`.

- [ ] **Step 4: Run tests** — `cd decky && python -m pytest tests/` (or `unittest`). All pass.

- [ ] **Step 5: Commit** — `feat(decky): session matching + spool --backup spawn`

---

## Task 3: Frontend lifecycle hook

**Files:** `decky/src/index.tsx`

- [ ] **Step 1:** Define `interface LifetimeNotification { unAppID: number; nInstanceID: number; bRunning: boolean; }` and `const onAppStop = callable<[appid: number], void>("on_app_stop");`.

- [ ] **Step 2:** In the `definePlugin` factory body (NOT in `Content`), register:
  ```ts
  const sub = SteamClient.GameSessions.RegisterForAppLifetimeNotifications(
    (n: LifetimeNotification) => { if (!n.bRunning) onAppStop(n.unAppID); }
  );
  ```
  and `onDismount() { sub.unregister(); }`.

- [ ] **Step 3:** Build (`pnpm build`); type-check clean.

- [ ] **Step 4: Hardware smoke** — launch a Spool-managed game, force "Exit Game"; confirm via `decky.logger` (Decky → plugin logs) that `on_app_stop` fired with the matching appid and the backup spawned. **Verify the appid equality** (open question) here.

- [ ] **Step 5: Commit** — `feat(decky): forward game-stop events to backend`

---

## Task 4: QAM UI (status + manual backup + settings)

**Files:** `decky/src/index.tsx`, `decky/main.py`

- [ ] **Step 1: Backend** — add `get_status()` (last session: game, backed_up, started_at), `get_settings()` / `set_spool_command(path)` (persist to `decky.DECKY_PLUGIN_SETTINGS_DIR/settings.json`), and `backup_now()` (spawn for the current/last session game).

- [ ] **Step 2: Frontend `Content`** panel:
  - status line (last game + backed-up state),
  - `ButtonItem` "Back up now" → `callable("backup_now")`,
  - a text field / setting for the spool command path (optional; autodetect by default),
  - optional toggle: "Notify on fallback backup".

- [ ] **Step 3: Optional toast** — backend `decky.emit("spool_backup_started", game)`; frontend `addEventListener` → `toaster.toast(...)`. Gated by the setting.

- [ ] **Step 4:** Build + type-check; manual QAM check on hardware.

- [ ] **Step 5: Commit** — `feat(decky): QAM panel — status, manual backup, settings`

---

## Task 5: CI + packaging + docs

**Files:** `.github/workflows/decky.yml`, `decky/README.md`, (optionally) `.github/workflows/release.yml`

- [ ] **Step 1: CI** — `decky.yml` on PR/push touching `decky/**`: setup pnpm v9 + Node 20, `pnpm install --frozen-lockfile`, `pnpm build`, `python -m pytest decky/tests`. Upload a `spool-backup` zip artifact laid out as Decky expects (`spool-backup/{dist/index.js, main.py, plugin.json, package.json}`).

- [ ] **Step 2: README** — install via Decky (manual zip install / dev deploy), what it does, the `backed_up` no-double-backup contract, settings, troubleshooting (where logs live), and the hardware requirement (Decky Loader on SteamOS/Bazzite).

- [ ] **Step 3 (optional): Release** — attach the plugin zip to the existing tag-triggered release, and/or document submission to the Decky plugin store (store entry can point at the `decky/` subdir or a thin mirror).

- [ ] **Step 4: Commit** — `ci(decky): build + test + package the plugin`; `docs(decky): install/setup README`.

---

## Task 6: Verification (hardware)

**Files:** none.

- [ ] **Forced close** — Deck/Bazzite Game Mode: launch a Spool game, **Exit Game** from Quick Access; confirm a `spool --backup` ran (logs) and `save_last_backed_up_at` advanced; `active-session.json.backed_up == true` afterward.
- [ ] **Normal quit** — quit in-game; confirm Spool's own backup ran and the plugin **no-ops** (record already `backed_up`). No double backup version created.
- [ ] **Non-Spool game** — launch a regular Steam game, exit; confirm the plugin no-ops (appid mismatch).
- [ ] **AppImage + native** — verify both `spool-launcher.sh --backup` and native `spool --backup` resolve.
- [ ] **Privilege** — confirm backup files are owned by `deck` (not root) and landed in the correct `~/.local/share/Spool` paths.

---

## Self-review notes

- **Spec coverage:** scaffold (T1), backend match/spawn (T2), frontend hook (T3), QAM UI + settings + toast (T4), CI/packaging/docs (T5), hardware verification (T6). All design sections mapped.
- **No Rust changes:** A's contract (session record + `--backup` + launcher) is complete and consumed read-only.
- **Carried risks (verify in T3/T6):** appid equality for non-Steam shortcuts on current SteamOS; backend privilege/`$HOME` as `deck`; detached-spawn survival across force-close.
- **Double-backup:** guarded by `backed_up`, re-read at stop; optional grace re-check; ludusavi versioning makes a stray double harmless.

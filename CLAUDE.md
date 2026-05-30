# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Spool** (formerly ludusavi-wrap) is a cross-platform (Windows + Linux) game library + save-management wrapper built with [Tauri 2](https://v2.tauri.app/) (Rust backend) and [SvelteKit 5](https://kit.svelte.dev/) frontend. Windows is the primary target, but the app also builds and runs on Linux — notably the gaming-handheld distros like **Bazzite**, **CachyOS**, and **SteamOS** (Steam Deck). Linux ships as an AppImage (see Releasing below). A few OS-integration features are Windows-only and degrade gracefully on Linux (see the *Windows-only* modules below). It maintains a persistent **game library** with cover art (via SteamGridDB) and lets users launch games directly from the app — automatically restoring saves before launch and backing them up on exit, with cloud-sync conflict detection via [ludusavi](https://github.com/mtkennerly/ludusavi). It also generates standalone launcher shortcuts for ASUS Armoury Crate and Steam, shares games over the LAN, and locks play state across devices via a self-hosted Hono (Node) sync server.

For the user-facing feature tour see [`README.md`](README.md); the self-hosted sync server is documented in [`server/README.md`](server/README.md).

## Commands

All commands run from `tauri/` unless noted.

```bash
# Install frontend dependencies (first time, or after package.json changes)
cd tauri
bun install

# Run application in development mode (hot-reload frontend + auto-rebuild backend)
bun run tauri dev

# Build a release binary + NSIS installer
bun run tauri build
# Output:
#   tauri/src-tauri/target/release/spool.exe
#   tauri/src-tauri/target/release/bundle/nsis/Spool_<version>_x64-setup.exe

# Backend-only checks
cd tauri/src-tauri
cargo check
cargo clippy
cargo test

# Frontend-only checks
cd tauri
bun run check     # svelte-check
bun run lint      # ESLint
bun run test      # Vitest unit tests

# End-to-end tests (real Tauri window via tauri-driver + WebdriverIO)
cd tauri
bun run test:e2e  # builds the app then runs the WebDriver suite
# Linux needs: libwebkit2gtk-4.1-dev, webkit2gtk-driver, and a display
# (use `xvfb-run -a bun run e2e` when headless). `cargo install tauri-driver`
# once. Windows uses msedgedriver. Specs live in tauri/e2e/specs/.

# Compile the embedded C# launcher stub (only when launcher_stub.cs changes)
# From the repo root, using the framework csc.exe shipped with Windows:
& "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe" `
    /target:winexe /win32icon:launcher_stub.ico `
    /out:launcher_stub.exe launcher_stub.cs
```

The `launcher_stub.exe` is embedded into the Rust binary via `include_bytes!` at compile time. It's a tiny .NET 4.x console exe that bounces Armoury Crate launches through `spool.exe --run`.

## Architecture

A single long-lived Tauri process owns all persistence, subprocess orchestration, OS integration, HTTP clients, and workflow state. The SvelteKit frontend is purely a view onto that state — every file IO, subprocess call, and HTTP request lives in Rust.

**Tray-resident lifecycle**: Spool runs as one long-lived process. The library window is a *view* on it — closing the window hides it to the system tray rather than quitting. Quit is **only** via the tray menu's "Quit Spool" item (`app.exit(0)`); window close and `RunEvent::ExitRequested` are otherwise prevented. Secondary `spool` invocations (from Steam shortcuts / Armoury Crate launchers) are caught by `tauri-plugin-single-instance` and forwarded as argv to the running primary — no cold-start cost on game launch.

### Rust backend (`tauri/src-tauri/src/`)

Foundation:
* **`main.rs` / `lib.rs`** — entry point, module wiring, Tauri command registration (the `generate_handler!` list is the source of truth for every IPC command), per-concern `State<T>` setup, single-instance plugin, tray icon + menu (`mount_tray`, emits `tray:show` / `tray:first-hide` / `tray:quit`), `RunEvent`/`WindowEvent` lifecycle hooks, CLI dispatch.
* **`error.rs`** — `AppError` enum + `AppResult` alias. Serialisable so errors round-trip across the IPC boundary as strings.
* **`paths.rs`** — centralised filesystem path resolution. Every module that touches an app file goes through here. Layout mirrors the legacy C# app so existing user data is picked up without migration.
* **`cli.rs`** — argv parsing for `--run "Name" "Exe"` vs normal launch. Used both at startup and by the single-instance forwarding callback.

Persistence:
* **`config.rs`** — app-wide settings persisted to `%LOCALAPPDATA%\Spool\config.json` (Ludusavi path, SteamGridDB key, theme, sync server, device identity, LAN share). On-disk shape mirrors the C# `ConfigData` exactly; `#[serde(default)]` on every field means existing files load without migration.
* **`library.rs`** — `GameEntry` + `Library` CRUD with atomic JSON saves (write-then-replace, `.bak` rotation) to `%LOCALAPPDATA%\Spool\library.json`. Sequential catalog numbers backfilled on first load. Emits `library:changed` on every mutation.

External integrations:
* **`ludusavi.rs`** — subprocess invocation of the ludusavi CLI. Owns the ~9 MB manifest cache (lazy-loaded into `Arc<HashMap>`), the search/find/enrich flow, and restore/backup invoked by the run workflow.
* **`steamgriddb.rs`** — HTTP client for SteamGridDB. Prefers Steam ID lookup (near-100% accurate) and falls back to name autocomplete. Downloads portrait covers to `%LOCALAPPDATA%\Spool\covers\` and extracts a vibrant accent colour from the image.
* **`steam.rs`** — non-Steam shortcut creation. Writes to `<steam>/userdata/<uid>/config/shortcuts.vdf` via `steam_shortcuts_util` with `--run` launch options, plus grid art placement under `grid/<appid>{suffix}.{ext}`. Uses `steamlocate` for Steam install discovery.
* **`sync.rs`** — sync-server HTTP client (Hono/Node server in `server/`). Account registration, per-game play-state lock acquire/release/heartbeat, save event recording, playtime + last-played cross-device sync. Background task polls `/health` every 30 s and emits `sync:status-changed`.
* **`lan.rs`** — the largest module (~100 KB). Two halves:
  * **Discovery** — every 5 s sends a small JSON announce packet over **UDP broadcast** (`255.255.255.255:47631`, *not* multicast — consumer mesh routers filter admin-scoped multicast but flood limited broadcasts). Peers stale out after 30 s. Announce carries the live `file_server_port`. Emits `lan:peers-changed`.
  * **Transfer** — an in-process axum HTTP server exposes `/games`, `/manifest` (walks a game folder and **blake3-hashes** every file; cached after first request), per-file `/file` downloads (HTTP range requests for resume), and cover/hero artwork. The install side (`start_peer_install` → `run_install`) downloads up to `LAN_PARALLEL_FILES` (4) files concurrently into a `<name>.partial` dir, verifies each file's blake3 hash, resumes interrupted transfers, then renames `.partial` → final and adds the game to the library. Single in-flight install slot with a cancel flag; throttled `lan:download` progress events. The serving side tracks `UploadSession`s (reaped when idle), exposed via `list_active_uploads` / `cancel_upload`, emitting `lan:uploads-changed`. Installs land in `lan_install_dir` from config, defaulting to `%LOCALAPPDATA%\Spool\lan-games\`.

Windows-only (these are `#[cfg(windows)]`-gated or no-op on Linux — the rest of the app, including the runner, library, LAN, sync, and download flows, is fully cross-platform):
* **`launcher.rs`** — extracts the embedded `launcher_stub.exe` to `%LOCALAPPDATA%\Spool\launchers\<safe_name>.exe` and appends a config payload bracketed by marker strings. The stub at runtime reads its own bytes and exec's `spool.exe --run`. Payload format matches the C# generator exactly so existing launchers stay compatible.
* **`registry.rs`** — probes HKCU + HKLM `AppCompatFlags\Layers` for the `RUNASADMIN` token so launches honour the per-exe Windows admin flag even when the library entry's own `run_as_admin` toggle is off.
* **`process.rs`** — game-process spawn. The normal path (`tokio::process::Command`) is cross-platform; only the elevated Run-As-Admin path is Windows-only — it uses the `runas` crate (`ShellExecuteExW` with the `runas` verb) wrapped in `spawn_blocking`. On Linux the elevated path returns an error (`run_as_admin` has no effect), so normal launches work everywhere.

Workflow orchestration:
* **`runner.rs`** — the marquee feature. Five-phase state machine: `restoring → launching → playing → backing-up → done`, emitting `run:phase` events at each transition. Single-launch RAII guard releases the slot even on panic. Cloud-sync conflicts during restore abort cleanly; backup failures after a successful session log-and-continue.

Startup backfills (one-shot tasks at boot, results saved once at the end):
* **`accent_backfill.rs`** — picks library entries with a cover on disk but no accent colour yet, runs `extract_vibrant_color` against each.
* **`size_backfill.rs`** — picks entries with a `game_folder_path` on disk but `install_size_mb == 0`, computes the recursive directory size via `walkdir` in `spawn_blocking`.

### SvelteKit frontend (`tauri/src/`)

Routes under `tauri/src/routes/`:
* **`+layout.svelte` / `+layout.ts`** — global chrome (frameless title bar, toast stack), theme application, cross-window event subscriptions, navigation shell.
* **`+page.svelte`** — main library window: sidebar (searchable game list, filter tabs, cover thumbnails, sync-status badges, peer-WiFi indicator) + detail panel (cover art, Play button with per-phase label, stats strip, action toolbar, About/Saves/Details cards).
* **`add/+page.svelte`** — Add Game popup (opened as a separate `WebviewWindow`). Drop or browse for an exe → ludusavi auto-identifies → ranked candidate list with confidence scores → Add to Library / Armoury Crate / Add to Steam.
* **`edit/+page.svelte`** — per-game settings dialog (install folder, run-as-admin, manual cover refresh).
* **`settings/+page.svelte`** — application settings (Ludusavi path with autodetect, SteamGridDB key, theme, LAN share, sync server, device name). Live save on commit, no Save button.

Shared code under `tauri/src/lib/`:
* **`api.ts`** — the single typed wrapper around Tauri's `invoke` IPC bridge. Every backend command is a method on the exported `api` object (e.g. `api.listGames()`, `api.startPeerInstall(...)`); components never call `invoke` directly. Also exports `assetUrl()` which wraps `convertFileSrc` for loading local files (covers, art) into the webview via the `asset:` protocol. **When you add a Rust `#[tauri::command]`, add its typed wrapper here.**
* **`types.ts`** — TypeScript mirrors of the Rust serde types (`GameEntry`, `ConfigData`, `LanPeer`, `PeerGame`, `DownloadProgress`, `SyncStatus`, `SearchCandidate`, etc.). Keep these in sync with the Rust structs they mirror.
* **`components/`** — reusable Svelte components: primitives (`Btn`, `Toggle`, `TextField`, `Pill`, `Icon`), layout/chrome (`WindowChrome`, `SettingsCard`, `SettingsRow`, `DetailCard`), the toast stack (`Toast`, `ToastStack`), the LAN transfer UI (`TransfersPanel`, `TransferPill`), `GameDetail`, `LibraryContextMenu`, `CassetteProgress`, and `CandidateRow`.
* **`toasts.svelte.ts`** — global toast store (Svelte 5 runes). `format.ts` / `tokens.ts` — display helpers (sizes, durations, design tokens). `updater.ts` — wraps `tauri-plugin-updater` (checks `latest.json`, prompts, applies). `GameCard.svelte` — sidebar/grid cover thumbnail.

### Data Files

All paths below are written as `%LOCALAPPDATA%\Spool\` (Windows). The root is resolved cross-platform via `paths::app_data_dir()` (`dirs::data_local_dir()`), so on Linux the same files live under `~/.local/share/Spool/` and on macOS under `~/Library/Application Support/Spool/`. The one-shot `ludusavi-wrap` → `Spool` rename migration only finds legacy data where it existed (effectively Windows); fresh Linux installs simply start clean.

| File | Location | Contents |
|------|----------|----------|
| `config.json` | `%LOCALAPPDATA%\Spool\` | App-wide settings (Ludusavi path, API keys, theme, sync server, LAN share, device ID) |
| `library.json` | `%LOCALAPPDATA%\Spool\` | Game library — list of `GameEntry` objects |
| `covers/` | `%LOCALAPPDATA%\Spool\` | Downloaded SteamGridDB cover images |
| `launchers/` | `%LOCALAPPDATA%\Spool\` | Generated per-game `.exe` launcher stubs (Armoury Crate) |
| `lan-games/` | `%LOCALAPPDATA%\Spool\` | Default install root for games downloaded from LAN peers (overridable via `lan_install_dir` config) |
| `debug.log` | `%LOCALAPPDATA%\Spool\` | App log (errors, startup events) |

### Key Patterns

* **Per-concern Tauri `State<T>`**: every command declares its dependencies as parameters (e.g. `library: State<'_, SharedLibrary>`, `config: State<'_, SharedConfig>`) — explicit, compiler-enforced, refactor-friendly. No single `AppState` god object.
* **Lock discipline**: never hold a `std::sync::Mutex` guard across `.await`. Every async command snapshots what it needs from state, drops the guard, then awaits. If state must cross an await point, that specific state moves to `tokio::sync::Mutex`.
* **Atomic JSON saves**: `library.rs` writes to a temp file then `rename`s over the target, rotating a `.bak` of the previous good file. Survives crash mid-write without corrupting either copy.
* **`AppHandle::emit` cross-window broadcast**: events go to all open webviews. The Add Game popup mutating the library triggers a refresh in the main window for free — no targeted emit needed. Current events: `library:changed`, `run:phase`, `sync:status-changed`, `lan:peers-changed`, `lan:download`, `lan:uploads-changed`, and the tray events `tray:show` / `tray:first-hide` / `tray:quit`.
* **Event naming**: colon-namespaced (`library:changed`, `run:phase`). Tauri 2 rejects `.` in event names at runtime (allowed charset is `[A-Za-z0-9_\-/:]+`).
* **RAII run-lock**: `runner.rs` acquires a single-launch guard whose `Drop` impl releases the slot. Releases on panic too — without it a crashed workflow would leave Spool unable to launch any game until restart.
* **Backfill tasks**: legacy library entries (pre-rename `ludusavi-wrap` users) lack newer fields like `accent_color` or `install_size_mb`. `accent_backfill.rs` and `size_backfill.rs` walk the library at startup, fill in the gaps via `walkdir` + colour extraction, save once at the end, emit `library:changed` so the UI repaints.
* **axum LAN server**: `lan.rs` runs a tiny axum HTTP server in-process exposing `/games`, `/manifest`, file downloads, and cover art. Bind tries the user's preferred port first, falls back to ephemeral if taken — so multiple Spool instances on the same box still come up clean.
* **Content-addressed LAN transfer**: the host blake3-hashes each file (`/manifest`); the installer verifies hashes per file and resumes via HTTP range requests into a `.partial` dir, renaming to final only on full success — an interrupted transfer is safe to retry.
* **`tracing` for logs**: the `tracing` crate with a file appender writes to `%LOCALAPPDATA%\Spool\debug.log` (same path as the C# app for continuity). Spans wrap RunWorkflow phases for clean trace output.
* **JSON shape compatibility with the legacy C# app**: both `library.json` and `config.json` round-trip cleanly because every field carries `#[serde(default)]`. Existing users get a zero-touch upgrade.

## Sync server (`server/`)

A small self-hostable [Hono](https://hono.dev/) app (TypeScript) served via `@hono/node-server` on Node, backed by SQLite through `better-sqlite3`. Its job is to stop two devices playing the same game at once and to sync playtime / last-played / save events. Mounted routers in `src/index.ts`:

| Route | Purpose |
|-------|---------|
| `/health` | Liveness + version probe (polled by the app every 30 s). |
| `/auth` | Admin-secret-gated account registration → returns an API key. |
| `/locks` | Per-game play-state lock acquire / release / heartbeat. |
| `/events` | Save-event recording. |
| `/last-played` | Cross-device last-played sync. |
| `/playtime` | Cross-device playtime accumulation. |

`src/db.ts` owns schema + queries; `src/middleware/auth.ts` validates the API key. Run locally with `npm run dev` (tsx, from `server/`); self-host via the `Dockerfile` / `docker-compose.yml` (see `server/README.md`). The Rust client is `tauri/src-tauri/src/sync.rs`.

## CI / CD

GitHub Actions workflows in `.github/workflows/`:
* **`ci.yml`** — runs on push to `master` and on PRs. A `build-windows` job (Windows runner) builds the backend and runs clippy/check/test plus the frontend checks; a `build-linux` job (Ubuntu, push-only) does a release-profile Linux compile to smoke-test the Linux build and warm its cache; an `e2e-linux` job runs the WebDriver end-to-end suite under Xvfb; and a `server` job runs the sync-server tests. `sccache` + `Swatinem/rust-cache` keep it fast. The push/PR split is deliberate so the cache saves to the default-branch scope that the tag-triggered release build can later restore.
* **`release.yml`** — tag-triggered release build. Builds **both** the Windows NSIS installer (`build-windows`) and the Linux AppImage (`build-linux`, with a Wayland-library strip + repack so it runs on Bazzite/CachyOS/SteamOS), then a `release` job publishes both plus a combined `latest.json` updater manifest (see Releasing below).
* **`server-publish.yml`** — on `v*` tags, builds and pushes the sync-server Docker image to GHCR, but only when `server/` actually changed since the previous tag.
* **`debug-token.yml`** — manual token/permissions diagnostics.

## Releasing

Releases are fully automated via `.github/workflows/release.yml` and triggered by pushing a version tag.

**Steps to release a new version:**

```powershell
# 1. Ensure all changes are committed and pushed to master
git checkout master
git pull

# 2. Create an annotated tag (triggers the release workflow)
git tag v5.0.1 -m "v5.0.1"
git push origin master v5.0.1
```

**What the workflow does automatically (no manual steps needed):**

The workflow has three jobs: **`build-windows`** and **`build-linux`** compile in parallel, then **`release`** (`needs: [build-windows, build-linux]`) assembles the GitHub Release from their uploaded artifacts.

`build-windows` (Windows runner):
1. Stamps the tag version into `tauri/src-tauri/tauri.conf.json` and `tauri/src-tauri/Cargo.toml` in-CI (no back-commit — Tauri's updater reads the version from the bundle metadata, not a static `update.xml`).
2. Compiles the launcher stub (`launcher_stub.cs` → `launcher_stub.exe`) using the runner's framework `csc.exe`.
3. Runs [`tauri-apps/tauri-action`](https://github.com/tauri-apps/tauri-action) with `--bundles nsis` to produce the NSIS installer + detached `.sig` (signed with `TAURI_SIGNING_PRIVATE_KEY`), and uploads them as the `windows-bundle` artifact.

`build-linux` (Ubuntu 22.04 runner):
4. Stamps the version the same way, installs the GTK/WebKit system deps, and runs `tauri-action` with `--bundles appimage` to produce `Spool_*_amd64.AppImage` + `.sig`.
5. **Strips the bundled `libwayland-*` libraries and repacks the AppImage**, then re-signs it. linuxdeploy's GTK plugin over-bundles host `libwayland-client/cursor/egl/server`; on Wayland sessions with newer Mesa (Bazzite, CachyOS, SteamOS, modern Fedora) the stale bundled `libwayland-client` aborts WebKit with `EGL_BAD_PARAMETER` before render. Stripping them lets WebKit fall back to the host's matching libs. Uploads the repacked AppImage + `.sig` as the `linux-bundle` artifact.

`release` (Ubuntu runner, after both builds):
6. Generates a categorised changelog from commit messages since the previous tag (Features / Bug Fixes / Other).
7. Downloads both bundle artifacts and creates the GitHub Release via `gh release create` with a PAT (`RELEASE_TOKEN`) — tauri-action's own release path uses the default `GITHUB_TOKEN`, which 403s on this repo — attaching the Windows installer + `.sig` and the Linux AppImage + `.sig`.
8. Synthesizes `latest.json` itself from the uploaded artifacts — a Tauri v2 updater manifest with both `windows-x86_64` and `linux-x86_64` platforms — and `gh release upload --clobber`s it. This is what `tauri-plugin-updater` in the app fetches from `https://github.com/aidankinzett/Spool/releases/latest/download/latest.json`.

**Version number conventions:**
- The app version is derived entirely from the git tag — there is no hardcoded version string in source code.
- Use `vMAJOR.MINOR.PATCH` format (e.g. `v5.0.1`).
- Skip patch numbers if needed — there is no strict requirement to be sequential.

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Spool** (formerly ludusavi-wrap) is a Windows-native game library + save-management wrapper built with [Tauri 2](https://v2.tauri.app/) (Rust backend) and [SvelteKit 5](https://kit.svelte.dev/) frontend. It maintains a persistent **game library** with cover art (via SteamGridDB) and lets users launch games directly from the app — automatically restoring saves before launch and backing them up on exit, with cloud-sync conflict detection via [ludusavi](https://github.com/mtkennerly/ludusavi). It also generates standalone launcher shortcuts for ASUS Armoury Crate and Steam, shares games over the LAN, locks play state across devices via a Bun/Hono sync server, and downloads games from Hydra-format catalogues via TorBox.

For design rationale see [`tauri/BACKEND.md`](tauri/BACKEND.md); for the port retrospective see [`tauri/REWRITE.md`](tauri/REWRITE.md).

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

# Compile the embedded C# launcher stub (only when launcher_stub.cs changes)
# From the repo root, using the framework csc.exe shipped with Windows:
& "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe" `
    /target:winexe /win32icon:launcher_stub.ico `
    /out:launcher_stub.exe launcher_stub.cs
```

The `launcher_stub.exe` is embedded into the Rust binary via `include_bytes!` at compile time. It's a tiny .NET 4.x console exe that bounces Armoury Crate launches through `spool.exe --run`.

## Architecture

A single long-lived Tauri process owns all persistence, subprocess orchestration, OS integration, HTTP clients, and workflow state. The SvelteKit frontend is purely a view onto that state — every file IO, subprocess call, and HTTP request lives in Rust.

### Rust backend (`tauri/src-tauri/src/`)

Foundation:
* **`main.rs` / `lib.rs`** — entry point, module wiring, Tauri command registration, per-concern `State<T>` setup, single-instance plugin, tray (planned), CLI dispatch.
* **`error.rs`** — `AppError` enum + `AppResult` alias. Serialisable so errors round-trip across the IPC boundary as strings.
* **`paths.rs`** — centralised filesystem path resolution. Every module that touches an app file goes through here. Layout mirrors the legacy C# app so existing user data is picked up without migration.
* **`cli.rs`** — argv parsing for `--run "Name" "Exe"` vs normal launch. Used both at startup and by the single-instance forwarding callback.

Persistence:
* **`config.rs`** — app-wide settings persisted to `%LOCALAPPDATA%\Spool\config.json` (Ludusavi path, SteamGridDB key, theme, sync server, device identity, LAN share, TorBox, download sources). On-disk shape mirrors the C# `ConfigData` exactly; `#[serde(default)]` on every field means existing files load without migration.
* **`library.rs`** — `GameEntry` + `Library` CRUD with atomic JSON saves (write-then-replace, `.bak` rotation) to `%LOCALAPPDATA%\Spool\library.json`. Sequential catalog numbers backfilled on first load. Emits `library:changed` on every mutation.

External integrations:
* **`ludusavi.rs`** — subprocess invocation of the ludusavi CLI. Owns the ~9 MB manifest cache (lazy-loaded into `Arc<HashMap>`), the search/find/enrich flow, and restore/backup invoked by the run workflow.
* **`steamgriddb.rs`** — HTTP client for SteamGridDB. Prefers Steam ID lookup (near-100% accurate) and falls back to name autocomplete. Downloads portrait covers to `%LOCALAPPDATA%\Spool\covers\` and extracts a vibrant accent colour from the image.
* **`steam.rs`** — non-Steam shortcut creation. Writes to `<steam>/userdata/<uid>/config/shortcuts.vdf` via `steam_shortcuts_util` with `--run` launch options, plus grid art placement under `grid/<appid>{suffix}.{ext}`. Uses `steamlocate` for Steam install discovery.
* **`hydra.rs`** — fetches and merges user-configured Hydra-format JSON catalogues (community game-download lists) for the Browse Games window.
* **`torbox.rs`** — HTTP client for the TorBox debrid service (POST magnet → poll until cached → request signed per-file URL). Pure request wrappers exposed as Tauri commands.
* **`sync.rs`** — sync-server HTTP client (Bun/Hono server in `server/`). Account registration, per-game play-state lock acquire/release/heartbeat, save event recording, playtime + last-played cross-device sync. Background task polls `/health` every 30 s and emits `sync:status-changed`.
* **`lan.rs`** — LAN peer discovery (UDP multicast on `239.255.83.83:47631`) + an axum HTTP file server that exposes the game library and serves files for transfer. Peers appear automatically in the library grid; downloads add the game on completion.

Windows-only:
* **`launcher.rs`** — extracts the embedded `launcher_stub.exe` to `%LOCALAPPDATA%\Spool\launchers\<safe_name>.exe` and appends a config payload bracketed by marker strings. The stub at runtime reads its own bytes and exec's `spool.exe --run`. Payload format matches the C# generator exactly so existing launchers stay compatible.
* **`registry.rs`** — probes HKCU + HKLM `AppCompatFlags\Layers` for the `RUNASADMIN` token so launches honour the per-exe Windows admin flag even when the library entry's own `run_as_admin` toggle is off.
* **`process.rs`** — game-process spawn. Normal path uses `tokio::process::Command`; elevated path uses the `runas` crate (`ShellExecuteExW` with the `runas` verb) wrapped in `spawn_blocking` so the blocking wait doesn't tie up the tokio runtime.

Workflow orchestration:
* **`runner.rs`** — the marquee feature. Five-phase state machine: `restoring → launching → playing → backing-up → done`, emitting `run:phase` events at each transition. Single-launch RAII guard releases the slot even on panic. Cloud-sync conflicts during restore abort cleanly; backup failures after a successful session log-and-continue.

Download orchestration:
* **`browse_download.rs`** — drives file transfers when the user clicks Download in the Browse Games window. Picks TorBox for `magnet:` URIs and direct HTTP otherwise. Single in-flight slot, cancel flag, throttled `browse:download` progress events.

Startup backfills (one-shot tasks at boot, results saved once at the end):
* **`accent_backfill.rs`** — picks library entries with a cover on disk but no accent colour yet, runs `extract_vibrant_color` against each.
* **`size_backfill.rs`** — picks entries with a `game_folder_path` on disk but `install_size_mb == 0`, computes the recursive directory size via `walkdir` in `spawn_blocking`.

### SvelteKit frontend (`tauri/src/`)

Routes under `tauri/src/routes/`:
* **`+layout.svelte` / `+layout.ts`** — global chrome (frameless title bar, toast stack), theme application, cross-window event subscriptions, navigation shell.
* **`+page.svelte`** — main library window: sidebar (searchable game list, filter tabs, cover thumbnails, sync-status badges, peer-WiFi indicator) + detail panel (cover art, Play button with per-phase label, stats strip, action toolbar, About/Saves/Details cards).
* **`add/+page.svelte`** — Add Game popup (opened as a separate `WebviewWindow`). Drop or browse for an exe → ludusavi auto-identifies → ranked candidate list with confidence scores → Add to Library / Armoury Crate / Add to Steam.
* **`browse/+page.svelte`** — Browse Games window. Three-pane Hydra catalogue browser with searchable/filterable list, detail pane, and inline download progress.
* **`edit/+page.svelte`** — per-game settings dialog (install folder, run-as-admin, manual cover refresh).
* **`settings/+page.svelte`** — application settings (Ludusavi path with autodetect, SteamGridDB key, theme, LAN share, sync server, TorBox, download sources, device name). Live save on commit, no Save button.

### Data Files

| File | Location | Contents |
|------|----------|----------|
| `config.json` | `%LOCALAPPDATA%\Spool\` | App-wide settings (Ludusavi path, API keys, theme, sync server, LAN share, TorBox, device ID) |
| `library.json` | `%LOCALAPPDATA%\Spool\` | Game library — list of `GameEntry` objects |
| `covers/` | `%LOCALAPPDATA%\Spool\` | Downloaded SteamGridDB cover images |
| `launchers/` | `%LOCALAPPDATA%\Spool\` | Generated per-game `.exe` launcher stubs (Armoury Crate) |
| `debug.log` | `%LOCALAPPDATA%\Spool\` | App log (errors, startup events) |

### Key Patterns

* **Per-concern Tauri `State<T>`**: every command declares its dependencies as parameters (e.g. `library: State<'_, SharedLibrary>`, `config: State<'_, SharedConfig>`) — explicit, compiler-enforced, refactor-friendly. No single `AppState` god object.
* **Lock discipline**: never hold a `std::sync::Mutex` guard across `.await`. Every async command snapshots what it needs from state, drops the guard, then awaits. If state must cross an await point, that specific state moves to `tokio::sync::Mutex`.
* **Atomic JSON saves**: `library.rs` writes to a temp file then `rename`s over the target, rotating a `.bak` of the previous good file. Survives crash mid-write without corrupting either copy.
* **`AppHandle::emit` cross-window broadcast**: events like `library:changed`, `run:phase`, `sync:status-changed`, `browse:download` go to all open webviews. The Add Game popup mutating the library triggers a refresh in the main window for free — no targeted emit needed.
* **Event naming**: colon-namespaced (`library:changed`, `run:phase`). Tauri 2 rejects `.` in event names at runtime (allowed charset is `[A-Za-z0-9_\-/:]+`).
* **RAII run-lock**: `runner.rs` acquires a single-launch guard whose `Drop` impl releases the slot. Releases on panic too — without it a crashed workflow would leave Spool unable to launch any game until restart.
* **Backfill tasks**: legacy library entries (pre-rename `ludusavi-wrap` users) lack newer fields like `accent_color` or `install_size_mb`. `accent_backfill.rs` and `size_backfill.rs` walk the library at startup, fill in the gaps via `walkdir` + colour extraction, save once at the end, emit `library:changed` so the UI repaints.
* **axum LAN server**: `lan.rs` runs a tiny axum HTTP server in-process exposing `/games`, file downloads, and cover art. Bind tries the user's preferred port first, falls back to ephemeral if taken — so multiple Spool instances on the same box still come up clean.
* **`tracing` for logs**: the `tracing` crate with a file appender writes to `%LOCALAPPDATA%\Spool\debug.log` (same path as the C# app for continuity). Spans wrap RunWorkflow phases for clean trace output.
* **JSON shape compatibility with the legacy C# app**: both `library.json` and `config.json` round-trip cleanly because every field carries `#[serde(default)]`. Existing users get a zero-touch upgrade.

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

1. Stamps the tag version into `tauri/src-tauri/tauri.conf.json` and `tauri/src-tauri/Cargo.toml` in-CI (no back-commit — Tauri's updater reads the version from the bundle metadata, not a static `update.xml`).
2. Generates a categorised changelog from commit messages since the previous tag (Features / Bug Fixes / Other).
3. Compiles the launcher stub (`launcher_stub.cs` → `launcher_stub.exe`) using the runner's framework `csc.exe`.
4. Installs frontend dependencies with `bun install --frozen-lockfile`.
5. Runs [`tauri-apps/tauri-action`](https://github.com/tauri-apps/tauri-action) with `--bundles nsis` to produce the NSIS installer plus the updater artifacts (`.sig` + `latest.json`). The bundle is signed with `TAURI_SIGNING_PRIVATE_KEY` so the Tauri updater accepts it.
6. Creates a GitHub Release with the generated changelog and attaches the installer + updater artifacts.

**Version number conventions:**
- The app version is derived entirely from the git tag — there is no hardcoded version string in source code.
- Use `vMAJOR.MINOR.PATCH` format (e.g. `v5.0.1`).
- Skip patch numbers if needed — there is no strict requirement to be sequential.

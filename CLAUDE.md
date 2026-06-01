# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Spool** is a cross-platform (Windows + Linux) game library + save-management wrapper built with [Tauri 2](https://v2.tauri.app/) (Rust backend) and [SvelteKit 5](https://kit.svelte.dev/) frontend. Windows and Linux are both primary targets — Linux notably on the gaming-handheld distros like **Bazzite**, **CachyOS**, and **SteamOS** (Steam Deck). Linux ships as an AppImage (see Releasing below). A few OS-integration features are platform-specific and degrade gracefully on the other OS (see the *Windows-only* and *Linux-only* module groups below). It maintains a persistent **game library** with cover art (via SteamGridDB) and lets users launch games directly from the app — automatically restoring saves before launch and backing them up on exit, with cloud-save sync and conflict detection via [ludusavi](https://github.com/mtkennerly/ludusavi) + bundled [rclone](https://rclone.org/). On Linux it launches Windows `.exe` games through **Proton** (umu-launcher). It also generates launcher shortcuts for ASUS Armoury Crate (Windows) and Steam, shares games over the LAN, warns when another device has an unsynced game session (advisory, via small JSON markers stored in the same rclone remote used for cloud saves), and adapts its UI between a desktop layout and a big-target **touch layout** for handhelds. The UI is dark-only (no theme switcher).

For the user-facing feature tour see [`README.md`](README.md).

## Writing style (docs, comments, commit messages)

Explain how things work, plainly. Don't editorialise about the design being good. Avoid self-congratulatory framing — phrases like "the key insight", "the clever/elegant/brilliant part", "the magic is", "the trick here" — and don't praise the project's own choices. State what the code does and why; let the reader judge whether it's smart. Likewise, don't disparage the tools Spool depends on (e.g. ludusavi, rclone, Proton) — describe their behaviour neutrally rather than framing it as a flaw Spool works around.

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
cargo clippy --all-targets -- -D warnings  # CI fails on any warning
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
# From tauri/src-tauri/, using the framework csc.exe shipped with Windows:
& "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe" `
    /target:winexe /win32icon:launcher_stub.ico `
    /out:launcher_stub.exe launcher_stub.cs
```

The `launcher_stub.exe` is embedded into the Rust binary via `include_bytes!` at compile time. It's a tiny .NET 4.x console exe that bounces Armoury Crate launches through `spool.exe --run`.

**Bundled sidecars**: `ludusavi` and `rclone` ship as Tauri sidecars (`externalBin` in `tauri.conf.json`, living under `tauri/src-tauri/binaries/`) — users don't install them separately. The Linux Proton runner additionally relies on **umu-launcher** (`umu-run`), which is *not* bundled and is expected on the host (Settings → Compatibility runs a dependency doctor; see `diagnostics.rs`).

## Architecture

A single long-lived Tauri process owns all persistence, subprocess orchestration, OS integration, HTTP clients, and workflow state. The SvelteKit frontend is purely a view onto that state — every file IO, subprocess call, and HTTP request lives in Rust.

**Tray-resident lifecycle**: Spool runs as one long-lived process. The library window is a *view* on it — closing the window hides it to the system tray rather than quitting. Quit is **only** via the tray menu's "Quit Spool" item (`app.exit(0)`); window close and `RunEvent::ExitRequested` are otherwise prevented. Secondary `spool` invocations (from Steam shortcuts / Armoury Crate launchers) are caught by `tauri-plugin-single-instance` and forwarded as argv to the running primary — no cold-start cost on game launch.

### Rust backend (`tauri/src-tauri/src/`)

Foundation:
* **`main.rs` / `lib.rs`** — entry point, module wiring, Tauri command registration (the `generate_handler!` list is the source of truth for every IPC command), per-concern `State<T>` setup, single-instance plugin, tray icon + menu (`mount_tray`, emits `tray:show` / `tray:first-hide` / `tray:quit`), `RunEvent`/`WindowEvent` lifecycle hooks, CLI dispatch.
* **`error.rs`** — `AppError` enum + `AppResult` alias. Serialisable so errors round-trip across the IPC boundary as strings.
* **`paths.rs`** — centralised filesystem path resolution. Every module that touches an app file goes through here.
* **`cli.rs`** — argv parsing for the headless subcommands (`--run "Name" "Exe"`, `--backup "Name"`, `--release-lock "Name"`) vs normal launch. Used both at startup and by the single-instance forwarding callback.

Persistence:
* **`config.rs`** — app-wide settings persisted to `%LOCALAPPDATA%\Spool\config.json`: binary paths (`ludusavi_path`, `umu_run_path`, `rclone_path`), Proton (`default_proton_path`), cloud saves (`cloud_provider`, `cloud_remote`, `cloud_webdav_url`/`_username`, `cloud_base_path` — the base remote folder: saves go to `<base>/ludusavi-backup`, Spool's control plane to `<base>/_spool`; `rclone_args`), SteamGridDB (`steamgriddb_enabled`, `steamgriddb_api_key`), UI density (`ui_mode`), LAN share (`lan_share_enabled`, `lan_share_port`, `lan_install_dir`, `lan_download_max_mbps`), and `device_name`. The cloud / LAN / Proton-launch fields are grouped into `CloudConfig` / `LanConfig` / `LaunchConfig` sub-structs, `#[serde(flatten)]`ed so the on-disk JSON keeps its flat `cloud_*` / `lan_*` keys (existing files and the frontend's flat mirror are unchanged). Each config struct carries a **container-level** `#[serde(default)]` so missing keys fall back to the struct's `Default` and older files load without migration — apply it at the struct level, never per-field (a per-field `#[serde(default)]` shadows the struct's custom `Default` values with the field-type default). We don't keep dead fields for legacy round-trip: a field the app no longer uses is removed from the struct rather than retained for compatibility. (`cloud_path` survives only because `migrate_cloud_base_path` still reads it to migrate pre-`cloud_base_path` configs; the old `sync_server_*` and `theme` fields are gone.)
* **`library.rs`** — `GameEntry` + `Library` CRUD with atomic JSON saves (write-then-replace, `.bak` rotation) to `%LOCALAPPDATA%\Spool\library.json`. Sequential catalog numbers backfilled on first load. Emits `library:changed` on every mutation.

External integrations:
* **`ludusavi.rs`** — subprocess invocation of the bundled ludusavi CLI. Owns the ~9 MB manifest cache (lazy-loaded into `Arc<HashMap>`), the search/find/enrich flow, and restore/backup invoked by the run workflow.
* **`ludusavi_config.rs`** — Spool owns ludusavi's `config.yaml` (under Spool's app-data dir) so it controls the backup/restore paths, the cloud remote, retention, and per-restore redirects. Enforces invariants atomically (tmp → rename + `.bak`): `manifest.enable`, matching `backup.path`/`restore.path` (required for cloud sync), `backup.format.chosen: simple`, retention `full: 3`/`differential: 0`, and a `cloud:` block. Wires up rclone for the cloud providers exposed in Settings (Dropbox, Google Drive, OneDrive, Box, FTP, SMB, WebDAV, or a custom remote) and injects rclone connection/retry timeouts so an unreachable remote fails in seconds rather than blocking a Game-Mode boot.
* **`steamgriddb.rs`** — HTTP client for SteamGridDB. Prefers Steam ID lookup (near-100% accurate) and falls back to name autocomplete. Downloads portrait covers to `%LOCALAPPDATA%\Spool\covers\` and extracts a vibrant accent colour from the image.
* **`metadata.rs`** — Steam Store metadata enrichment (description, developer, publisher, genres, release date) via the public `appdetails` endpoint, keyed by the `steam_id` ludusavi resolves at add-time. No API key needed. The fields map onto the corresponding `GameEntry` fields rendered by `GameDetail.svelte`, and only blanks are filled — a manual edit is never clobbered. The endpoint is rate-limited (~200 req / 5 min), so the add-game path fires a single best-effort request and the bulk work is throttled in `metadata_backfill.rs`.
* **`steam.rs`** — non-Steam shortcut creation. Writes to `<steam>/userdata/<uid>/config/shortcuts.vdf` via `steam_shortcuts_util` with `--run` launch options, plus grid art placement under `grid/<appid>{suffix}.{ext}`. Uses `steamlocate` for Steam install discovery.
* **`rclone.rs`** — rclone-backed cross-device control plane (replaces the old HTTP sync server). Stores small JSON blobs in the *same* rclone remote used for cloud saves: per-game **session markers** (`_spool/sessions/<blake3(name)>.json`) that drive the advisory "another device has an unsynced session" launch warning, and per-device blobs (`_spool/devices/<id>.json`) folded at startup for cross-device playtime / last-played / the save-backup badge. Reads markers with `rclone cat` (read-after-write consistent), writes with `rclone rcat`, and probes `rclone lsd` every 60 s for reachability — emitting `sync:status-changed`. No accounts/auth: the remote itself is the trust boundary.
* **`lan/`** — the LAN game-sharing subsystem (a directory module: `mod.rs`, `discovery.rs`, `server.rs`, `install.rs`):
  * **`discovery.rs`** — every 5 s sends a small JSON announce packet over **UDP broadcast** (`255.255.255.255:47631`, *not* multicast — consumer mesh routers filter admin-scoped multicast but flood limited broadcasts). Peers stale out after 30 s. Announce carries the live `file_server_port`. Emits `lan:peers-changed`.
  * **`server.rs`** — an in-process axum HTTP server exposes `/games`, `/manifest` (walks a game folder and **blake3-hashes** every file; cached after first request), per-file `/file` downloads (HTTP range requests for resume), and cover/hero artwork. Binds the user's preferred port (`lan_share_port`, default 47632) or an ephemeral fallback. Tracks `UploadSession`s (reaped when idle), exposed via `list_active_uploads` / `cancel_upload`, emitting `lan:uploads-changed`.
  * **`install.rs`** — the receiver side (`start_peer_install` → `run_install`) downloads up to `LAN_PARALLEL_FILES` (4) files concurrently into a `<name>.partial` dir, verifies each file's blake3 hash, resumes interrupted transfers, then renames `.partial` → final and adds the game to the library. Single in-flight install slot with a cancel flag; throttled `lan:download` progress events. Honours `lan_download_max_mbps` (0 = unlimited). Installs land in `lan_install_dir` from config, defaulting to `%LOCALAPPDATA%\Spool\lan-games\`.

Windows-only (these are `#[cfg(windows)]`-gated or no-op on Linux — the rest of the app, including the runner, library, LAN, sync, and download flows, is fully cross-platform):
* **`launcher.rs`** — extracts the embedded `launcher_stub.exe` to `%LOCALAPPDATA%\Spool\launchers\<safe_name>.exe` and appends a config payload bracketed by marker strings. The stub at runtime reads its own bytes and exec's `spool.exe --run`.
* **`registry.rs`** — probes HKCU + HKLM `AppCompatFlags\Layers` for the `RUNASADMIN` token so launches honour the per-exe Windows admin flag even when the library entry's own `run_as_admin` toggle is off.
* **`process.rs`** — game-process spawn. The normal path (`tokio::process::Command`) is cross-platform; only the elevated Run-As-Admin path is Windows-only — it uses the `runas` crate (`ShellExecuteExW` with the `runas` verb) wrapped in `spawn_blocking`. On Linux the elevated path returns an error (`run_as_admin` has no effect), so normal launches work everywhere. Also owns `strip_appimage_env` (used by `system_open.rs` and the Proton runner) to spawn child processes with the AppImage's bundled `LD_LIBRARY_PATH`/`GTK_*` env removed so they see the host runtime.

Linux-only (Proton, Steam Deck / SteamOS integration — `#[cfg(target_os = "linux")]`-gated, no-op or unsupported elsewhere):
* **`proton.rs`** — launches Windows `.exe` games on Linux via **umu-launcher** (`umu-run`). Each game gets its own isolated Wine prefix under `~/.local/share/Spool/prefixes/<game_id>/` (mirroring Steam's compatdata-per-appid model). Passes `GAMEID`, `WINEPREFIX`, and optional `PROTONPATH`. Auto-detects installed Proton builds (stock Steam Proton, GE-Proton, UMU-Proton) from the Steam dirs, preferring the newest. Includes a winetricks helper (`umu-run winetricks -q <verbs>`) for installing Windows runtime deps into a prefix.
* **`gamemode.rs`** — detects SteamOS/Deck **Game Mode** (gamescope) via `$GAMESCOPE_WAYLAND_DISPLAY` (overridable with `$SPOOL_ATTACHED_LAUNCH`). When true, a `--run` launch switches to *attached* mode: Spool runs the workflow then exits immediately so Steam registers the game as stopped, rather than staying tray-resident. Not the Feral `gamemoded` daemon — purely a launch-behaviour signal.
* **`session.rs`** — writes a JSON session record (`game`, `steam_appid`, `session_id`, `started_at`, `backed_up`) on a Game-Mode `--run` launch and flips `backed_up` once backup completes. The Decky plugin reads it on game-stop: if still false, Steam force-killed Spool before backup, so the plugin runs `spool --release-lock` then `spool --backup` as a fallback (flags this device's session as unsynced in the remote, then captures + uploads the save and clears the marker).
* **`decky_install.rs`** — one-click installer for the companion **Decky Loader** "Spool Backup" plugin (TS frontend + Python backend, embedded in the binary). In Game Mode, exiting a game via Quick Access force-kills Spool before post-session backup; the plugin runs `spool --release-lock` + `spool --backup` outside the game's process tree as a safety net (flags the session unsynced, then backs up + clears the marker). Installs to `~/homebrew/plugins/spool-backup` via `pkexec` and restarts the `plugin_loader` service. Reports a status struct (`supported`, `installed`, `installed_version`, `bundled_version`, `decky_present`).
* **`plugin_server.rs`** *(Unix-only, `#[cfg(unix)]`)* — loopback HTTP server started by `spool --headless-server` so the Decky plugin can query library/session state, serve cover art, and trigger backup operations over local HTTP instead of spawning a `spool --backup` / `spool --release-lock` subprocess per operation. Binds a loopback port (prefers 47650, ephemeral fallback) and writes it to `~/.local/share/Spool/plugin-http-port` for the plugin's Python backend and React UI to read; an absent port file means the server isn't running. Config + library are reloaded from disk on every request (not cached) so the running GUI's changes are always visible.
* **`suspend.rs`** *(`#[cfg(target_os = "linux")]`, no-op elsewhere)* — system-suspend watcher for the unsynced-session marker. When a Deck sleeps mid-session every process freezes and the `rclone` heartbeat stops, so the marker would age out to "stale". The watcher subscribes to systemd-logind's `PrepareForSleep` D-Bus signal, holding a logind *delay* inhibitor lock until it has flipped the marker to *suspended* (which never goes stale), then releases the lock so sleep proceeds; on resume it re-asserts an awake marker and warns if a peer took the session over. The per-session task is aborted by `run_workflow` when the session ends.
* **`redirects.rs`** — cross-platform save-path mapping. Parses a backup's `mapping.yaml` and generates ludusavi `restore` redirect rules that map foreign Windows paths / Proton prefixes / Linux native paths onto the local machine's equivalents (e.g. Windows `C:/Users/<user>` → Proton `drive_c/users/steamuser`). Classifies by path *format*, not the recorded OS field, so saves round-trip across any OS.

Cross-platform OS integration:
* **`system_open.rs`** — replaces `@tauri-apps/plugin-opener` for "Open folder" actions. Spawns the native opener (`xdg-open` / `open` / `explorer.exe`) with the AppImage env stripped, so the host file manager isn't broken by Spool's sandboxed `LD_LIBRARY_PATH`.
* **`diagnostics.rs`** — the Settings → Compatibility dependency doctor. Checks whether `umu-run`, `ludusavi`, and `rclone` are reachable and reports the resolution source (config override / bundled sidecar / system PATH / missing), plus per-distro install hints derived from `/etc/os-release` (Arch, Fedora, Debian, openSUSE).
* **`streaming_host.rs`** — Apollo / Sunshine (self-hosted Moonlight) streaming-host integration. Detects an installed host's config dir (Linux `~/.config/{sunshine,Apollo}`, Windows `%ProgramFiles%[(x86)]\{Sunshine,Apollo}\config`) and upserts a Spool-launching entry into its `apps.json` whose `cmd` runs `spool --run "Name" "Exe" --attached`, so a streamed Moonlight/Artemis client gets the same fullscreen-splash, exit-on-close flow as SteamOS Game Mode. Upsert is by app name and round-trips through `serde_json::Value` to preserve every other entry and unknown per-app keys (e.g. Apollo's `uuid`); atomic write with a `.bak`. On Windows, writing into Program Files falls back to an elevated (UAC) write.

Workflow orchestration:
* **`runner.rs`** — the marquee feature. Five-phase state machine: `restoring → launching → playing → backing-up → done`, emitting `run:phase` events at each transition (consumed by the in-app UI, the Game-Mode splash window, and — when the main window is hidden — native OS notifications). Single-launch RAII guard releases the slot even on panic. Cloud restore pulls from the remote first; a true local-vs-cloud divergence emits a `cloud:notice`/conflict signal that the frontend's `CloudConflictModal` resolves, while a cloud copy that's merely newer is fast-forwarded silently. Backup failures after a successful session log-and-continue.

Startup backfills (one-shot tasks at boot, results saved once at the end):
* **`accent_backfill.rs`** — picks library entries with a cover on disk but no accent colour yet, runs `extract_vibrant_color` against each.
* **`size_backfill.rs`** — picks entries with a `game_folder_path` on disk but `install_size_mb == 0`, computes the recursive directory size via `walkdir` in `spawn_blocking`.
* **`metadata_backfill.rs`** — picks entries that have a `steam_id` but empty `description`/`developer` (added before metadata fetching shipped, or whose add-time enrichment failed), fetches Steam Store metadata for each via `metadata.rs`, and folds in the missing fields. Throttles ~1.5 s between requests to respect the Steam Store rate limit.

### SvelteKit frontend (`tauri/src/`)

Routes under `tauri/src/routes/`:
* **`+layout.svelte` / `+layout.ts`** — global chrome (frameless title bar, toast stack), dark-theme/density application (drives `<html data-mode>` from `uiMode`), cross-window event subscriptions, navigation shell.
* **`+page.svelte`** — main library window. Picks **`LibraryDesktop.svelte`** or **`LibraryTouch.svelte`** based on the resolved UI mode, and renders the `CloudConflictModal` when a save conflict is signalled. Desktop is a sidebar list + detail panel (filter tabs All / Recent / Played, cover thumbnails, sync-status badges, peer-WiFi indicator, right-click context menu, separate child windows for Add/Edit/Settings). Touch is a shelf layout (Continue / All / LAN tabs, large tappable tiles, long-press context menu, two-tap to open detail, overlays instead of child windows).
* **`add/+page.svelte`** — Add Game flow (separate `WebviewWindow` on desktop, overlay in touch). Drop or browse for an exe → ludusavi auto-identifies → ranked candidate list with confidence scores → Add as &lt;name&gt; / Add without save tracking.
* **`edit/+page.svelte`** — per-game editor: identity (name, folder), install folder, launch settings (run-as-admin on Windows; Wine prefix + Proton version + winetricks helper on Linux), LAN sharing toggle, manual cover refresh, Remove.
* **`splash/+page.svelte`** — the full-screen **Game-Mode launch splash**: shows the game cover and `run:phase` progress (restoring → backing up → done/error), session playtime, and the cloud-conflict modal overlay. Signals readiness via `notify_splash_ready` so the attached `--run` workflow doesn't emit phases into the void.
* **`settings/+page.svelte`** — application settings in a two-pane (nav + body) layout, grouped **Display** (touch mode), **Library** (Ludusavi path + autodetect; Compatibility/Proton dependency doctor + umu-run path + default Proton [Linux]; Add Spool to Steam [Linux]; Decky backup plugin [Linux]; Cloud saves via rclone; SteamGridDB key), and **Sharing & Sync** (LAN sharing, device name). Live save on commit, no Save button.

Shared code under `tauri/src/lib/`:
* **`api.ts`** — the single typed wrapper around Tauri's `invoke` IPC bridge. Every backend command is a method on the exported `api` object (e.g. `api.listGames()`, `api.startPeerInstall(...)`); components never call `invoke` directly. Also exports `assetUrl()` which wraps `convertFileSrc` for loading local files (covers, art) into the webview via the `asset:` protocol. **When you add a Rust `#[tauri::command]`, add its typed wrapper here.**
* **`types.ts`** — TypeScript mirrors of the Rust serde types (`GameEntry`, `ConfigData`, `LanPeer`, `PeerGame`, `DownloadProgress`, `SyncStatus`, `SearchCandidate`, etc.). Keep these in sync with the Rust structs they mirror.
* **`uiMode.svelte.ts`** — single source of truth for UI density. `setting` (`auto` | `desktop` | `touch`, from config) resolves to `resolved` (`desktop` | `touch`); auto-detection uses `matchMedia('(pointer: coarse)')` (touchscreen-primary devices like a Deck/Ally). Writes `<html data-mode>` so CSS scales targets/spacing.
* **`components/`** — reusable Svelte components: primitives (`Btn`, `Toggle`, `TextField`, `Pill`, `Segmented`, `Icon`), layout/chrome (`AppChrome`, `WindowChrome`, `TouchTopBar`, `SettingsCard`, `SettingsRow`, `DetailCard`), the two library layouts (`LibraryDesktop`, `LibraryTouch`, `LibrarySearch`), the toast stack (`Toast`, `ToastStack`), the LAN transfer UI (`TransfersPanel`, `TransferPill`), `GameDetail`, `LibraryContextMenu`, `CloudConflictModal` (the local-vs-cloud save conflict picker), `CassetteProgress`, `CatalogId`, `SpoolMark`, and `CandidateRow`.
* **`toasts.svelte.ts`** — global toast store (Svelte 5 runes). `library.svelte.ts` — shared library state/store. `format.ts` / `tokens.ts` — display helpers (sizes, durations, design tokens). `nav.ts` — navigation helpers. `updater.ts` — wraps `tauri-plugin-updater` (checks `latest.json`, prompts, applies). `GameCard.svelte` — sidebar/grid cover thumbnail.

### Data Files

All paths below are written as `%LOCALAPPDATA%\Spool\` (Windows). The root is resolved cross-platform via `paths::app_data_dir()` (`dirs::data_local_dir()`), so on Linux the same files live under `~/.local/share/Spool/` and on macOS under `~/Library/Application Support/Spool/`.

| File | Location | Contents |
|------|----------|----------|
| `config.json` | `%LOCALAPPDATA%\Spool\` | App-wide settings (binary paths, Proton, cloud-save/rclone incl. `cloud_base_path`, SteamGridDB, UI mode, LAN share, device name) |
| `library.json` | `%LOCALAPPDATA%\Spool\` | Game library — list of `GameEntry` objects |
| `covers/` | `%LOCALAPPDATA%\Spool\` | Downloaded SteamGridDB cover images |
| `launchers/` | `%LOCALAPPDATA%\Spool\` | Generated per-game `.exe` launcher stubs (Armoury Crate, Windows) |
| `lan-games/` | `%LOCALAPPDATA%\Spool\` | Default install root for games downloaded from LAN peers (overridable via `lan_install_dir` config) |
| ludusavi config + backup | `%LOCALAPPDATA%\Spool\` | Spool-owned ludusavi `config.yaml` and the `ludusavi-backup` dir (managed by `ludusavi_config.rs`; backup/restore paths must match for cloud sync) |
| `prefixes/` | `~/.local/share/Spool/` *(Linux)* | Per-game Proton/Wine prefixes created by `proton.rs` |
| `debug.log` | `%LOCALAPPDATA%\Spool\` | App log (errors, startup events) |

### Key Patterns

* **Per-concern Tauri `State<T>`**: every command declares its dependencies as parameters (e.g. `library: State<'_, SharedLibrary>`, `config: State<'_, SharedConfig>`) — explicit, compiler-enforced, refactor-friendly. No single `AppState` god object.
* **Lock discipline**: never hold a `std::sync::Mutex` guard across `.await`. Every async command snapshots what it needs from state, drops the guard, then awaits. If state must cross an await point, that specific state moves to `tokio::sync::Mutex`.
* **Atomic JSON saves**: `library.rs` writes to a temp file then `rename`s over the target, rotating a `.bak` of the previous good file. Survives crash mid-write without corrupting either copy.
* **`AppHandle::emit` cross-window broadcast**: events go to all open webviews. The Add Game popup mutating the library triggers a refresh in the main window for free — no targeted emit needed. Current events: `library:changed`, `run:phase`, `cloud:notice`, `cloud-newer`, `sync:status-changed`, `lan:peers-changed`, `lan:download`, `lan:uploads-changed`, `lan-games`, and the tray events `tray:show` / `tray:first-hide` / `tray:quit`.
* **Event naming**: colon-namespaced (`library:changed`, `run:phase`). Tauri 2 rejects `.` in event names at runtime (allowed charset is `[A-Za-z0-9_\-/:]+`).
* **RAII run-lock**: `runner.rs` acquires a single-launch guard whose `Drop` impl releases the slot. Releases on panic too — without it a crashed workflow would leave Spool unable to launch any game until restart.
* **Backfill tasks**: older library entries may lack newer fields like `accent_color`, `install_size_mb`, or the Steam Store metadata fields. `accent_backfill.rs`, `size_backfill.rs`, and `metadata_backfill.rs` walk the library at startup, fill in the gaps (colour extraction / `walkdir` size / Steam Store fetch), save once at the end, and emit `library:changed` so the UI repaints.
* **axum LAN server**: `lan/server.rs` runs a tiny axum HTTP server in-process exposing `/games`, `/manifest`, file downloads, and cover art. Bind tries the user's preferred port first, falls back to ephemeral if taken — so multiple Spool instances on the same box still come up clean.
* **Content-addressed LAN transfer**: the host blake3-hashes each file (`/manifest`); the installer verifies hashes per file and resumes via HTTP range requests into a `.partial` dir, renaming to final only on full success — an interrupted transfer is safe to retry.
* **`tracing` for logs**: the `tracing` crate with a file appender writes to `%LOCALAPPDATA%\Spool\debug.log`. Spans wrap RunWorkflow phases for clean trace output.
* **JSON shape compatibility**: both `library.json` and `config.json` round-trip cleanly because their structs carry a **container-level** `#[serde(default)]`, so missing keys fall back to the struct's `Default` and adding fields never breaks older files. Apply `#[serde(default)]` at the struct (container) level, not per-field — a per-field default shadows the struct's custom `Default` values with the field-type default. Fields the app no longer uses are removed, not kept around for legacy round-trip.

## Cross-device control plane (rclone)

There is no separate server. Spool's cross-device features ride entirely on the rclone remote already configured for cloud saves, under `<cloud_base_path>/_spool/` (a sibling of `<cloud_base_path>/ludusavi-backup/`, never nested inside it — ludusavi's `--cloud-sync` would prune unrecognised files). Owned by `tauri/src-tauri/src/rclone.rs`:

* **Session markers** `_spool/sessions/<blake3(game_name)>.json` — written while a game is played (heartbeated every 60 s), flipped to `pending-backup` on exit, and deleted once the post-session backup confirms the saves reached the cloud. A marker existing ⇔ "that device has a session whose saves aren't in the cloud yet", which drives the advisory blocking warning on another device's launch (with a "play here anyway" override).
* **Per-device blobs** `_spool/devices/<device_id>.json` — each device writes only its own file (conflict-free): `{ playtime, last_played, backups }`. Startup folds all device files → sum playtime, max last-played, newest backer for the badge.

Reads use `rclone cat` (read-after-write consistent on more backends than a listing); writes use `rclone rcat`; reachability is an `rclone lsd` probe. Everything no-ops gracefully when cloud saves aren't configured.

## CI / CD

GitHub Actions workflows in `.github/workflows/`:
* **`ci.yml`** — runs on push to `master` and on PRs. A `build-windows` job (Windows runner) builds the backend and runs clippy/check/test plus the frontend checks; a `build-linux` job (Ubuntu, push-only) does a release-profile Linux compile to smoke-test the Linux build and warm its cache; and an `e2e-linux` job runs the WebDriver end-to-end suite under Xvfb. `sccache` + `Swatinem/rust-cache` keep it fast. The push/PR split is deliberate so the cache saves to the default-branch scope that the tag-triggered release build can later restore.
* **`release.yml`** — tag-triggered release build. Builds **both** the Windows NSIS installer (`build-windows`) and the Linux AppImage (`build-linux`, with a Wayland-library strip + repack so it runs on Bazzite/CachyOS/SteamOS), then a `release` job publishes both plus a combined `latest.json` updater manifest (see Releasing below).
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

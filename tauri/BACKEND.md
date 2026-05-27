# Spool Backend Architecture (Rust / Tauri)

A working document for porting the C# WPF backend to a Rust Tauri backend.
Treat this as the source-of-truth design doc — update it when decisions
change.

---

## Overview

Spool is a game library + save-management wrapper. The Rust backend owns:

- **Persistence** of the game library and app settings
- **Subprocess orchestration** of [ludusavi] for save restore/backup
- **External APIs**: SteamGridDB for cover art
- **OS integration**: Steam shortcuts.vdf, Armoury Crate, registry, process
  spawning, file dialogs, system tray
- **Workflow orchestration**: the multi-phase game-launch state machine
- **Lifecycle**: single long-lived tray-resident process that opens
  library / sync-overlay / setup windows on demand

The frontend (SvelteKit) is purely a view onto this state. All file IO,
subprocess management, and HTTP calls live in Rust.

### Deployment model: tray-resident

Spool runs as a single long-lived process that lives in the system tray
(like Steam or Discord). Secondary `--run` invocations from Steam shortcuts
or Armoury Crate launchers are **forwarded to the running instance** via
`tauri-plugin-single-instance` — they don't spawn new processes. This
eliminates cold-start latency on game launch, gives a single source of
truth for "is a game running", and makes the v2 sync-server / background
backup features straightforward (one long-lived process to maintain
WebSocket connections, scheduled work, etc.).

[ludusavi]: https://github.com/mtkennerly/ludusavi

---

## Module layout

```
src-tauri/src/
├── main.rs              # entry point
├── lib.rs               # module wiring, state, command registration,
│                        # lifecycle (normal / --run / first-run)
│
├── error.rs             # AppError + AppResult                      ✅ done
├── paths.rs             # filesystem locations                      ✅ done
├── logging.rs           # tracing setup, debug.log writer
│
├── config.rs            # Config struct, settings persistence
├── library.rs           # GameEntry + Library CRUD                  ✅ partial
│
├── ludusavi.rs          # CLI subprocess + --api JSON DTO parsing
├── steamgriddb.rs       # cover/hero/logo/grid search + download
├── steam.rs             # shortcuts.vdf writer + Steam path detection
├── armoury.rs           # Armoury Crate file integration  (Windows)
├── launcher.rs          # launcher_stub.exe payload generator (Windows)
├── process.rs           # game process spawn (runas / shell verb)
│
├── runner.rs            # RunWorkflow: orchestrates restore → launch → backup
├── cli.rs               # CLI arg parsing for `--run` mode
├── tray.rs              # system tray icon, menu, click dispatch
├── windows.rs           # window orchestration (show/hide library, open overlay)
└── update.rs            # Tauri updater wiring + migration from update.xml
```

Files stay flat until they outgrow ~500 lines, then split to a folder.

---

## Module responsibilities

| Module | Owns | Talks to | Tauri commands |
|---|---|---|---|
| `config` | `Config`, settings JSON, ludusavi-detection | `paths` | `get_config`, `update_config` |
| `library` | `GameEntry`, `Library`, atomic save + .bak | `paths` | `list_games`, `add_game`, `update_game`, `remove_game` |
| `ludusavi` | subprocess invocation, JSON DTOs, fuzzy name lookup | `config` (binary path) | `search_games` |
| `steamgriddb` | HTTP client, image downloads | `config` (API key), `paths` | `fetch_cover`, `fetch_all_art` |
| `steam` | shortcuts.vdf format, Steam install detection | OS registry (Windows) | `add_to_steam` |
| `armoury` | Armoury Crate registration | `paths`, `launcher` | `generate_for_armoury` |
| `launcher` | stub extraction + payload append | `paths`, `config` | called by `armoury` only |
| `process` | game launch with shell verb / runas | OS registry (Windows) | called by `runner` only |
| `runner` | run workflow state machine | `config`, `library`, `ludusavi`, `process` | `launch_game` |
| `tray` | tray icon, context menu, click dispatch, status indicator | `windows`, `runner` | none directly (driven by tray events) |
| `windows` | show/hide library window, spawn overlay windows for `--run`, route to setup on first-run | all UI-touching modules | `show_library`, `hide_to_tray` |
| `update` | auto-updater, legacy `update.xml` migration | `config` | `check_for_updates`, `apply_update` |

---

## Cross-cutting concerns

### State pattern

Use Tauri `State<T>` per concern, not a single `AppState`. Commands declare
their dependencies as parameters — explicit and compiler-enforced:

```rust
async fn launch_game(
    id: String,
    library: State<'_, SharedLibrary>,
    config:  State<'_, SharedConfig>,
    ludusavi: State<'_, LudusaviClient>,
) -> AppResult<()> { ... }
```

- `Mutex<Library>` and `Mutex<Config>` for mutable state
- `LudusaviClient`, `SteamGridDbClient` are stateless — managed without a
  Mutex
- **Lock rule**: never hold a `std::sync::Mutex` across `.await`. If we
  need to, that specific state moves to `tokio::sync::Mutex`.

### Async vs sync

| Operation | Mode | Why |
|---|---|---|
| File IO (library.json, config.json) | sync | small files, fast, no need to await |
| HTTP (SteamGridDB) | async (`reqwest`) | latency-bound, can be concurrent |
| Subprocess (ludusavi, game) | async (`tokio::process`) | long-running, must not block runtime |

### Error handling

Single `AppError` enum, variants per domain. Already `Serialize` so it
round-trips to the frontend as a string. Domain modules use `?` freely.

### Events (backend → frontend push)

Long-running ops emit progress events rather than returning blob results:

| Event | Payload | When |
|---|---|---|
| `library.changed` | `{ game_id }` | any library mutation |
| `cover.downloaded` | `{ game_id, kind, path }` | SteamGridDB download finishes |
| `run.phase` | `{ game_id, phase, message }` | RunWorkflow phase transition |
| `run.error` | `{ game_id, message }` | RunWorkflow fails |
| `update.available` | `{ version }` | new release detected |

Dotted names + JSON payload so the frontend can subscribe per-domain.

### Logging

`tracing` crate with a file appender → `%LOCALAPPDATA%\Spool\debug.log`
(same path as C# for continuity). Initialised once in `lib.rs::run`. Spans
around RunWorkflow phases for clean trace output.

### Cancellation

For ops that can be cancelled (game-launch sequence if user closes window),
use `tokio_util::sync::CancellationToken`. Pass tokens through workflow
phases. Drop = cancel.

### Single-instance enforcement

The tray-resident model makes single-instance the default state — one
Spool process, period. `tauri-plugin-single-instance` enforces it:
secondary launches (e.g. user clicks a Steam shortcut while Spool is
already running) are intercepted and their args forwarded to the primary
process. The primary dispatches based on the args:

- `--run "Name" "Exe"` → open overlay window + start RunWorkflow
- no args → show / unhide the library window
- `--quit` (optional) → graceful shutdown

This solves three problems at once: no concurrent library.json writes, no
duplicate process bloat, and no "two overlays for the same game" race.

### Concurrency model for game launches

- Normal launch (UI click): runs in-process, async task, emits `run.phase`
- `--run X` invocation: this *is* the launch — the entire app exists to
  drive that one workflow
- Two concurrent launches from one Spool: allowed? **Decision: serialise
  per-game, allow per-different-game.** Two simultaneous restores on the
  same save dir would corrupt it; two different games is fine. Enforce via
  a `HashMap<game_id, Mutex<()>>` in the runner.

---

## App lifecycle

One long-lived process. The tray icon is mounted at startup and stays
until the user explicitly quits. Windows come and go as needed.

### Startup (always the same path)

1. Parse CLI args
2. Initialise logging, tracing, panic hook
3. Migrate `%LOCALAPPDATA%\ludusavi-wrap` → `%LOCALAPPDATA%\Spool` if needed
4. Load config + library
5. Mount system tray icon + context menu
6. **Dispatch based on args:**
   - `--run "Name" "Exe"` → start RunWorkflow + open overlay window
   - no args, first-run (no config) → open Setup window
   - no args, returning user → open library window (or stay in tray
     silently if `--minimised` / autostart-with-OS)

### Runtime: triggers that open windows

| Trigger | Action |
|---|---|
| Tray icon click | toggle library window (show if hidden, focus if open) |
| Tray menu "Show Library" | show library window |
| Tray menu "Quit" | check for running workflows → graceful shutdown |
| Secondary `spool --run X Y` arrives | open overlay window + start RunWorkflow |
| Library window close button | hide window (process keeps running) — with first-time prompt explaining tray |
| Setup completion | replace setup with library window |

### Game launch (in detail)

The overlay window is short-lived per game launch:

1. RunWorkflow starts, emits `run.phase "restoring"`
2. Overlay opens, subscribes to events, shows progress
3. Workflow proceeds through restore → spawn game → wait → backup
4. On `run.phase: done` → overlay closes (or shows "completed" briefly)
5. Tray icon returns to idle state

### Tray icon states

- **idle** — default icon
- **game-running** — coloured dot overlay, tooltip "Playing {name}"
- **syncing** — animated icon during restore/backup phases
- **error** — red overlay if last workflow failed (clears on next launch)

### Shutdown (only on explicit Quit)

- Window close ≠ shutdown. Window close hides the window.
- Tray "Quit" → check for in-flight workflows:
  - If a backup is running, block quit until it completes, show
    "Backing up saves before exit…" toast
  - If a game is still running, prompt: "{Game} is still running.
    Quit anyway? Backup won't run until next launch."
- Then: clean up tray, flush logs, exit.

---

## Key workflows

### Launch a game (the big one)

```
launch_game(id)
  ├─ library::find(id) → entry
  ├─ runner::execute(entry, config):
  │   ├─ emit run.phase "restoring"
  │   ├─ ludusavi::restore(name) → handle CloudConflict, CloudSyncFailed
  │   ├─ emit run.phase "launching"
  │   ├─ process::spawn(exe, run_as_admin) → wait_for_exit
  │   ├─ library::update_last_played(id, now)
  │   ├─ emit run.phase "backing-up"
  │   ├─ ludusavi::backup(name)
  │   └─ library::update_playtime(id, session_minutes)
  └─ emit run.phase "done"
```

Errors at any phase → emit `run.error` + return early. Frontend overlay
subscribes for live updates.

### Add a game

```
add_game(name, exe_path)
  ├─ library::add(entry) → persist
  ├─ emit library.changed
  ├─ return entry.id immediately
  └─ spawn task: steamgriddb::fetch_cover(name)
       └─ on success: library::update + emit cover.downloaded
```

Frontend gets the entry instantly; cover fills in async.

### Add to Steam

```
add_to_steam(id)
  ├─ entry = library::find(id)
  ├─ steamgriddb::fetch_all_art(name) → cover/hero/grid/logo
  ├─ steam::write_shortcut(
  │     name,
  │     exe = spool_binary_path,
  │     args = ["--run", name, exe_path],
  │     art_paths,
  │   )
  └─ emit library.changed
```

**No launcher stub on any platform for Steam.** Steam's `LaunchOptions`
field carries `--run "Name" "ExePath"`.

### Generate Armoury Crate launcher (Windows only)

```
generate_for_armoury(id)
  ├─ entry = library::find(id)
  ├─ launcher::extract_stub_to(launchers_dir/{safe_name}.exe)
  ├─ launcher::append_payload(name, exe, spool_path)
  ├─ armoury::register(launcher_exe_path, art)
  └─ library::update(entry.launcher_exe_path)
```

Stub is needed here because Armoury Crate can't pass args to the game.

---

## Cross-platform strategy

| Concern | Windows | Linux | macOS |
|---|---|---|---|
| Steam shortcuts | shortcuts.vdf at `%LOCALAPPDATA%\Steam\userdata` | `~/.steam/steam/userdata` | `~/Library/Application Support/Steam/userdata` |
| Launcher stub | needed for Armoury Crate | **not needed** — flag invocation | **not needed** |
| Armoury Crate | yes | n/a | n/a |
| Run-as-admin | shell verb `runas` | n/a (use polkit if ever needed) | n/a |
| Registry | `winreg` crate | n/a | n/a |
| Process launch | `ShellExecute`-like via `tokio::process` | `tokio::process` | `tokio::process` |
| Toast notifications | `tauri-plugin-notification` (v1 reduced fidelity) | same | same |

Modules gated with `#[cfg(target_os = "windows")]`:
- `armoury` (entire module)
- `launcher` (only consumer is armoury)
- Registry-related helpers in `process` and `steam`

The single binary contains all targets; conditional compilation drops the
Windows-only code from Linux/macOS builds.

---

## Implementation phases

| Phase | Scope | Deliverable |
|---|---|---|
| 1 | Foundation | `config.rs`, `logging.rs`, library CRUD + events |
| 2 | Tray + lifecycle | `tray.rs`, `windows.rs`, `cli.rs`, single-instance plugin, close-to-tray prompt, autostart setting |
| 3 | Ludusavi | `ludusavi.rs` with restore/backup/search + JSON DTOs |
| 4 | Run workflow | `process.rs` + `runner.rs` + overlay window route |
| 5 | SteamGridDB | `steamgriddb.rs` + Add Game flow integration |
| 6 | Steam + Armoury | `steam.rs`, `armoury.rs`, `launcher.rs` (Windows) |
| 7 | Polish | updater, migration, first-run, panic hook, tray icon state variants |

Each phase ends with: backend modules + Tauri commands + at least one
integration test using a real fixture.

---

## Testing strategy

### What ports from C#

C# tests don't auto-convert (different language) but the **test scenarios
are absolutely worth porting** — they encode hard-won edge cases:

| C# test class | Port to | Priority |
|---|---|---|
| `GameEntryTests` | `library.rs` mod tests | High — JSON round-trip is critical |
| `GameLibraryTests` | `library.rs` mod tests | High — atomic save, .bak rotation |
| `LauncherGeneratorTests` | `launcher.rs` mod tests | Medium — Windows-only |
| `RunWorkflowTests` | `runner.rs` mod tests | High — phase orchestration is complex |
| `GameEntryBuilder` | Rust builder helper in `tests/common.rs` | High — shared test utility |

Treat the C# tests as a *specification*: read what each scenario asserts,
write a Rust test that asserts the same.

### Test infrastructure

- **Unit tests** inline with `#[cfg(test)] mod tests` at the bottom of
  each module
- **Fixtures** under `src-tauri/tests/fixtures/`:
  - `library.sample.json` — sanitised real library for round-trip tests
  - `ludusavi.restore.cloud-conflict.json` — captured `--api` outputs
- **Mock subprocess**: a `LudusaviRunner` trait — `RealRunner` for prod,
  `FakeRunner` for tests that returns canned `ProcessResult`s
- **Mock HTTP**: `wiremock` crate for SteamGridDB tests
- **Integration tests** in `src-tauri/tests/`: drive Tauri commands
  end-to-end against a temp data dir

### First test to write

Highest-leverage test: **load a real `library.json`, save it, load again,
assert identical**. Catches any drift between C# emit and Rust round-trip.
Put it under Phase 1, run on every commit.

---

## Migration considerations (existing C# users)

| Asset | Migration approach |
|---|---|
| `library.json` | Read directly — schema matches exactly via `serde(default)` |
| `config.json` | Same — port Config struct with identical field names |
| `covers/`, `launchers/` | Reuse in-place — same directory |
| Steam shortcuts pointing at `launchers/*.exe` | Leave alone; existing stubs keep working. New shortcuts use direct `--run` invocation. |
| `%LOCALAPPDATA%\ludusavi-wrap\` directory | Migrate on first run, mirror C# `MigrateAppData()` |
| `update.xml` (current updater) | Tauri uses signed `latest.json`. Either: ship final C# release that points users at a one-time "upgrade-to-Tauri" build, or parallel-publish both formats during transition. **TBD — needs decision.** |
| Per-game launcher.exe stubs | Keep working. Regenerate only when "Add to Steam" or "Generate for Armoury Crate" is re-run on that game. |

---

## Deferred to v2

These features exist in the C# app and have a home in the module layout
but are explicitly **not** in v1 scope:

- **Sync server** (`PlayStateLockClient` — play-state locks across
  devices) → future `sync.rs`
- **LAN sharing** (peer discovery + file serving) → future `lan.rs`
- **Hydra / TorBox download sources** → future `downloads/` module
- **Rich Windows toast notifications** (UWP-style progress toasts) —
  v1 uses simpler Tauri notifications
- **Touch-mode UI** (`IsTouchOptimized`) — v1 is desktop-only

The module skeleton accommodates all of these without restructuring.

---

## Open questions / decisions to revisit

1. **Updater migration path** — see migration table above. Needs a
   decision before v1 ships. Also: tray-resident apps need to quit
   cleanly, update, restart, and restore tray state. Test this path.
2. **Linux tray fallback** — some desktop environments (GNOME without
   extensions) don't render system tray icons. Need a fallback story:
   keep the library window open, headless background mode, or detect
   and warn? Not a v1 blocker.
3. **Mica / Acrylic backdrop on Windows** — Tauri webview supports it
   via `tauri-plugin-decorum` or platform calls. Worth replicating? Or
   stick with flat CSS theming for cross-platform consistency?
4. **Per-game launch serialisation** — `HashMap<id, Mutex>` as proposed,
   or just a global "one launch at a time" mutex? Simpler is fine if
   users rarely launch two at once.
5. **Game-exit detection on Linux** — `process::Child::wait` works for
   directly spawned processes, but games launched via wrapper scripts
   (Lutris, Heroic, Proton) detach. May need PID tracking or wait on
   the actual game binary.
6. **First-run wizard scope** — minimum: ludusavi path + close-to-tray
   explanation. Optional: SteamGridDB key, theme preference, autostart
   checkbox. UI is frontend territory; backend just exposes
   `is_first_run`, `complete_setup` commands.
7. **Autostart default** — opt-in (default off, surfaced in settings)
   vs opt-out (default on, with prompt on first run). Lean opt-in for
   respect-the-user reasons; users who want it will enable it.

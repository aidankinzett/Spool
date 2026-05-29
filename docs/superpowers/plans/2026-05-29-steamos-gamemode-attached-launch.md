# SteamOS Game-Mode Attached Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When Steam launches Spool's shortcut in SteamOS Game Mode, run the game workflow behind a minimal splash and then EXIT when the game exits (so Steam stops tracking it), plus expose a headless `spool --backup "Name"` one-shot and an on-disk session record for the future Decky plugin.

**Architecture:** Detect Game Mode via gamescope's `GAMESCOPE_WAYLAND_DISPLAY` env var. Branch `lib.rs::run()` into three startup shapes — fully-headless `--backup`, attached `--run` (no single-instance / tray / pollers; splash window; `app.exit(0)` when the workflow finishes), and today's unchanged tray-resident default. A session-record JSON file plus a `backed_up` flag lets a future plugin avoid double-backups.

**Tech Stack:** Rust + Tauri 2, `steam_shortcuts_util` (appid), `chrono`, `serde_json`, SvelteKit 5 (splash route).

---

## File Structure

| File | Responsibility |
|------|----------------|
| `tauri/src-tauri/src/gamemode.rs` (new) | Pure Game-Mode detection from env |
| `tauri/src-tauri/src/session.rs` (new) | Active-session record: write/read/mark + appid compute |
| `tauri/src-tauri/src/cli.rs` (modify) | Add `CliMode::Backup` + parsing |
| `tauri/src-tauri/src/paths.rs` (modify) | `active_session_file()` |
| `tauri/src-tauri/src/runner.rs` (modify) | Extract `backup_game_core`; mark session after self-backup |
| `tauri/src-tauri/src/lib.rs` (modify) | Module wiring; three-way startup branch; attached setup + headless backup |
| `tauri/src-tauri/tauri.conf.json` (modify) | `main` window `visible: false` (shown explicitly) |
| `tauri/src/routes/splash/+page.svelte` (new) | Minimal phase splash listening to `run:phase` |

Conventions to match: modules are `snake_case.rs` wired in `lib.rs`; every command/struct mirrors C# JSON shape with `#[serde(default)]` where it's persisted; errors use `AppError`/`AppResult`; never hold a `std::sync::Mutex` guard across `.await`.

---

## Task 1: Game-Mode detection (`gamemode.rs`)

**Files:**
- Create: `tauri/src-tauri/src/gamemode.rs`
- Modify: `tauri/src-tauri/src/lib.rs` (add `mod gamemode;` near the other `mod` lines ~28-50)

- [ ] **Step 1: Write the failing test**

Create `tauri/src-tauri/src/gamemode.rs` with only the test + a pure decision fn signature:

```rust
//! Detects whether Spool is running inside a SteamOS / Steam Deck "Game Mode"
//! session (the gamescope-composited Big Picture session) vs a normal desktop
//! session. The `--run` startup path uses this to switch into attached-launch
//! mode: in Game Mode, Spool runs the game workflow then EXITS so Steam sees
//! the game stop, instead of staying tray-resident.

/// Pure decision core, separated from env reads so it is unit-testable.
/// `override_val` is `$SPOOL_ATTACHED_LAUNCH`, `gamescope` is
/// `$GAMESCOPE_WAYLAND_DISPLAY`, `is_linux` gates the gamescope signal.
fn decide(override_val: Option<&str>, gamescope: Option<&str>, is_linux: bool) -> bool {
    if let Some(v) = override_val {
        let v = v.trim();
        if v == "1" || v.eq_ignore_ascii_case("true") {
            return true;
        }
        if v == "0" || v.eq_ignore_ascii_case("false") {
            return false;
        }
    }
    is_linux && gamescope.map(|s| !s.is_empty()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::decide;

    #[test]
    fn gamescope_present_on_linux_is_game_mode() {
        assert!(decide(None, Some("gamescope-0"), true));
    }

    #[test]
    fn gamescope_present_off_linux_is_not_game_mode() {
        assert!(!decide(None, Some("gamescope-0"), false));
    }

    #[test]
    fn no_gamescope_is_not_game_mode() {
        assert!(!decide(None, None, true));
        assert!(!decide(None, Some(""), true));
    }

    #[test]
    fn override_forces_on_and_off() {
        assert!(decide(Some("1"), None, false));
        assert!(decide(Some("true"), None, false));
        assert!(!decide(Some("0"), Some("gamescope-0"), true));
        assert!(!decide(Some("false"), Some("gamescope-0"), true));
    }
}
```

- [ ] **Step 2: Run the test to verify it passes (logic is already complete)**

Run: `cd tauri/src-tauri && cargo test --lib gamemode`
Expected: 4 tests pass. (You must also add `mod gamemode;` to `lib.rs` first — do Step 3 before running.)

- [ ] **Step 3: Add the module declaration**

In `tauri/src-tauri/src/lib.rs`, add alphabetically near the other `mod` declarations (after `mod error;` / with the rest, ~line 33):

```rust
mod gamemode;
```

- [ ] **Step 4: Add the public env-reading wrapper**

Append to `gamemode.rs` (above the `#[cfg(test)]` block):

```rust
/// True when Spool should use attached-launch behavior. See `decide`.
pub fn is_steam_game_mode() -> bool {
    let override_val = std::env::var("SPOOL_ATTACHED_LAUNCH").ok();
    let gamescope = std::env::var("GAMESCOPE_WAYLAND_DISPLAY").ok();
    decide(
        override_val.as_deref(),
        gamescope.as_deref(),
        cfg!(target_os = "linux"),
    )
}
```

- [ ] **Step 5: Verify build + tests**

Run: `cd tauri/src-tauri && cargo test --lib gamemode && cargo clippy --all-targets -- -D warnings`
Expected: tests pass; clippy clean. (`is_steam_game_mode` will warn as unused until Task 6 — temporarily acceptable, OR add `#[allow(dead_code)]` above it and remove the attribute in Task 6.)

- [ ] **Step 6: Commit**

```bash
git add tauri/src-tauri/src/gamemode.rs tauri/src-tauri/src/lib.rs
git commit -m "feat(linux): Game Mode detection via GAMESCOPE_WAYLAND_DISPLAY"
```

---

## Task 2: `--backup` CLI mode (`cli.rs`)

**Files:**
- Modify: `tauri/src-tauri/src/cli.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `tauri/src-tauri/src/cli.rs`:

```rust
    #[test]
    fn backup_with_name_parses() {
        let argv = ["spool.exe", "--backup", "Hades"];
        assert_eq!(
            parse_args(&argv),
            CliMode::Backup {
                game_name: "Hades".to_string(),
            }
        );
    }

    #[test]
    fn backup_missing_name_falls_back_to_normal() {
        assert_eq!(parse_args::<&str>(&["spool.exe", "--backup"]), CliMode::Normal);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd tauri/src-tauri && cargo test --lib cli`
Expected: FAIL — `CliMode::Backup` does not exist (compile error).

- [ ] **Step 3: Add the variant and parsing**

In `cli.rs`, add to the `CliMode` enum (after `Run { .. }`):

```rust
    /// Headless one-shot: back up a single game's saves, then exit. Used by
    /// the Decky plugin's forced-close fallback. No GUI, no tray.
    Backup { game_name: String },
```

In `parse_args`, after the existing `--run` block and before `CliMode::Normal`:

```rust
    if rest.len() >= 2 && rest[0] == "--backup" {
        return CliMode::Backup {
            game_name: rest[1].to_string(),
        };
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd tauri/src-tauri && cargo test --lib cli`
Expected: PASS (all `cli` tests, including the originals).

- [ ] **Step 5: Commit**

```bash
git add tauri/src-tauri/src/cli.rs
git commit -m "feat(cli): add --backup headless mode parsing"
```

---

## Task 3: Session record (`session.rs` + `paths.rs`)

**Files:**
- Create: `tauri/src-tauri/src/session.rs`
- Modify: `tauri/src-tauri/src/paths.rs`
- Modify: `tauri/src-tauri/src/lib.rs` (add `mod session;`)

- [ ] **Step 1: Add the path helper**

In `tauri/src-tauri/src/paths.rs`, after `config_file()` (~line 25):

```rust
/// Record of the in-progress launch session, written by attached `--run` mode
/// so the Decky plugin can decide whether a forced-close fallback backup is
/// needed. Removed/marked done once a backup succeeds.
pub fn active_session_file() -> PathBuf {
    app_data_dir().join("active-session.json")
}
```

- [ ] **Step 2: Write the failing test (path-injected core)**

Create `tauri/src-tauri/src/session.rs`:

```rust
//! Active-session record for SteamOS Game-Mode launches.
//!
//! Attached `--run` mode writes this at launch and flips `backed_up = true`
//! once a backup completes (Spool's own post-session backup, or a headless
//! `spool --backup`). A future Decky plugin reads it on the game-stop event:
//! if `backed_up` is still false, Steam force-killed Spool before it backed
//! up, so the plugin spawns `spool --backup` as a fallback.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSession {
    pub game: String,
    pub steam_appid: u32,
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub backed_up: bool,
}

/// Steam's CRC-based appid for a non-Steam shortcut. MUST match
/// `steam::upsert_spool_shortcut`'s computation so the value equals the appid
/// Steam reports to the plugin: `calculate_app_id("\"<exe>\"", game_name)`.
pub fn compute_steam_appid(spool_exe: &str, game_name: &str) -> u32 {
    let quoted_exe = format!("\"{}\"", spool_exe.replace('"', "\\\""));
    steam_shortcuts_util::app_id_generator::calculate_app_id(&quoted_exe, game_name)
}

fn write_start_at(path: &Path, game: &str, steam_appid: u32, started_at: DateTime<Utc>) -> AppResult<String> {
    let session_id = format!("{steam_appid}-{}", started_at.timestamp_millis());
    let rec = ActiveSession {
        game: game.to_string(),
        steam_appid,
        session_id: session_id.clone(),
        started_at,
        backed_up: false,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&rec)?)?;
    Ok(session_id)
}

fn read_at(path: &Path) -> Option<ActiveSession> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn mark_backed_up_at(path: &Path) {
    if let Some(mut rec) = read_at(path) {
        rec.backed_up = true;
        if let Ok(bytes) = serde_json::to_vec_pretty(&rec) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_mark() {
        let dir = std::env::temp_dir().join(format!("spool-session-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("active-session.json");

        let now = chrono::DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let id = write_start_at(&path, "Hades", 0x8000_0001, now).unwrap();
        assert!(id.starts_with("2147483649-"));

        let rec = read_at(&path).expect("record written");
        assert_eq!(rec.game, "Hades");
        assert!(!rec.backed_up);

        mark_backed_up_at(&path);
        assert!(read_at(&path).unwrap().backed_up);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mark_when_absent_is_noop() {
        let path = std::env::temp_dir().join("spool-session-absent-xyz.json");
        std::fs::remove_file(&path).ok();
        mark_backed_up_at(&path); // must not panic
        assert!(read_at(&path).is_none());
    }

    #[test]
    fn appid_matches_steam_shortcut_formula() {
        // Same inputs as steam::upsert_spool_shortcut → identical appid.
        let quoted = format!("\"{}\"", "/home/u/spool-launcher.sh");
        let expected =
            steam_shortcuts_util::app_id_generator::calculate_app_id(&quoted, "Hades");
        assert_eq!(compute_steam_appid("/home/u/spool-launcher.sh", "Hades"), expected);
    }
}
```

- [ ] **Step 3: Add public wrappers that use the real path**

Append to `session.rs` (above `#[cfg(test)]`):

```rust
/// Write the session record for a launch starting now.
pub fn write_start(game: &str, steam_appid: u32) -> AppResult<String> {
    write_start_at(&crate::paths::active_session_file(), game, steam_appid, Utc::now())
}

/// Read the current session record, if any.
#[allow(dead_code)]
pub fn read() -> Option<ActiveSession> {
    read_at(&crate::paths::active_session_file())
}

/// Mark the current session's backup as done. No-op when no record exists.
pub fn mark_backed_up() {
    mark_backed_up_at(&crate::paths::active_session_file());
}
```

- [ ] **Step 4: Wire the module**

In `lib.rs`, add near the other `mod` lines:

```rust
mod session;
```

- [ ] **Step 5: Run tests + clippy**

Run: `cd tauri/src-tauri && cargo test --lib session && cargo clippy --all-targets -- -D warnings`
Expected: 3 session tests pass; clippy clean. (`read`/`write_start`/`mark_backed_up`/`compute_steam_appid` may warn unused until Tasks 6-7 wire them — the `#[allow(dead_code)]` on `read` covers the longest-lived one; add `#[allow(dead_code)]` to the others if clippy is run with `-D warnings` before those tasks, and remove in Tasks 6-7.)

- [ ] **Step 6: Commit**

```bash
git add tauri/src-tauri/src/session.rs tauri/src-tauri/src/paths.rs tauri/src-tauri/src/lib.rs
git commit -m "feat: active-session record for forced-close backup dedup"
```

---

## Task 4: Extract `backup_game_core` (`runner.rs`)

**Files:**
- Modify: `tauri/src-tauri/src/runner.rs`

Goal: a backup function that takes a `&SharedLibrary` (a `Mutex<Library>`) and an already-resolved ludusavi path — usable from both the `manual_backup` command and the headless `--backup` path, without an `AppHandle`.

- [ ] **Step 1: Add the core function**

In `runner.rs`, add (near `manual_backup`):

```rust
/// AppHandle-free backup core. Resolves the game's name + wine prefix from the
/// library, runs `ludusavi backup`, and persists the entry's backup stats.
/// Returns the bundle count + total bytes. Callers handle event emission and
/// sync-server recording (best-effort) themselves.
pub async fn backup_game_core(
    ludusavi_client: &LudusaviClient,
    ludusavi_exe: &Path,
    config_dir: &Path,
    library: &SharedLibrary,
    game_id: &str,
) -> AppResult<ManualBackupResult> {
    let (game_name, use_proton, prefix_override) = {
        let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        let entry = lib
            .find(game_id)
            .ok_or_else(|| AppError::Other(format!("game not found: {game_id}")))?;
        (
            entry.game_name.clone(),
            entry.use_proton,
            entry.wine_prefix_path.clone(),
        )
    };
    let wine_prefix: Option<PathBuf> = if cfg!(not(windows)) && use_proton {
        Some(
            prefix_override
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| crate::proton::game_prefix_path(game_id)),
        )
    } else {
        None
    };

    let out = ludusavi_client
        .backup(ludusavi_exe, config_dir, &game_name, wine_prefix.as_deref())
        .await
        .map_err(|e| AppError::Other(format!("ludusavi backup: {e}")))?;

    let (game_count, bytes_total) = out
        .overall
        .as_ref()
        .map(|o| (o.total_games as i32, o.total_bytes))
        .unwrap_or((0, 0));

    if game_count > 0 {
        let mut lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
        if let Some(entry) = lib.entries.iter_mut().find(|e| e.id == game_id) {
            entry.save_backup_count += 1;
            entry.save_last_backed_up_at = Some(Utc::now());
            entry.save_backup_size_mb = (bytes_total as f64) / (1024.0 * 1024.0);
            entry.sync_badge = Some("synced".to_string());
        }
        lib.save()?;
    }

    Ok(ManualBackupResult {
        game_count,
        bytes_total: bytes_total as u64,
    })
}
```

- [ ] **Step 2: Rewrite `manual_backup` to delegate**

Replace the body of `#[tauri::command] pub async fn manual_backup` so it resolves inputs, calls the core, then does the emit + sync recording:

```rust
#[tauri::command]
pub async fn manual_backup(app: AppHandle, game_id: String) -> AppResult<ManualBackupResult> {
    let ludusavi_exe = {
        let config = app.state::<SharedConfig>();
        let cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        crate::paths::resolve_ludusavi_path(&cfg.data.ludusavi_path).ok_or_else(|| {
            AppError::Other(
                "Ludusavi is not configured. Place ludusavi in your PATH or configure it in Settings.".into(),
            )
        })?
    };
    let config_dir = crate::paths::ludusavi_config_dir();
    let ludusavi_client = app.state::<LudusaviClient>();
    let library = app.state::<SharedLibrary>();

    let result =
        backup_game_core(&ludusavi_client, &ludusavi_exe, &config_dir, &library, &game_id).await?;

    if result.game_count > 0 {
        let _ = app.emit("library:changed", &game_id);
        // game_name for the sync event:
        let game_name = {
            let lib = library.lock().map_err(|_| AppError::LockPoisoned)?;
            lib.find(&game_id).map(|e| e.game_name.clone())
        };
        if let Some(name) = game_name {
            sync::record_backup_event(&app, &name).await;
        }
    }
    Ok(result)
}
```

Note: `manual_prep` is still used by `manual_restore` — leave it. If clippy flags any now-unused import, fix it.

- [ ] **Step 3: Verify build + existing tests**

Run: `cd tauri/src-tauri && cargo test --lib && cargo clippy --all-targets -- -D warnings`
Expected: builds; existing runner tests still pass; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add tauri/src-tauri/src/runner.rs
git commit -m "refactor(runner): extract AppHandle-free backup_game_core"
```

---

## Task 5: Mark session backed-up after the self-backup (`runner.rs`)

**Files:**
- Modify: `tauri/src-tauri/src/runner.rs` (`run_workflow`, end of fn)

- [ ] **Step 1: Add the mark call**

In `run_workflow`, immediately before the final `emit_phase(app, game_id, "done", None);` (~line 765), add:

```rust
    // Game Mode: flag the active-session record so the Decky plugin's
    // forced-close fallback knows this session already backed up. No-op
    // when there's no record (desktop / Windows launches).
    crate::session::mark_backed_up();
```

- [ ] **Step 2: Verify build**

Run: `cd tauri/src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: clean (this also clears the `#[allow(dead_code)]` need on `session::mark_backed_up`).

- [ ] **Step 3: Commit**

```bash
git add tauri/src-tauri/src/runner.rs
git commit -m "feat(runner): mark active-session backed-up after self-backup"
```

---

## Task 6: Headless `--backup` startup branch (`lib.rs`)

**Files:**
- Modify: `tauri/src-tauri/src/lib.rs`

- [ ] **Step 1: Add the headless backup runner fn**

In `lib.rs`, add a standalone function (outside `run()`):

```rust
/// Headless one-shot backup: load config + library, run ludusavi backup for
/// the named game, mark the session record, then return. No GUI / tray /
/// single-instance. Used by `spool --backup "Name"` (the Decky plugin's
/// forced-close fallback). Returns process exit code.
fn run_backup_headless(game_name: &str) -> i32 {
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "--backup: failed to load config");
            return 1;
        }
    };
    let library = match Library::load() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "--backup: failed to load library");
            return 1;
        }
    };
    let Some(game_id) = library
        .entries
        .iter()
        .find(|e| e.game_name == game_name)
        .map(|e| e.id.clone())
    else {
        tracing::error!(name = %game_name, "--backup: no library entry matches");
        return 1;
    };
    let Some(ludusavi_exe) = paths::resolve_ludusavi_path(&config.data.ludusavi_path) else {
        tracing::error!("--backup: ludusavi not configured");
        return 1;
    };

    // Make sure Spool's ludusavi config (backup path, cloud remote) exists.
    if let Err(e) = ludusavi_config::ensure_config() {
        tracing::warn!(error = %e, "--backup: ensure_config failed");
    }

    let config_dir = paths::ludusavi_config_dir();
    let lib_state: SharedLibrary = Mutex::new(library);
    let client = LudusaviClient::new();

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "--backup: failed to start tokio runtime");
            return 1;
        }
    };
    let result = rt.block_on(async {
        runner::backup_game_core(&client, &ludusavi_exe, &config_dir, &lib_state, &game_id).await
    });

    match result {
        Ok(r) => {
            tracing::info!(game_name, games = r.game_count, "--backup complete");
            session::mark_backed_up();
            0
        }
        Err(e) => {
            tracing::error!(error = %e, "--backup failed");
            1
        }
    }
}
```

Add the needed imports at the top of `lib.rs` if not already present: `ludusavi_config` is a `mod`, reference as `crate::ludusavi_config` or add `use`. `SharedLibrary` and `Library` are already imported; `Config` is imported; `LudusaviClient` is imported; `Mutex` is imported.

- [ ] **Step 2: Branch `run()` early for `--backup`**

In `run()`, right after `let _log_guard = init_tracing();` and `tracing::info!("spool starting up");`, before `paths::migrate_from_ludusavi_wrap();`, insert:

```rust
    // Headless one-shot backup (Decky plugin forced-close fallback). No GUI.
    let initial_args: Vec<String> = std::env::args().collect();
    if let CliMode::Backup { ref game_name } = cli::parse_args(&initial_args) {
        std::process::exit(run_backup_headless(game_name));
    }
```

Then delete the later duplicate `let initial_args: Vec<String> = std::env::args().collect();` (~line 144) since it's now declared above — reuse the one you just added. (Search for the existing declaration and remove it; the `setup` closure already moves `initial_args`.)

- [ ] **Step 3: Build + smoke test**

Run: `cd tauri/src-tauri && cargo build && cargo clippy --all-targets -- -D warnings`
Expected: builds clean.

Manual smoke (any machine with a library entry + ludusavi configured):
Run: `cargo run -- --backup "<an existing game name>"`
Expected: process runs ludusavi backup and **exits** (does not open a window); `active-session.json` (if present) gets `backed_up: true`; non-zero exit + log line on a bad name.

- [ ] **Step 4: Commit**

```bash
git add tauri/src-tauri/src/lib.rs
git commit -m "feat(cli): headless spool --backup one-shot for plugin fallback"
```

---

## Task 7: Splash route + `main` window config (frontend + conf)

**Files:**
- Create: `tauri/src/routes/splash/+page.svelte`
- Modify: `tauri/src-tauri/tauri.conf.json`

- [ ] **Step 1: Set `main` window to not auto-show**

In `tauri.conf.json`, the single window object under `app.windows` — add `"label": "main"` and `"visible": false`:

```json
      {
        "label": "main",
        "title": "Spool",
        "width": 1280,
        "height": 800,
        "minWidth": 900,
        "minHeight": 600,
        "decorations": false,
        "visible": false,
        "backgroundColor": "#0b0c0e"
      }
```

(We now show `main` explicitly in `setup` — see Task 8 — which also removes the white-flash on startup.)

- [ ] **Step 2: Create the splash route**

Create `tauri/src/routes/splash/+page.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import type { RunPhaseEvent } from '$lib/types';

  let phase = $state<string>('restoring');
  let message = $state<string>('Preparing…');

  const LABELS: Record<string, string> = {
    restoring: 'Restoring saves…',
    launching: 'Launching game…',
    playing: 'Starting…',
    'backing-up': 'Backing up saves…',
    done: 'Done',
    error: 'Launch failed',
  };

  onMount(() => {
    const unlisten = listen<RunPhaseEvent>('run:phase', (event) => {
      phase = event.payload.phase;
      message = event.payload.message ?? LABELS[phase] ?? phase;
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<div class="splash">
  <div class="logo">SPOOL</div>
  <div class="spinner" class:error={phase === 'error'}></div>
  <div class="label">{LABELS[phase] ?? message}</div>
</div>

<style>
  :global(body) {
    margin: 0;
    background: #0b0c0e;
    color: #e8eaed;
    overflow: hidden;
  }
  .splash {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1.25rem;
    height: 100vh;
    font-family: system-ui, sans-serif;
  }
  .logo {
    font-weight: 700;
    letter-spacing: 0.35em;
    font-size: 1.1rem;
    opacity: 0.8;
  }
  .spinner {
    width: 36px;
    height: 36px;
    border: 3px solid #2a2d31;
    border-top-color: #7aa2f7;
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  .spinner.error {
    border-top-color: #f7768e;
    animation: none;
  }
  .label {
    font-size: 0.95rem;
    opacity: 0.85;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
```

- [ ] **Step 3: Verify the frontend builds + type-checks**

Run: `cd tauri && bun run check`
Expected: no svelte-check errors (the `splash` route compiles; `RunPhaseEvent` import resolves from `$lib/types`).

- [ ] **Step 4: Commit**

```bash
git add tauri/src/routes/splash/+page.svelte tauri/src-tauri/tauri.conf.json
git commit -m "feat(ui): minimal Game-Mode launch splash route"
```

---

## Task 8: Attached `--run` startup branch (`lib.rs`)

**Files:**
- Modify: `tauri/src-tauri/src/lib.rs`

This wires it all together: in attached mode, skip single-instance / tray / pollers, show the splash (not `main`), launch the game from Rust, and `app.exit(0)` when the workflow finishes.

- [ ] **Step 1: Compute the attached flag and branch plugin registration**

In `run()`, after the `--backup` early-return and after `initial_args` is declared, compute:

```rust
    let cli_mode = cli::parse_args(&initial_args);
    let attached = matches!(cli_mode, CliMode::Run { .. }) && gamemode::is_steam_game_mode();
    if attached {
        tracing::info!("attached launch mode (SteamOS Game Mode) — no tray, exit on game close");
    }
```

Change the builder so single-instance is only added when **not** attached. Replace the chained `tauri::Builder::default().plugin(...).plugin(...)...` start with a mutable binding:

```rust
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build());
    if !attached {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_forwarded_launch(app, &argv);
        }));
    }
    let app = builder
        .manage::<SharedLibrary>(Mutex::new(library))
        // ... (rest of the existing .manage / .invoke_handler chain unchanged) ...
        .setup(move |app| {
            // ... see Step 2 ...
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
```

- [ ] **Step 2: Branch the `setup` closure**

Restructure the `setup` closure body so attached mode takes a separate path. The closure already moves `initial_args`; also move `attached` in (it's `Copy`). Replace the closure body with:

```rust
        .setup(move |app| {
            if attached {
                // ── Attached Game-Mode launch ────────────────────────────
                // No tray, no pollers, no library window. Show a splash,
                // launch the game from Rust, exit when the workflow ends.
                let CliMode::Run { game_name, .. } = cli::parse_args(&initial_args) else {
                    app.exit(1);
                    return Ok(());
                };
                let Some(id) = find_game_id_by_name(&app.state::<SharedLibrary>(), &game_name)
                else {
                    tracing::error!(name = %game_name, "attached --run: no library entry matches");
                    app.exit(1);
                    return Ok(());
                };

                // Write the session record (appid matches the Steam shortcut).
                if let Some(exe) = paths::spool_executable() {
                    let appid =
                        session::compute_steam_appid(&exe.to_string_lossy(), &game_name);
                    if let Err(e) = session::write_start(&game_name, appid) {
                        tracing::warn!(error = %e, "failed to write active-session record");
                    }
                }

                // Make sure ludusavi config exists before the workflow runs.
                if let Err(e) = ludusavi_config::ensure_config() {
                    tracing::warn!(error = %e, "failed to initialise ludusavi config dir");
                }

                // Splash window (the `main` window stays hidden / unused).
                if let Err(e) = tauri::WebviewWindowBuilder::new(
                    app,
                    "splash",
                    tauri::WebviewUrl::App("splash".into()),
                )
                .title("Spool")
                .decorations(false)
                .inner_size(520.0, 260.0)
                .center()
                .resizable(false)
                .build()
                {
                    tracing::warn!(error = %e, "failed to create splash window");
                }

                // Launch + exit when done. app.exit(0) lets Steam see the
                // game stop (RunEvent::ExitRequested only blocks code.is_none()).
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = runner::launch_game_inner(&app_handle, &id).await {
                        tracing::error!(error = %e, "attached --run workflow failed");
                    }
                    app_handle.exit(0);
                });

                return Ok(());
            }

            // ── Normal tray-resident startup (unchanged) ─────────────────
            mount_tray(app.handle())?;

            if let Some(main) = app.get_webview_window("main") {
                let win = main.clone();
                let app_handle = app.handle().clone();
                // `main` is now created hidden — show it explicitly (also
                // removes the startup white-flash).
                let _ = main.show();
                main.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                        emit_tray_intro_once(&app_handle);
                    }
                });
            }

            if let Err(e) = ludusavi_config::ensure_config() {
                tracing::warn!(error = %e, "failed to initialise ludusavi config dir");
            }

            lan::spawn_discovery(app.handle().clone());
            accent_backfill::spawn_backfill(app.handle().clone());
            size_backfill::spawn_backfill(app.handle().clone());
            sync::spawn_health_poller(app.handle().clone());
            sync::spawn_startup_sync(app.handle().clone());

            if let CliMode::Run { ref game_name, .. } = cli::parse_args(&initial_args) {
                let library = app.state::<SharedLibrary>();
                let pending = app.state::<PendingRun>();
                if let Some(id) = find_game_id_by_name(&library, game_name) {
                    pending.set(id);
                } else {
                    tracing::warn!(name = %game_name, "startup --run: no library entry matches");
                }
            }

            Ok(())
        })
```

Add the import for the webview builder types at the top `use tauri::{...}` block: add `WebviewUrl, WebviewWindowBuilder` (or reference fully-qualified as written above — both fine).

- [ ] **Step 3: Confirm exit semantics need no change**

The existing `app.run(|_, event| { if RunEvent::ExitRequested { code, api } && code.is_none() { api.prevent_exit() } })` is correct as-is: our `app_handle.exit(0)` passes `code = Some(0)`, so it is allowed through; a user closing the splash (last window) yields `code.is_none()` and is blocked, so the in-flight launch isn't killed by an accidental splash close. **No change needed** to the `app.run` closure.

- [ ] **Step 4: Build + clippy**

Run: `cd tauri/src-tauri && cargo build && cargo clippy --all-targets -- -D warnings && cargo test --lib`
Expected: builds clean; all unit tests pass; no dead-code warnings remain (every new fn is now wired).

- [ ] **Step 5: Commit**

```bash
git add tauri/src-tauri/src/lib.rs
git commit -m "feat(linux): attached Game-Mode launch — splash, no tray, exit on game close"
```

---

## Task 9: Verification

**Files:** none (verification only)

- [ ] **Step 1: Full backend check suite**

Run: `cd tauri/src-tauri && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all pass, clippy clean.

- [ ] **Step 2: Frontend checks**

Run: `cd tauri && bun run check && bun run lint`
Expected: no errors.

- [ ] **Step 3: Desktop regression smoke (any platform)**

Run: `cd tauri && bun run tauri dev`
Expected: the library window appears normally (now shown explicitly), tray works, closing the window hides to tray — i.e. **no behavior change** when `GAMESCOPE_WAYLAND_DISPLAY`/override is absent.

- [ ] **Step 4: Attached-mode simulation (desktop, forced via override)**

Build, then run the binary with the override + a real game name:
Run: `SPOOL_ATTACHED_LAUNCH=1 ./target/debug/spool --run "<game name>" "<exe path>"` (PowerShell: `$env:SPOOL_ATTACHED_LAUNCH=1; ...`)
Expected: a small splash window appears (no library, no tray icon), the workflow runs, and when the game exits **the process exits** (no lingering tray process). `active-session.json` exists during the run and is marked `backed_up: true` after a normal exit.

- [ ] **Step 5: Manual Deck/Bazzite Game-Mode check (real hardware, documented for the user)**

On a Steam Deck or Bazzite Game Mode session: add the game to Steam via Spool, launch it from the Steam library, confirm:
- splash shows during restore/launch (not the full library),
- game runs,
- quitting in-game returns to Steam with the game shown as **stopped** (Spool process gone),
- saves backed up (check `save_last_backed_up_at`).

- [ ] **Step 6: Update CLAUDE.md notes (optional, if behavior docs are kept current)**

If updating docs: note the attached Game-Mode launch path and `spool --backup` in the runner/cli sections of `CLAUDE.md`. Commit separately.

---

## Self-Review Notes

- **Spec coverage:** detection (T1), `--backup` parse (T2), session record + appid (T3), backup-core refactor (T4), session marking (T5), headless backup (T6), splash (T7), attached branch incl. no-single-instance/no-tray/no-pollers/exit-on-close (T8), verification incl. desktop-regression (T9). All spec sections mapped.
- **Out of scope:** Decky plugin (Sub-project B) — not in this plan, by design.
- **Type consistency:** `backup_game_core` signature is identical in T4 (definition), T6 (headless call), and the `manual_backup` rewrite. `ActiveSession` fields and `compute_steam_appid`/`write_start`/`mark_backed_up`/`read` names are consistent across T3/T5/T6/T8. `CliMode::Backup { game_name }` consistent in T2/T6/T8.
- **Known risk carried from spec:** splash rendering under gamescope (verify in T9 step 5); headless `--backup` sync events degrade silently (acceptable).

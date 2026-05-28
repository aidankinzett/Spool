# Spool · Tauri Rewrite — Status & Lessons

A retrospective + handoff doc for the in-progress port of Spool from
C# / WPF to Rust / Tauri 2 + SvelteKit 5. Pairs with [`BACKEND.md`](./BACKEND.md)
(the design doc) and the design package the UI was implemented from.

Last updated: 2026-05-27, after the run-workflow + toasts commit
(`37b9c93`).

---

## Where we are right now

The Tauri app is **feature-complete for the core MVP loop** described in
BACKEND.md. From scratch in one branch, in the order it landed:

| # | Commit | Slice |
|---|---|---|
| 1 | `bfaf55f` | Backend modules + initial library rendering |
| 2 | `7a243d6` | Cassette design system foundation (tokens, primitives, frameless chrome) |
| 3 | `96b0be1` | Bun workspace fix |
| 4 | `a01fc86` | Settings page + config backend + Lucide icon library |
| 5 | `adaa9ff` | Add Game flow + ludusavi search + cover art |
| 6 | `aa52e45` | Library detail view + sidebar |
| 7 | `37b9c93` | Run workflow + toast notifications |

**You can now:** add a game via the popup, see it in the library with
real SteamGridDB cover art, click into its detail page, hit Play → saves
restore → game launches → on exit, saves back up → toast confirms. Cloud
conflicts during restore surface as a warn toast with "Open Ludusavi" CTA.

---

## What works end-to-end

### Library
- ✅ Two-pane layout (sidebar list + scrollable detail)
- ✅ Search filter (live as you type)
- ✅ Filter tabs (All / Recent / Played) with counts
- ✅ Auto-select first game on load; fall back gracefully when selected game is removed
- ✅ `library:changed` event propagates across windows (popup ↔ library)

### Add Game
- ✅ Popup window (real `WebviewWindow`, not modal overlay)
- ✅ File dialog opens automatically on mount
- ✅ Auto-identifies via `ludusavi find` based on exe filename
- ✅ Search bar widens to manual lookup with 250 ms debounce
- ✅ Candidates show: name + match score (hidden when ≥95%) + Steam ID badge + GOG badge + cloud-sync indicator + first save path
- ✅ "Add without save tracking" escape hatch
- ✅ Backend lazy-loads the ~9 MB ludusavi manifest on first search; subsequent searches are instant
- ✅ Cover art auto-fetched in background after add (SteamGridDB)

### Game detail
- ✅ Hero with title, catalog id, first-genre eyebrow, Play button
- ✅ Stats strip (last played, playtime, install size, saves)
- ✅ Action toolbar: Open folder + Remove wired; others stubbed
- ✅ About / Saves / Details cards with empty states
- ✅ Cover art rendered via `convertFileSrc` + asset protocol

### Play
- ✅ Full restore → launch → wait → backup cycle
- ✅ Per-phase Play-button label ("Restoring saves…" → "Launching…" → "Playing" → "Backing up…")
- ✅ Updates `last_played_at`, `playtime_minutes`, `save_backup_count`, `save_last_backed_up_at`, `save_backup_size_mb`
- ✅ Single-launch lock with RAII guard (releases on drop, including panic)
- ✅ Cloud-conflict detection → warn toast with "Open Ludusavi" CTA
- ✅ Backup failures logged but don't fail the workflow
- ✅ Green "Saves backed up" toast on completion

### Settings
- ✅ Ludusavi path (manual + Auto-detect via PATH scan + Browse dialog)
- ✅ SteamGridDB toggle + API key (masked with reveal)
- ✅ Device name
- ✅ Live save on commit (no Save button)
- ✅ Config JSON shape matches C# `Config.cs` exactly — no migration needed

### Toasts
- ✅ Global stack mounted in `+layout.svelte`
- ✅ 4 kinds (ok / info / warn / bad) matching design palette
- ✅ Optional CTA + catalog id badge
- ✅ Auto-dismiss for ok/info (5 s); sticky for warn/bad

---

## Module status

Reading the module layout from BACKEND.md against reality:

| Module | Status | Notes |
|---|---|---|
| `error.rs` | ✅ Done | `AppError` + `AppResult`, serializable to frontend |
| `paths.rs` | ✅ Done | `app_data_dir`, `library_file`, `config_file`, `covers_dir`, `launchers_dir` |
| `logging.rs` | ❌ Not built | Currently `eprintln!`. Tracing setup is a small follow-up. |
| `config.rs` | ✅ Done | Full ConfigData mirror; device identity + ludusavi auto-detect + exe stamp |
| `library.rs` | ✅ Done | GameEntry + Library + add/update/remove + safe_name + catalog backfill |
| `ludusavi.rs` | ✅ Done | search, manifest cache, restore, backup, open_ludusavi_gui |
| `steamgriddb.rs` | 🚧 Partial | Cover (portrait) only. Hero/wide-grid/logo deferred to Add-to-Steam slice. |
| `steam.rs` | ❌ Not built | Will land with "Add to Steam" slice |
| `armoury.rs` | ❌ Not built | Windows-only; Armoury Crate file integration |
| `launcher.rs` | ❌ Not built | Windows-only; embedded stub for Armoury Crate |
| `process.rs` | 🚧 Partial | Spawn + wait works. Run-as-admin (Win32 `runas`) deferred to v1.1. |
| `runner.rs` | ✅ Done | Full workflow + RunState + RAII guard |
| `cli.rs` | ❌ Not built | No `--run` mode yet. Currently the only entry is the library window. |
| `tray.rs` | ❌ Not built | Tray-resident model planned but unimplemented; we still spawn a fresh process per launch |
| `windows.rs` | 🚧 Partial | We open the Add Game popup via `WebviewWindow` from `+page.svelte`, but there's no dedicated orchestration module yet |
| `update.rs` | ❌ Not built | Auto-update deferred |

---

## Lessons learned

Hard-won discoveries that future-you (or any contributor) should know about.

### 1. Tauri 2 rejects periods in event names

Spent a debugging round on this. Tauri's event names must match
`[A-Za-z0-9_\-/:]+` — periods are forbidden at runtime, only surfacing
as a console error on `listen()` registration. `library.changed` fails;
`library:changed` works. Convention through the codebase is now
colon-namespaced (`library:changed`, `run:phase`, future `cover:downloaded`,
`update:available`, …).

### 2. Bun walks up the directory tree looking for a workspace root

When `tauri/` and `server/` both have `package.json` files, bun infers
a multi-project workspace and hoists shared deps to a common ancestor.
With Claude's worktree layout there's no clear anchor, so bun walked
up past the worktree boundary and created a phantom workspace at
`.claude/worktrees/{package.json,bun.lock,node_modules}`. **Fix**: add
`"workspaces": []` to `tauri/package.json` as a "stop here" marker.

(`linker = "isolated"` in `bunfig.toml` also stops the hoisting but
breaks SvelteKit on Windows because Node's resolver can't follow bun's
pnpm-style symlinks for some transitive deps like `kleur`.)

### 3. Svelte 5 narrowing doesn't propagate into snippets

Inside an `{:else if !config}` branch, the parent scope has narrowed
`config` to non-null. Snippets are closures, though, and TypeScript
treats them as separate scopes — so `bind:value={config.field}` inside
a `{#snippet}` re-broadens to `ConfigData | null`. Workaround: non-null
assertion (`config!.field`) at the use sites. Cleaner than refactoring
out the snippet API.

### 4. Don't hold `std::sync::Mutex` across `.await`

The lock rule from BACKEND.md is real and painful when broken. Every
async command in `runner.rs` and `steamgriddb.rs` snapshots what it
needs from state, drops the guard, then awaits:

```rust
let (game_name, exe_path) = {
    let lib = library.lock()?;
    let entry = lib.find(&id)?;
    (entry.game_name.clone(), entry.exe_path.clone())
}; // guard dropped before any .await below
```

If we ever need to hold state across `.await`, the plan in BACKEND.md
is to switch that specific state to `tokio::sync::Mutex`. Hasn't been
necessary yet.

### 5. Serde-derived `Deserialize` on generic structs needs an explicit bound

`SgdbResponse<T>` failed to compile with the default derive because
serde couldn't prove `T: DeserializeOwned`. Fix:

```rust
#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: serde::de::DeserializeOwned"))]
struct SgdbResponse<T> { … }
```

### 6. CSS `@keyframes` defined in component `<style>` doesn't reach inline `style:animation:` attributes

The Add Game "identifying" spinner uses `style="animation: spool-spin …"`
on SVG `<g>` elements. Defining the keyframes in the component's
`<style>` block (Svelte scopes them) means the inline `animation`
property can't find them. Moved the keyframes to `app.css` (global).
The error message `:global(@keyframes …)` is *also* rejected by Svelte's
parser, so the only path is plain global CSS.

### 7. SteamGridDB lookup by Steam ID is dramatically more reliable than name search

When the ludusavi manifest gives us a Steam ID for a game, the
SteamGridDB lookup is near-100% accurate. Falling back to name
autocomplete is fine but picks up junk (e.g. searching "Hades" matches
"Hades", "Hades II", "Hade", and friends with similar fuzzy scores).
Always prefer `steam_id` when present.

### 8. Holding a Tauri IPC connection for hours is fine

`launch_game` awaits for the entire game session — could be hours. The
underlying `tauri::ipc` channel happily stays open. Don't try to spawn
the workflow into a detached task to avoid the long hold — you'd lose
the ability to surface errors via the command's return value (which
the toast system relies on).

### 9. Cross-window events work out of the box

`AppHandle::emit("library:changed", &payload)` broadcasts to **all**
webviews. The Add Game popup updating the library state triggers a
refresh in the library window automatically. No targeted emit needed.

### 10. Window control SVG paths must be centered in their viewBox

The custom min/max/close glyphs were drawn in the upper-left ~10×10
of a 16×16 viewBox, so they rendered visibly off-center inside the
flex-centered chrome buttons. Lesson for any pixel-perfect SVG inside
a flex container: anchor the path at the viewBox center.

### 11. Bun's `linker = "isolated"` breaks SvelteKit on Windows

Tried it as a fix for the workspace-inference issue. Node fails to
resolve `kleur` (a SvelteKit transitive dep) through bun's pnpm-style
symlinks — `ERR_MODULE_NOT_FOUND`. Hoisted (default) works fine; the
`workspaces: []` marker is the right fix.

---

## Architecture decisions that paid off

- **Per-concern Tauri `State<T>`** rather than a single `AppState` — every
  command declares its dependencies as parameters, so reading any
  handler tells you exactly what state it touches. Refactor-friendly.
- **Sequential catalog numbers backfilled on first load** — gave legacy
  C# entries SPL-NNNN without a migration step. `catalog_number == 0`
  is the sentinel; backfill, save once, done.
- **JSON shape compatibility with C#** for both `library.json` and
  `config.json` — `#[serde(default)]` on every field means we can read
  existing user data with zero migration. Round-trips cleanly so we
  can ship side-by-side with the C# app during the transition.
- **`library:changed` event broadcasting** — one event, all windows
  refresh. Add Game popup commits a change; main library re-fetches
  via the same listener that fires from in-window operations.
- **Background cover fetch on add** — `tauri::async_runtime::spawn`
  from inside the (sync) `add_game` command means the new card appears
  instantly with the synthetic sleeve, and the real art swaps in
  whenever it arrives. No UI block.
- **Lazy manifest cache with `Arc<HashMap>`** — first call to ludusavi
  search pays the 1–2 s parse cost; subsequent calls are instant.
  `Arc::clone` on the cache is cheap (pointer copy), so commands hand
  it around without holding the RwLock during enrichment.
- **RAII RunGuard for the single-launch lock** — releases the slot
  even on panic. Without it a crashed workflow would leave Spool
  unable to launch any game until restart.
- **Backup failures don't fail the workflow** — game already ran
  successfully; a flaky network shouldn't show a red error. Log and
  carry on. Backup is best-effort by design.

---

## Things we deferred and why

| Deferred | Why | When |
|---|---|---|
| **Tray-resident model** | Big architectural shift, not blocking the demo | Before public release — required for `--run` flow |
| **`--run` CLI mode** | Depends on tray + single-instance plugin | Same slice as tray |
| **Single-instance plugin** | Comes with tray model | Same |
| **Run-as-admin elevation** | Needs Win32 `ShellExecute`-style invocation; most games work as user | v1.1 if users report games failing |
| **Per-game accent colour** | Need image colour extraction; pure polish, not blocking core loop | Whenever |
| **Toast UWP-style notifications (Windows)** | We have in-app toasts; the OS-level notifications are nice-to-have | v2 |
| **First-run wizard / `ludusavi-wrap` → `Spool` migration** | New users get a clean install via Settings; existing C# users have the old app working | Before public release |
| **Logging via `tracing`** | `eprintln!` works; debug.log path already chosen | Before public release |
| **Auto-update** | New build, not a feature users miss yet | Before public release |
| **Add to Steam / Armoury Crate generation** | Whole separate slice; need `steam.rs` + `launcher.rs` + `armoury.rs` | Next major slice |
| **Edit game dialog** | Existing entries can be removed and re-added; not blocking core loop | After right-click menu |
| **Right-click context menu on sidebar** | Toolbar provides the actions; menu is a polish slice | After Add to Steam |
| **Hero / wide grid / logo art** | Cover is enough for the library UI; the other art kinds are for Steam shortcut generation | With Add to Steam |
| **Sync server / LAN / TorBox / Browse Games** | All v2 features per BACKEND.md scope | v2 |

---

## Open questions

The "decisions to revisit" from BACKEND.md, with current thinking:

1. **Updater migration path** — still unresolved. The C# app uses
   `update.xml`; Tauri uses signed `latest.json`. Either parallel-publish
   during transition or ship one final C# release that prompts users to
   download the new Tauri build manually. Lean toward the manual path
   (less complexity, one-shot).

2. **Linux tray fallback** — not blocking; defer until Linux build is a
   priority.

3. **Mica / Acrylic backdrop** — leaning toward keeping the flat dark
   theming for cross-platform consistency. The cassette aesthetic doesn't
   need OS-level translucency.

4. **Per-game launch serialisation** — we have a global single-game
   mutex right now (simpler than the planned `HashMap<id, Mutex>`).
   Users rarely launch two games at once; if anyone hits the wall, swap
   to per-game then.

5. **Game-exit detection on Linux** — untested. `Child::wait` works for
   the direct-spawn case (no wrapper script). Future Steam Deck users
   will tell us.

6. **First-run wizard scope** — TBD; currently the Settings page handles
   everything but there's no guided first launch.

7. **Autostart default** — moot until tray-resident lands.

---

## Suggested next slices

In rough priority order, with rationale:

1. **Tray-resident + single-instance + `--run` CLI** — the natural next
   architectural slice. Unlocks Steam shortcut launches landing on the
   running app instead of cold-starting Spool each time. Significant
   work (~1 week) but a quality multiplier.
2. **`steam.rs` + `launcher.rs` + `armoury.rs`** — the Add to Steam /
   Armoury Crate buttons in the action toolbar become real. Needed for
   parity with the C# release.
3. **First-run flow + migration from `ludusavi-wrap`** — covers the
   "existing user installs the new build" story. Currently they'd see
   an empty library because we look at `%LOCALAPPDATA%\Spool\` but their
   data is at `%LOCALAPPDATA%\ludusavi-wrap\`.
4. **Logging via `tracing`** — small, quick win. Already have the
   debug.log path; just need the setup.
5. **Per-game accent colour** — pure polish. Extract dominant colour
   from cover image at fetch time, store on `GameEntry`, plumb through
   the existing `accent` props.
6. **Edit game dialog + right-click menu** — UX completeness.
7. **Auto-updater** — before public release.

The pattern from this session — design package in, vertical slice
out, commit, push — works well. Each slice is ~½ to 1 day of focused
work; each commit ends in a runnable state.

---

## Working with the codebase

A few practical pointers if someone picks this up cold:

- Run dev: `cd tauri && bun run tauri dev`. First boot rebuilds Rust
  (~1 min); subsequent HMR is instant for Svelte, fast for Rust.
- Type-check frontend: `bun run check`
- Compile backend: `cd src-tauri && cargo check` (or `cargo test`)
- The whole Tauri rewrite lives in `tauri/`. The C# WPF app at the
  repo root is the still-shipping production code.
- `tauri/BACKEND.md` is the design / architecture doc; this file is
  the implementation log.
- Use Lucide for any new icon — `import { Settings } from '@lucide/svelte'`,
  then `<Settings size={14} />`. The custom `Icon.svelte` is only for the
  three window-chrome glyphs.
- Add Tauri commands via `#[tauri::command]` in a domain module, then
  register in `lib.rs::run()` under `tauri::generate_handler![…]`.
- Always run BOTH `cargo check` and `bun run check` after touching the
  contract — catches drift between the Rust struct and the TS mirror.

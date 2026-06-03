# Plan: move the library to SQLite (sqlx)

## Why

Spool now runs **multiple processes against the same data at the same time**:

- the tray-resident GUI instance,
- an **attached** `spool --run … --attached` instance per Steam/Game-Mode launch
  (see `lib.rs::run_attached_launch`),
- the Decky **`spool --headless-server`** plugin server (Game Mode only).

Today every one of these loads the whole `library.json` into an in-memory
`Vec<GameEntry>`, mutates its copy, and rewrites the entire file. The atomic
write-then-rename in `library.rs::save` stops the file from *tearing*, but it
does nothing about **lost updates**: if the attached instance writes playtime
while the tray instance writes a `sync_badge`, the last writer wins and the
other change vanishes. As the data model grows (session history, save
revisions, richer play stats) the surface for these clobbers grows with it.

SQLite via `sqlx` fixes this structurally: WAL mode gives many readers + one
writer across processes, OS file locks + `busy_timeout` serialise writers
safely, and **field-level `UPDATE`s** mean two processes touching different
columns (or different games) never clobber each other. Postgres is the wrong
shape — it's a daemon to install and supervise, pointless for local single-box
state.

**Scope:** `library.json` only. `config.json` stays JSON — it's small, rarely
written, almost always by the single tray instance, human-readable, and
mirrored by the frontend. No reason to move it.

## The core problem the migration must actually solve

Swapping the storage engine is not enough. Two things must change together:

1. **Writes must become targeted**, not whole-document rewrites. A
   read-`Vec`-mutate-write pattern loses updates even on SQLite. Hot,
   concurrently-written fields get their own `UPDATE … WHERE id = ?`
   statements (ideally relative, e.g. `playtime_minutes = playtime_minutes + ?`).
2. **Reads must not trust a stale in-process cache.** The current
   `SharedLibrary` is a long-lived `Vec` snapshot. Across processes that
   snapshot goes stale the moment another process writes. The plugin server
   already sidesteps this by reloading from disk per request; the GUI must get
   a refresh signal (it can't rely on `library:changed`, which is in-process).

## Target shape

- A `Db` handle wrapping `sqlx::SqlitePool` (a small pool; SQLite is one writer
  but pooled readers are fine), stored in Tauri `State<Db>` alongside the
  existing per-concern state.
- One table `games` whose columns mirror `GameEntry`. Scalars map directly;
  the three `Vec<String>`/list fields (`genres`, `save_paths`) and any future
  collections are stored as JSON text columns (`genres TEXT NOT NULL DEFAULT
  '[]'`) — SQLite has `json_*` functions if we ever need to query into them,
  but for now they round-trip as opaque JSON, exactly as they do in the file
  today. `DateTime<Utc>` → RFC3339 `TEXT` (sqlx maps this natively).
- `id TEXT PRIMARY KEY`. Index `game_name` (used by `find_game_id_by_name`)
  and `catalog_number`.
- `sqlx::migrate!()` embedded migrations run at startup. The `_sqlx_migrations`
  table makes this idempotent and versioned — this replaces the ad-hoc
  `#[serde(default)]` "missing key" compatibility story with real schema
  migrations (adding a column is `ALTER TABLE`, not a serde default).
- PRAGMAs on every connection via `sqlx`'s `after_connect` /
  `SqliteConnectOptions`: `journal_mode=WAL`, `busy_timeout=5000`,
  `synchronous=NORMAL`, `foreign_keys=ON`.

### Keep the `Library`/`SharedLibrary` façade, change its guts

24 files touch `SharedLibrary`/`.lock()`. We do **not** want to rewrite all of
them in one pass. Strategy: keep a `Library` type and a `SharedLibrary` alias as
the public interface, but back it with the pool instead of a `Vec`, and make its
methods `async` DB calls. Most call sites that do
`lib.entries.iter().find(...)` become `db.find(id).await` / `db.list().await` —
mechanical, reviewable, and convertible file-by-file.

Hot write paths get dedicated methods instead of "mutate the struct, save":
- `db.bump_playtime(id, minutes).await` (relative update — the runner)
- `db.record_backup(id, count, size_mb, at).await` (the runner)
- `db.set_sync_badge(id, badge).await`, `db.set_cloud_baseline(id, base).await`
- `db.set_accent(id, color).await` / `db.set_install_size(id, mb).await`
  (backfills — these run in the GUI process only, low contention, but cheap to
  make targeted anyway)
- `db.upsert(entry).await` for add/edit (full-row write is correct there —
  it's a user action on one game, not a concurrent field bump)

### Cross-process change notification

`library:changed` is a Tauri event = in-process only. After the migration the
GUI needs to know when *another* process wrote. Options, cheapest first:

1. **`updated_at` / monotonic `version` poll.** A `meta(version INTEGER)` row
   bumped in the same transaction as any write; the GUI polls it on a slow
   timer (e.g. 2–5 s) and re-queries when it changes. Dead simple, no new deps.
2. **File-watch the `-wal` file** with `notify` and debounce. More immediate,
   more moving parts.

Recommend (1) for the first cut — the only cross-process writer in practice is
the attached instance at end-of-session, so a few seconds of latency for the
tray GUI to reflect updated playtime is fine. The attached/headless processes
already read fresh from the DB per operation, so they need nothing.

## Migration / rollout order

Each step compiles, passes tests, and ships independently.

1. **Add deps + plumbing, no behaviour change.** Add `sqlx` (features:
   `runtime-tokio`, `sqlite`, `chrono`, `macros`, `migrate`). Create
   `db.rs` with the pool, PRAGMAs, and the `games` migration. Add
   `paths::library_db()` → `%LOCALAPPDATA%\Spool\library.db`. Wire `Db` into
   Tauri state but don't read from it yet.

2. **One-shot import from `library.json`.** On startup, if `library.db` is
   absent (or empty) and `library.json` exists, load the JSON via the existing
   serde path and insert every entry. Keep `library.json` on disk untouched as
   a fallback/export for one or two releases (don't delete it — it's the
   rollback path). The importer currently relies on `Library::load` having
   already backfilled catalog numbers on the entries it's handed; that
   backfill must move into the importer before the JSON loader is removed
   (step 6), or imported rows would get `0`.

3. **Switch reads.** Convert `list_games`, `find`, and the read-only call sites
   (LAN `PeerGame::from_entry`, plugin server `/library`, steam/launcher
   shortcut builders, diagnostics) to query the DB. The plugin server's
   "reload from disk every request" becomes "just query" — same freshness,
   less code.

   **Wire the pool into the headless entry points here.** Today
   `Db::init` only runs in the GUI/attached-`--run` processes; the
   `--backup` / `--release-lock` / `--headless-server` subcommands
   `std::process::exit()` in `lib.rs` *before* the pool is created
   (`headless.rs`, `plugin_server.rs` still go through `Library::load()` /
   per-request JSON reload). That means the three-live-processes contention
   case the migration exists for is **not actually exercised against the db
   until this step opens a pool in those entry points.** Don't skip it.

   This is also where the db becomes load-bearing, so add the
   migration/connect resilience deferred from step 1: a small retry around
   `Db::init` (a few attempts, short backoff) so a transient `SQLITE_BUSY`
   while another process is mid-migration doesn't leave a process with no db.

4. **Switch writes to targeted methods.** Convert `add_game`/`update_game`/
   `remove_game`/`delete_game_core` to `upsert`/`delete`, and the runner +
   backfills to the field-level setters above. This is the step that actually
   kills the lost-update bug.

5. **Cross-process refresh.** Add the `version` row + GUI poll (option 1).

6. **Cleanup.** Remove the `Vec`-backed `Library`, the `.bak` rotation, and the
   serde round-trip comments that no longer apply. Decide when to stop writing
   the legacy `library.json` mirror (probably keep an export-to-json command for
   user peace of mind).

## Things to get right (gotchas)

- **Lock discipline still applies.** `sqlx` is async; never hold the old
  `std::sync::Mutex` across `.await`. The façade methods become `async` and the
  `Mutex` goes away entirely for library state — a net simplification.
- **`busy_timeout` is mandatory.** Without it, a concurrent writer gets
  `SQLITE_BUSY` instead of waiting. 5 s is plenty for these tiny writes.
- **WAL needs local disk.** `app_data_dir()` is always local, so fine — but
  never point the DB at a network/`rclone` path.
- **Three live processes during a Game-Mode session.** Test the real scenario:
  headless-server + attached `--run` + (if present) tray GUI, all hitting
  `library.db`. This is the case that motivated the whole migration.
- **The `sqlx` compile-time macros need a DB at build time** (or
  `SQLX_OFFLINE=true` + a committed `.sqlx/` query cache). Decide up front:
  use `query!`/`query_as!` with a committed offline cache (CI-friendly, no DB
  needed to build), or plain `sqlx::query` (runtime-checked, no cache). Given
  CI builds on Windows + Linux without a DB, **commit the `.sqlx` offline cache**
  and set `SQLX_OFFLINE=true` in CI.
- **`GameEntry` stays the serde DTO** across the IPC boundary and for the LAN
  `PeerGame` shape — we map rows ↔ `GameEntry`, we don't expose sqlx types to
  the frontend. `types.ts` is unaffected.
- **Frontend is unaffected** beyond the optional version-poll: `api.listGames()`
  etc. keep the same signatures.

## Out of scope (for now)

- Moving `config.json` to SQLite.
- Querying into the JSON list columns (`genres`, `save_paths`) — they stay
  opaque until a feature needs server-side filtering on them.
- A full session-history / save-revision schema. The migration lays the
  groundwork (a real DB) but those tables are follow-on work.

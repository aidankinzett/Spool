# Touch Mode PR 4 — Library Controller Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract all non-markup logic out of `+page.svelte` into `lib/library.svelte.ts` so the two-pane desktop view (`LibraryDesktop.svelte`) is a pure view over a shared controller, enabling PR 5 to add `LibraryTouch` over the same controller without duplicating any logic.

**Architecture:** `createLibrary()` (a runes-closure function in `library.svelte.ts`) owns all `$state`, `$derived`, Tauri event subscriptions, and action methods. It returns a typed `Library` object via getters + setters. `LibraryDesktop.svelte` receives `lib: Library` as a prop and owns only UI-local state (popovers, element refs, context menu) plus the 525-line markup — unchanged from today. `+page.svelte` becomes a 5-line thin shell. `filterGames` is extracted as a pure exported helper enabling direct unit tests without a component context.

**Tech Stack:** SvelteKit 5 (runes — `$state`, `$derived`, `onMount`, `SvelteSet`), Tauri 2 event system (`listen`), TypeScript, Vitest + jsdom. No new dependencies.

This is PR 4 of the 6-PR rollout in `design_handoff_touch_mode/Touch Mode - Refactor Plan.md` (§6a). Depends on PR 3 (`lib/nav.ts`, `AppChrome`). Branch: `touch-mode-pr4-library-controller`.

---

## Why filterGames is extracted as a pure helper

`$derived` values inside a runes closure are reactive in a live app but are opaque to Vitest — you can't read `lib.filteredGames` in a jsdom test without a full Svelte component context. Extracting `filterGames` as a standalone exported function breaks that dependency: the filter/search logic is unit-testable with plain function calls. The `$derived` inside the controller then calls it. One function, two callers (controller's `$derived` + test suite).

## Out of scope for PR 4

- `LibraryTouch.svelte` — PR 5.
- `{#if uiMode.resolved === 'touch'}` branch in `+page.svelte` — PR 5.
- Run-state tests — the `run:phase` listener setup requires a component context; e2e covers it.
- The bespoke "Add a game" / "Add your first game" button token debt — deferred to a cleanup pass.

## File map

| File | Change |
|---|---|
| `tauri/src/lib/library.svelte.ts` | **Create** — `filterGames()` pure helper + `createLibrary()` controller + `Library` export type |
| `tauri/src/lib/components/LibraryDesktop.svelte` | **Create** — desktop two-pane view, consumes `lib: Library` prop |
| `tauri/src/routes/+page.svelte` | **Replace** — thin shell: `createLibrary()` + `<LibraryDesktop {lib} />` |
| `tauri/src/lib/library.test.ts` | **Create** — unit tests for `filterGames` |

---

## Task 0: Confirm baseline (inline — no subagent)

- [ ] **Step 1: Confirm branch and checks green**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git branch --show-current   # expect: touch-mode-pr4-library-controller
```
```bash
cd tauri/src-tauri && cargo check
cd ../tauri && bun run check && bun run lint && bun run test
```
Expected: all pass. The test suite currently has `format.test.ts` and `GameDetail.test.ts`.

---

## Task 1: Create `lib/library.svelte.ts` — the controller

**Files:**
- Create: `tauri/src/lib/library.svelte.ts`

This is a verbatim extraction of all non-markup logic from `+page.svelte` lines 1–533, reorganised into a closure. Read `tauri/src/routes/+page.svelte` before writing to confirm variable names match exactly.

- [ ] **Step 1: Write the file**

Create `tauri/src/lib/library.svelte.ts` with exactly:

```ts
import { onMount } from 'svelte';
import { SvelteSet } from 'svelte/reactivity';
import { listen } from '@tauri-apps/api/event';
import { api } from '$lib/api';
import { fmtCatalog } from '$lib/format';
import { toasts } from '$lib/toasts.svelte';
import { checkForUpdateOnStartup } from '$lib/updater';
import type {
  ConfigData,
  DownloadProgress,
  GameEntry,
  LanPeer,
  PeerGame,
  RunPhase,
  RunPhaseEvent,
  SyncStatus,
  UploadSnapshot,
} from '$lib/types';

// ── Pure filter helper — exported so tests can call it without a component ──
/** Filter and sort the game list by the current filter + search query. */
export function filterGames(
  games: GameEntry[],
  filter: 'all' | 'recent' | 'played',
  searchQuery: string,
): GameEntry[] {
  let list = games.slice();
  if (filter === 'recent') {
    list = list
      .filter((g) => g.last_played_at != null || g.added_at != null)
      .sort((a, b) => {
        const at = a.last_played_at ?? a.added_at ?? '';
        const bt = b.last_played_at ?? b.added_at ?? '';
        return bt.localeCompare(at);
      });
  } else if (filter === 'played') {
    list = list.filter((g) => g.playtime_minutes > 0);
  }
  if (searchQuery.trim()) {
    const q = searchQuery.toLowerCase();
    list = list.filter((g) => g.game_name.toLowerCase().includes(q));
  }
  return list;
}

// ── Controller ───────────────────────────────────────────────────────────────
export function createLibrary() {
  // Core library state
  let games = $state<GameEntry[]>([]);
  let config = $state<ConfigData | null>(null);
  let loaded = $state(false);
  let error = $state<string | null>(null);

  // Selection + search
  let selectedId = $state<string | null>(null);
  let searchQuery = $state('');
  let filter = $state<'all' | 'recent' | 'played'>('all');

  // Run tracking
  let runningId = $state<string | null>(null);
  let runningPhase = $state<RunPhase | null>(null);

  // LAN state
  let lanPeers = $state<LanPeer[]>([]);
  let openPeer = $state<LanPeer | null>(null);
  let peerGames = $state<PeerGame[]>([]);
  let peerGamesLoading = $state(false);
  let peerGamesError = $state<string | null>(null);
  let activeDownload = $state<DownloadProgress | null>(null);
  let startingGameId = $state<string | null>(null);
  // Lives outside $state — changing it never affects rendering.
  const toastedDownloadTokens = new SvelteSet<string>();
  let activeUploads = $state<UploadSnapshot[]>([]);

  // Sync state
  let syncStatus = $state<SyncStatus>({
    reachability: 'unconfigured',
    server_version: null,
    error: null,
    last_ok_ago_secs: null,
  });

  // Derived
  const filteredGames = $derived(filterGames(games, filter, searchQuery));
  const selectedGame = $derived(
    selectedId ? games.find((g) => g.id === selectedId) ?? null : null,
  );
  const syncOk = $derived(syncStatus.reachability === 'online');
  const syncOff = $derived(syncStatus.reachability === 'offline');
  const syncTitle = $derived(
    syncStatus.reachability === 'unconfigured'
      ? 'Sync server not configured — open Settings to set it up'
      : syncOk
        ? `Sync server online${syncStatus.server_version ? ` · v${syncStatus.server_version}` : ''}`
        : `Sync server unreachable${syncStatus.error ? ` · ${syncStatus.error}` : ''}`,
  );
  const downloadActive = $derived(
    activeDownload != null &&
      (activeDownload.status === 'starting' || activeDownload.status === 'transferring'),
  );
  const downloadCount = $derived(downloadActive ? 1 : 0);
  const downloadPercent = $derived(
    activeDownload && activeDownload.bytes_total > 0
      ? Math.round((activeDownload.bytes_done / activeDownload.bytes_total) * 100)
      : 0,
  );
  const liveUploads = $derived(activeUploads.filter((u) => !u.cancelled));
  const uploadCount = $derived(liveUploads.length);
  const uploadPercent = $derived(uploadCount > 0 ? 60 : 0);

  // Methods
  async function refresh() {
    try {
      games = await api.listGames();
      if (selectedId && !games.some((g) => g.id === selectedId)) {
        selectedId = games[0]?.id ?? null;
      } else if (!selectedId && games.length > 0) {
        selectedId = games[0].id;
      } else if (games.length === 0) {
        selectedId = null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loaded = true;
    }
  }

  async function refreshLanPeers() {
    try {
      lanPeers = await api.listLanPeers();
    } catch (e) {
      console.error('[lan] listLanPeers failed:', e);
    }
  }

  async function refreshActiveUploads() {
    try {
      activeUploads = await api.listActiveUploads();
    } catch (e) {
      console.error('[lan] listActiveUploads failed:', e);
    }
  }

  async function kickUpload(session: UploadSnapshot) {
    try {
      await api.cancelUpload(session.session_id);
    } catch (e) {
      console.error('[lan] cancelUpload failed:', e);
    }
  }

  async function openPeerView(peer: LanPeer) {
    openPeer = peer;
    peerGames = [];
    peerGamesError = null;
    if (peer.file_server_port === 0) {
      peerGamesError = 'This peer is discovery-only (no file server yet).';
      return;
    }
    peerGamesLoading = true;
    try {
      peerGames = await api.fetchPeerGames(peer.addr, peer.file_server_port);
    } catch (e) {
      peerGamesError = String(e);
    } finally {
      peerGamesLoading = false;
    }
  }

  function backToPeerList() {
    openPeer = null;
    peerGames = [];
    peerGamesError = null;
  }

  /** Called by LibraryDesktop's closeLanPopover to reset peer drill-down state. */
  function clearPeerView() {
    openPeer = null;
    peerGames = [];
    peerGamesError = null;
  }

  async function cancelActiveInstall() {
    if (!activeDownload) return;
    try {
      await api.cancelPeerInstall(activeDownload.install_token);
    } catch (e) {
      console.error('[lan] cancel install failed:', e);
    }
  }

  async function installFromPeer(peer: LanPeer, game: PeerGame) {
    if (
      activeDownload &&
      (activeDownload.status === 'starting' || activeDownload.status === 'transferring')
    ) {
      toasts.show({
        kind: 'warn',
        label: 'LAN',
        title: 'Another install is in progress',
        sub: `Finish ${activeDownload.game_name} first.`,
      });
      return;
    }
    startingGameId = game.id;
    try {
      await api.startPeerInstall(peer.addr, peer.file_server_port, game.id);
      toasts.show({
        kind: 'info',
        label: 'LAN',
        title: 'Install started',
        sub: `${game.game_name} · from ${peer.device_name}`,
      });
    } catch (e) {
      toasts.show({
        kind: 'bad',
        label: 'LAN',
        title: 'Install failed to start',
        sub: String(e),
      });
    } finally {
      if (startingGameId === game.id) startingGameId = null;
    }
  }

  function showRunErrorToast(gameId: string, message: string) {
    const game = games.find((g) => g.id === gameId);
    const catalog = game ? fmtCatalog(game.catalog_number) : undefined;
    const subjectName = game?.game_name ?? 'this game';
    if (/cloud sync conflict/i.test(message)) {
      toasts.show({
        kind: 'warn',
        label: 'LUDUSAVI · CONFLICT',
        catalog,
        title: 'Cloud sync conflict',
        sub: `${subjectName} has different saves locally and in the cloud. Open Ludusavi to pick which to keep.`,
        cta: {
          label: 'Open Ludusavi',
          onClick: () => {
            api.openLudusaviGui().catch((e) => console.error('[ludusavi] open failed:', e));
          },
        },
      });
      return;
    }
    toasts.show({
      kind: 'bad',
      label: 'LAUNCH · FAILED',
      catalog,
      title: "Couldn't launch game",
      sub: message,
    });
  }

  // onMount: initial fetch + Tauri event subscriptions
  onMount(() => {
    refresh();
    api
      .getConfig()
      .then((c) => (config = c))
      .catch((e) => console.error('[library] getConfig failed:', e));
    setTimeout(() => {
      checkForUpdateOnStartup().catch((e) =>
        console.error('[updater] startup check failed:', e),
      );
    }, 2000);

    let unlistenLibraryChanged: (() => void) | undefined;
    let unlistenRunPhase: (() => void) | undefined;
    let unlistenTrayIntro: (() => void) | undefined;
    let unlistenLanPeers: (() => void) | undefined;
    let unlistenLanDownload: (() => void) | undefined;
    let unlistenLanUploads: (() => void) | undefined;
    let unlistenSyncStatus: (() => void) | undefined;

    listen<string>('library:changed', () => refresh())
      .then((fn) => (unlistenLibraryChanged = fn))
      .catch((e) => console.error('[library] listener failed:', e));

    listen<RunPhaseEvent>('run:phase', (event) => {
      const { game_id, phase, message } = event.payload;
      if (phase === 'done' || phase === 'error') {
        runningId = null;
        runningPhase = null;
      } else {
        runningId = game_id;
        runningPhase = phase;
      }
      if (phase === 'error') {
        showRunErrorToast(game_id, message ?? 'Game launch failed');
      } else if (phase === 'done') {
        const game = games.find((g) => g.id === game_id);
        if (game) {
          toasts.show({
            kind: 'ok',
            label: 'LUDUSAVI',
            title: 'Saves backed up',
            sub: `${game.game_name} · session complete`,
            catalog: fmtCatalog(game.catalog_number),
          });
        }
      }
    })
      .then((fn) => {
        unlistenRunPhase = fn;
        return api.takePendingRun();
      })
      .then((pendingId) => {
        if (pendingId) {
          api
            .launchGame(pendingId)
            .catch((e) => console.error('[lifecycle] pending --run failed:', e));
        }
      })
      .catch((e) => console.error('[library] run-phase listener failed:', e));

    listen<null>('tray:first-hide', () => {
      toasts.show({
        kind: 'info',
        label: 'TRAY',
        title: 'Spool is still running',
        sub: 'Click the tray icon to bring the window back. You can quit fully from the tray menu.',
        duration: 0,
      });
    })
      .then((fn) => (unlistenTrayIntro = fn))
      .catch((e) => console.error('[tray] intro listener failed:', e));

    refreshLanPeers();
    listen<null>('lan:peers-changed', () => refreshLanPeers())
      .then((fn) => (unlistenLanPeers = fn))
      .catch((e) => console.error('[lan] peers listener failed:', e));

    api
      .currentPeerDownload()
      .then((p) => {
        if (p) activeDownload = p;
      })
      .catch((e) => console.error('[lan] currentPeerDownload failed:', e));

    api
      .currentSyncStatus()
      .then((s) => (syncStatus = s))
      .catch((e) => console.error('[sync] currentSyncStatus failed:', e));
    listen<SyncStatus>('sync:status-changed', (event) => {
      syncStatus = event.payload;
    })
      .then((fn) => (unlistenSyncStatus = fn))
      .catch((e) => console.error('[sync] status listener failed:', e));

    refreshActiveUploads();
    listen<null>('lan:uploads-changed', () => refreshActiveUploads())
      .then((fn) => (unlistenLanUploads = fn))
      .catch((e) => console.error('[lan] uploads listener failed:', e));

    listen<DownloadProgress>('lan:download', (event) => {
      const p = event.payload;
      const isTerminal =
        p.status === 'done' || p.status === 'error' || p.status === 'canceled';
      const firstTerminal = isTerminal && !toastedDownloadTokens.has(p.install_token);
      if (firstTerminal) toastedDownloadTokens.add(p.install_token);
      activeDownload = p;
      if (p.status === 'done' && firstTerminal) {
        toasts.show({ kind: 'ok', label: 'LAN', title: 'Install complete',
          sub: `${p.game_name} · from ${p.source_device_name}` });
      } else if (p.status === 'error' && firstTerminal) {
        toasts.show({ kind: 'bad', label: 'LAN', title: 'Install failed',
          sub: p.message ?? `${p.game_name} could not be installed` });
      } else if (p.status === 'canceled' && firstTerminal) {
        toasts.show({ kind: 'info', label: 'LAN', title: 'Install cancelled',
          sub: `${p.game_name} · partial files cleaned up` });
      }
    })
      .then((fn) => (unlistenLanDownload = fn))
      .catch((e) => console.error('[lan] download listener failed:', e));

    return () => {
      unlistenLibraryChanged?.();
      unlistenRunPhase?.();
      unlistenTrayIntro?.();
      unlistenLanPeers?.();
      unlistenLanDownload?.();
      unlistenLanUploads?.();
      unlistenSyncStatus?.();
    };
  });

  return {
    // Read state
    get games() { return games; },
    get config() { return config; },
    get loaded() { return loaded; },
    get error() { return error; },
    get runningId() { return runningId; },
    get runningPhase() { return runningPhase; },
    get lanPeers() { return lanPeers; },
    get openPeer() { return openPeer; },
    get peerGames() { return peerGames; },
    get peerGamesLoading() { return peerGamesLoading; },
    get peerGamesError() { return peerGamesError; },
    get activeDownload() { return activeDownload; },
    get startingGameId() { return startingGameId; },
    get activeUploads() { return activeUploads; },
    get syncStatus() { return syncStatus; },
    // Writable state (view binds directly)
    get selectedId() { return selectedId; },
    set selectedId(v: string | null) { selectedId = v; },
    get searchQuery() { return searchQuery; },
    set searchQuery(v: string) { searchQuery = v; },
    get filter() { return filter; },
    set filter(v: 'all' | 'recent' | 'played') { filter = v; },
    // Derived (read-only)
    get filteredGames() { return filteredGames; },
    get selectedGame() { return selectedGame; },
    get syncOk() { return syncOk; },
    get syncOff() { return syncOff; },
    get syncTitle() { return syncTitle; },
    get downloadActive() { return downloadActive; },
    get downloadCount() { return downloadCount; },
    get downloadPercent() { return downloadPercent; },
    get liveUploads() { return liveUploads; },
    get uploadCount() { return uploadCount; },
    get uploadPercent() { return uploadPercent; },
    // Methods
    refresh,
    refreshLanPeers,
    refreshActiveUploads,
    kickUpload,
    openPeerView,
    backToPeerList,
    clearPeerView,
    cancelActiveInstall,
    installFromPeer,
  };
}

export type Library = ReturnType<typeof createLibrary>;
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run check && bun run lint
```
Expected: both PASS. (No consumer yet — this just type-checks.)

- [ ] **Step 3: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/library.svelte.ts
git commit -m "feat(touch): add library.svelte.ts controller (createLibrary + filterGames)

Extracts all logic from +page.svelte into a runes-closure controller.
filterGames() is exported as a pure helper for unit tests. Library type
exported for the LibraryDesktop prop. Desktop unchanged until
LibraryDesktop is wired in next tasks.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Write filterGames unit tests

**Files:**
- Create: `tauri/src/lib/library.test.ts`
- Test runner: `bun run test` (Vitest, jsdom, pattern `src/**/*.{test,spec}.{js,ts}`)

`filterGames` is pure — it takes `(games, filter, searchQuery)` and returns a filtered+sorted array. No mocks needed.

The `GameEntry` type has many fields; the only ones `filterGames` reads are `game_name`, `last_played_at`, `added_at`, `playtime_minutes`. Use a minimal factory.

- [ ] **Step 1: Write the failing tests**

Create `tauri/src/lib/library.test.ts` with:

```ts
import { describe, it, expect } from 'vitest';
import { filterGames } from '$lib/library.svelte';
import type { GameEntry } from '$lib/types';

function g(over: Partial<GameEntry> & { id: string; game_name: string }): GameEntry {
  return {
    catalog_number: 1,
    exe_path: '',
    safe_name: '',
    cover_image_path: null,
    hero_image_path: null,
    added_at: null,
    last_played_at: null,
    launcher_exe_path: null,
    game_folder_path: null,
    run_as_admin: false,
    description: null,
    developer: null,
    publisher: null,
    genres: [],
    release_date: null,
    install_size_mb: 0,
    playtime_minutes: 0,
    lan_shared: false,
    lan_share_folder: null,
    save_backup_count: 0,
    save_last_backed_up_at: null,
    save_backup_size_mb: 0,
    install_source: '',
    lan_install_source_device_name: null,
    accent_color: null,
    sync_badge: null,
    ...over,
  };
}

const HOLLOW = g({ id: 'hk', game_name: 'Hollow Knight', playtime_minutes: 120, last_played_at: '2026-05-20T10:00:00Z' });
const ELDEN  = g({ id: 'er', game_name: 'Elden Ring',    playtime_minutes: 0,   last_played_at: null, added_at: '2026-05-15T10:00:00Z' });
const CELESTE = g({ id: 'ce', game_name: 'Celeste',       playtime_minutes: 0,   last_played_at: null, added_at: null });
const GAMES = [HOLLOW, ELDEN, CELESTE];

describe('filterGames — filter: all', () => {
  it('returns all games with no search query', () => {
    expect(filterGames(GAMES, 'all', '')).toEqual(GAMES);
  });

  it('filters case-insensitively by game_name', () => {
    expect(filterGames(GAMES, 'all', 'hollow')).toEqual([HOLLOW]);
    expect(filterGames(GAMES, 'all', 'HOLLOW')).toEqual([HOLLOW]);
    expect(filterGames(GAMES, 'all', 'eld')).toEqual([ELDEN]);
  });

  it('returns empty array when search matches nothing', () => {
    expect(filterGames(GAMES, 'all', 'xyzzy')).toEqual([]);
  });

  it('ignores whitespace-only search query', () => {
    expect(filterGames(GAMES, 'all', '   ')).toEqual(GAMES);
  });
});

describe('filterGames — filter: played', () => {
  it('returns only games with playtime_minutes > 0', () => {
    expect(filterGames(GAMES, 'played', '')).toEqual([HOLLOW]);
  });

  it('combines played filter with search', () => {
    expect(filterGames(GAMES, 'played', 'elden')).toEqual([]);
    expect(filterGames(GAMES, 'played', 'hollow')).toEqual([HOLLOW]);
  });
});

describe('filterGames — filter: recent', () => {
  it('excludes games with no last_played_at and no added_at', () => {
    const result = filterGames(GAMES, 'recent', '');
    expect(result).not.toContainEqual(CELESTE);
  });

  it('includes games with either last_played_at or added_at', () => {
    const result = filterGames(GAMES, 'recent', '');
    expect(result).toContainEqual(HOLLOW);
    expect(result).toContainEqual(ELDEN);
  });

  it('sorts most-recent first (last_played_at preferred over added_at)', () => {
    const result = filterGames(GAMES, 'recent', '');
    expect(result[0]).toEqual(HOLLOW); // 2026-05-20 > 2026-05-15
    expect(result[1]).toEqual(ELDEN);
  });

  it('combines recent filter with search', () => {
    expect(filterGames(GAMES, 'recent', 'elden')).toEqual([ELDEN]);
  });
});
```

- [ ] **Step 2: Run tests — expect FAIL (filterGames not yet importable from the alias)**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run test
```

If Task 1 is already committed, `filterGames` is importable and the tests should PASS immediately (the function is pure and correct). If they fail, the error message will point to a type mismatch in the `g()` factory — compare against `tauri/src/lib/types.ts` `GameEntry` and fix the factory fields.

- [ ] **Step 3: Confirm all tests pass**

```bash
bun run test
```
Expected: all tests in `library.test.ts` PASS.

- [ ] **Step 4: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/library.test.ts
git commit -m "test(touch): add filterGames unit tests

Covers all/played/recent filters, case-insensitive search, combination
of filter + search, and sort order for recent. Pure function, no mocks.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Create `LibraryDesktop.svelte` — the view layer

**Files:**
- Create: `tauri/src/lib/components/LibraryDesktop.svelte`

This file gets:
- The **script section**: all UI-local state + handlers + imports needed by the template
- The **template**: the exact markup from `+page.svelte` lines 535–1059, with every controller variable prefixed by `lib.`

### Step 1: Understand what stays LOCAL in LibraryDesktop

These are NOT in the controller and must be declared locally in LibraryDesktop's script:

```ts
// UI-only open/close state
let lanOpen = $state(false);
let transfersOpen = $state(false);
let ctxMenu = $state<{ game: GameEntry; x: number; y: number } | null>(null);

// Element refs for click-outside detection
let lanWifiBtn: HTMLButtonElement | undefined = $state();
let lanPopoverEl: HTMLDivElement | undefined = $state();
let transferPillEl: HTMLSpanElement | undefined = $state();
let transfersPanelEl: HTMLDivElement | undefined = $state();

// Display-only constant
const filters: { id: 'all' | 'recent' | 'played'; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'recent', label: 'Recent' },
  { id: 'played', label: 'Played' },
];

// UI handlers
function openContextMenu(e: MouseEvent, g: GameEntry) {
  e.preventDefault();
  ctxMenu = { game: g, x: e.clientX, y: e.clientY };
}

function closeLanPopover() {
  lanOpen = false;
  lib.clearPeerView();  // resets openPeer + peerGames + peerGamesError in controller
}

function handleLanOutside(e: MouseEvent) {
  if (!lanOpen) return;
  if (lanPopoverEl?.contains(e.target as Node)) return;
  if (lanWifiBtn?.contains(e.target as Node)) return;
  closeLanPopover();
}

function handleTransfersOutside(e: MouseEvent) {
  if (!transfersOpen) return;
  if (transfersPanelEl?.contains(e.target as Node)) return;
  if (transferPillEl?.contains(e.target as Node)) return;
  transfersOpen = false;
}

onMount(() => {
  document.addEventListener('mousedown', handleLanOutside, true);
  document.addEventListener('mousedown', handleTransfersOutside, true);
  return () => {
    document.removeEventListener('mousedown', handleLanOutside, true);
    document.removeEventListener('mousedown', handleTransfersOutside, true);
  };
});
```

### Step 2: Understand the substitution table for the template markup

The template (lines 535–1059 of `+page.svelte`) references controller state and methods by their bare names. In LibraryDesktop, ALL of these must be prefixed with `lib.`:

| Original (bare name) | In LibraryDesktop |
|---|---|
| `games` | `lib.games` |
| `config` | `lib.config` |
| `loaded` | `lib.loaded` |
| `error` | `lib.error` |
| `selectedId` | `lib.selectedId` |
| `searchQuery` | `lib.searchQuery` |
| `filter` | `lib.filter` |
| `filteredGames` | `lib.filteredGames` |
| `selectedGame` | `lib.selectedGame` |
| `runningId` | `lib.runningId` |
| `runningPhase` | `lib.runningPhase` |
| `lanPeers` | `lib.lanPeers` |
| `openPeer` | `lib.openPeer` |
| `peerGames` | `lib.peerGames` |
| `peerGamesLoading` | `lib.peerGamesLoading` |
| `peerGamesError` | `lib.peerGamesError` |
| `activeDownload` | `lib.activeDownload` |
| `startingGameId` | `lib.startingGameId` |
| `activeUploads` | `lib.activeUploads` |
| `syncOk` | `lib.syncOk` |
| `syncOff` | `lib.syncOff` |
| `syncTitle` | `lib.syncTitle` |
| `downloadCount` | `lib.downloadCount` |
| `downloadPercent` | `lib.downloadPercent` |
| `uploadCount` | `lib.uploadCount` |
| `uploadPercent` | `lib.uploadPercent` |
| `backToPeerList` | `lib.backToPeerList` |
| `openPeerView` | `lib.openPeerView` |
| `cancelActiveInstall` | `lib.cancelActiveInstall` |
| `kickUpload` | `lib.kickUpload` |
| `installFromPeer` | `lib.installFromPeer` |
| `refresh` (not used in template) | — |

These stay as **bare names** (local to LibraryDesktop — do NOT prefix):
- `openContextMenu`, `closeLanPopover`, `handleLanOutside`, `handleTransfersOutside`
- `lanOpen`, `transfersOpen`, `ctxMenu`
- `lanWifiBtn`, `lanPopoverEl`, `transferPillEl`, `transfersPanelEl`
- `filters` (the display constant)
- `openView` (imported from `$lib/nav`)
- `assetUrl` (imported from `$lib/api`)
- `fmtCatalog`, `fmtRate`, `relDate` (imported from `$lib/format`)

Two template lines need careful rewriting:

**`bind:value` on search** (in sidebar search input, ~line 867):
```svelte
<!-- OLD: -->
bind:value={searchQuery}
<!-- NEW: Svelte 5 bind: works with getters+setters, so this is correct: -->
bind:value={lib.searchQuery}
```

**Filter button assignment** (~line 877):
```svelte
<!-- OLD: -->
onclick={() => (filter = f.id)}
<!-- NEW: -->
onclick={() => (lib.filter = f.id)}
```

**Game row selection assignment** (~line 949):
```svelte
<!-- OLD: -->
onclick={() => (selectedId = g.id)}
<!-- NEW: -->
onclick={() => (lib.selectedId = g.id)}
```

- [ ] **Step 1: Write the script section of `LibraryDesktop.svelte`**

Create `tauri/src/lib/components/LibraryDesktop.svelte`. Start with the `<script>` block:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    ArrowLeft,
    BookOpen,
    ChevronRight,
    Cloud,
    CloudOff,
    Download,
    Loader2,
    Plus,
    Search,
    Settings,
    Wifi,
    X,
  } from '@lucide/svelte';
  import { openView } from '$lib/nav';
  import { assetUrl } from '$lib/api';
  import { fmtCatalog, fmtRate, relDate } from '$lib/format';
  import type { GameEntry } from '$lib/types';
  import type { Library } from '$lib/library.svelte';
  import AppChrome from '$lib/components/AppChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import GameDetail from '$lib/components/GameDetail.svelte';
  import LibraryContextMenu from '$lib/components/LibraryContextMenu.svelte';
  import TransferPill from '$lib/components/TransferPill.svelte';
  import TransfersPanel from '$lib/components/TransfersPanel.svelte';

  let { lib }: { lib: Library } = $props();

  // UI-only state (not in controller)
  let lanOpen = $state(false);
  let transfersOpen = $state(false);
  let ctxMenu = $state<{ game: GameEntry; x: number; y: number } | null>(null);

  // Element refs for click-outside detection
  let lanWifiBtn: HTMLButtonElement | undefined = $state();
  let lanPopoverEl: HTMLDivElement | undefined = $state();
  let transferPillEl: HTMLSpanElement | undefined = $state();
  let transfersPanelEl: HTMLDivElement | undefined = $state();

  // Display-only constant
  const filters: { id: 'all' | 'recent' | 'played'; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'recent', label: 'Recent' },
    { id: 'played', label: 'Played' },
  ];

  function openContextMenu(e: MouseEvent, g: GameEntry) {
    e.preventDefault();
    ctxMenu = { game: g, x: e.clientX, y: e.clientY };
  }

  function closeLanPopover() {
    lanOpen = false;
    lib.clearPeerView();
  }

  function handleLanOutside(e: MouseEvent) {
    if (!lanOpen) return;
    if (lanPopoverEl?.contains(e.target as Node)) return;
    if (lanWifiBtn?.contains(e.target as Node)) return;
    closeLanPopover();
  }

  function handleTransfersOutside(e: MouseEvent) {
    if (!transfersOpen) return;
    if (transfersPanelEl?.contains(e.target as Node)) return;
    if (transferPillEl?.contains(e.target as Node)) return;
    transfersOpen = false;
  }

  onMount(() => {
    document.addEventListener('mousedown', handleLanOutside, true);
    document.addEventListener('mousedown', handleTransfersOutside, true);
    return () => {
      document.removeEventListener('mousedown', handleLanOutside, true);
      document.removeEventListener('mousedown', handleTransfersOutside, true);
    };
  });
</script>
```

- [ ] **Step 2: Append the template to `LibraryDesktop.svelte`**

Open `tauri/src/routes/+page.svelte` and copy everything from line 535 to line 1059 (inclusive — from `<div class="flex h-screen flex-col bg-bg-0 text-ink-0">` to the closing `{/if}` of the `ctxMenu` block). Paste it immediately after the `</script>` tag in `LibraryDesktop.svelte`.

Then apply **all substitutions from the table in Step 2 above** — replace every bare controller name with its `lib.xxx` form. The local-only names (`lanOpen`, `transfersOpen`, `ctxMenu`, `openContextMenu`, `closeLanPopover`, `handleLanOutside`, `handleTransfersOutside`, element refs, `filters`, `openView`, `assetUrl`, `fmtCatalog`, `fmtRate`, `relDate`) stay bare.

Three specific rewrite lines to pay attention to (as described above):
- `bind:value={searchQuery}` → `bind:value={lib.searchQuery}`
- `onclick={() => (filter = f.id)}` → `onclick={() => (lib.filter = f.id)}`
- `onclick={() => (selectedId = g.id)}` → `onclick={() => (lib.selectedId = g.id)}`

- [ ] **Step 3: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run check && bun run lint
```
Expected: both PASS with 0 errors. If svelte-check reports "selectedId is read-only" or similar, check that the controller exposes `set selectedId(v)` in its return object (it does).

- [ ] **Step 4: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/lib/components/LibraryDesktop.svelte
git commit -m "feat(touch): add LibraryDesktop.svelte (desktop two-pane view)

Moves the library markup + UI-local state (lanOpen, transfersOpen,
ctxMenu, element refs, click-outside handlers) into a dedicated view
component. Consumes lib: Library for all data and actions. Desktop
renders identically; +page.svelte thin shell wired in next task.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Replace `+page.svelte` with thin shell

**Files:**
- Modify: `tauri/src/routes/+page.svelte` (replace entire file)

- [ ] **Step 1: Replace the file**

Replace the entire contents of `tauri/src/routes/+page.svelte` with:

```svelte
<script lang="ts">
  import { createLibrary } from '$lib/library.svelte';
  import LibraryDesktop from '$lib/components/LibraryDesktop.svelte';

  const lib = createLibrary();
</script>

<LibraryDesktop {lib} />
```

- [ ] **Step 2: Verify**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri
bun run check && bun run lint && bun run test
```
Expected: all pass. The `bun run check` runs svelte-check across all 4048+ files including the new `LibraryDesktop.svelte` and the slimmed `+page.svelte`.

- [ ] **Step 3: Visual smoke test (manual)**

```bash
bun run tauri dev
```
The library window must look and behave **identically** to before this PR: same sidebar, same game detail, same LAN popover, same transfers panel, same context menu, same sync status icon. Search, filter tabs, and game selection must all work. Close the dev app when confirmed.

- [ ] **Step 4: Commit**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap
git add tauri/src/routes/+page.svelte
git commit -m "refactor(touch): slim +page.svelte to thin shell

createLibrary() + <LibraryDesktop {lib} />. All logic is now in the
controller; the view is a pure function of lib's reactive state.
Desktop behavior unchanged; LibraryTouch plugs in next PR.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Full verification + guardrail self-check

**Files:** none (verification only).

- [ ] **Step 1: Full check suite**

```bash
cd C:\Users\akinz\Git\ludusavi-wrap\tauri\src-tauri && cargo check && cargo clippy && cargo test
cd C:\Users\akinz\Git\ludusavi-wrap\tauri && bun run check && bun run lint && bun run test
```
Expected: all PASS. The test count should be higher than before (new `filterGames` tests).

- [ ] **Step 2: Confirm `+page.svelte` is truly thin**

```bash
wc -l tauri/src/routes/+page.svelte
```
Expected: 7 lines (the 6-line svelte file + trailing newline).

- [ ] **Step 3: Confirm no duplicate state — no `$state` in `+page.svelte`**

```bash
git grep "\$state\|\$derived" -- tauri/src/routes/+page.svelte
```
Expected: no matches.

- [ ] **Step 4: Guardrail compliance (handoff §6a)**

- `createLibrary()` is the one controller, `LibraryDesktop` is the view-over-controller. ✅
- `LibraryDesktop` does not duplicate any business logic from the controller. ✅
- `LibraryTouch` (PR 5) will consume the same `lib: Library` prop without any controller changes. ✅
- No `*Touch.svelte` files created in this PR. ✅

- [ ] **Step 5: Finish the branch**

Announce: "I'm using the finishing-a-development-branch skill." Then follow **superpowers:finishing-a-development-branch**. This branch is stacked on `touch-mode-pr1-density-tokens`. Do not push without explicit confirmation.

---

## Self-review notes

**Spec coverage (handoff §6a / TASKS.md PR 4):**
- `lib/library.svelte.ts` with `createLibrary()` — all `onMount` fetch + `listen(...)` subscriptions + action methods ✅ (Task 1)
- `LibraryDesktop.svelte` consuming the controller via a `lib` prop ✅ (Task 3)
- `+page.svelte` thin shell ✅ (Task 4)
- Vitest tests for controller filtering ✅ (Task 2 — `filterGames` is the testable extraction the handoff calls for)
- Verify desktop unchanged ✅ (Task 4 Step 3 manual check)

**Placeholder scan:** All code blocks are complete. The "apply substitutions from the table" instruction in Task 3 Step 2 is a precise specification, not a placeholder — every name is listed with its exact new form.

**Type/name consistency:** `Library = ReturnType<typeof createLibrary>` in Task 1; `lib: Library` in Task 3 — consistent. `filterGames(games, filter, searchQuery)` defined in Task 1; tests call `filterGames(GAMES, 'all', '')` in Task 2 — consistent. `lib.clearPeerView()` called in `closeLanPopover` in Task 3 Step 1; `clearPeerView` method defined and returned in Task 1 — consistent. `lib.filter = f.id` uses the `set filter(v)` setter defined in Task 1 — consistent.

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
        if (message) {
          // A message on `done` means local backup succeeded but cloud upload failed.
          toasts.show({
            kind: 'warn',
            label: 'LUDUSAVI',
            title: 'Cloud upload failed',
            sub: game ? `${game.game_name} · ${message}` : message,
            catalog: game ? fmtCatalog(game.catalog_number) : undefined,
          });
        } else if (game) {
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

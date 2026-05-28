<script lang="ts">
  /**
   * Library — two-pane layout (sidebar list + selected-game detail).
   *
   * Sidebar:
   *   - search input
   *   - filter tabs: All / Recent / Played
   *   - scrollable list of small entries (cover + name + catalog/last-played)
   *   - footer with the primary "Add a game" button
   *
   * Detail pane:
   *   - hero, stats, action toolbar, three info cards
   *   - rendered by `GameDetail.svelte`
   *
   * Listens to `library:changed` so adds/removes from the Add Game popup
   * (or anywhere else) refresh the list automatically. If the selected
   * entry vanishes, falls back to the first remaining game.
   */
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
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { listen } from '@tauri-apps/api/event';
  import { api, assetUrl } from '$lib/api';
  import { fmtCatalog, fmtRate, relDate } from '$lib/format';
  import { toasts } from '$lib/toasts.svelte';
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
  import { checkForUpdateOnStartup } from '$lib/updater';
  import WindowChrome from '$lib/components/WindowChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import GameDetail from '$lib/components/GameDetail.svelte';
  import LibraryContextMenu from '$lib/components/LibraryContextMenu.svelte';
  import TransferPill from '$lib/components/TransferPill.svelte';
  import TransfersPanel from '$lib/components/TransfersPanel.svelte';

  let games = $state<GameEntry[]>([]);
  // Snapshot of app config — only used to decide whether to show the
  // Browse Games chrome button (gated on download_sources.length > 0).
  let config = $state<ConfigData | null>(null);
  let loaded = $state(false);
  let error = $state<string | null>(null);

  let selectedId = $state<string | null>(null);
  let searchQuery = $state('');
  let filter = $state<'all' | 'recent' | 'played'>('all');

  // Currently-running game. Updated by `run:phase` events from the backend.
  // GameDetail uses these to drive the Play button label.
  let runningId = $state<string | null>(null);
  let runningPhase = $state<RunPhase | null>(null);

  // Sidebar right-click context menu — open at {x, y} for {game} or null.
  let ctxMenu = $state<{ game: GameEntry; x: number; y: number } | null>(null);

  // LAN peers — backend pushes via `lan:peers-changed`; we re-fetch and
  // surface a count badge + popover on click.
  let lanPeers = $state<LanPeer[]>([]);
  let lanOpen = $state(false);
  let lanWifiBtn: HTMLButtonElement | undefined = $state();
  let lanPopoverEl: HTMLDivElement | undefined = $state();
  // When the user clicks into a peer in the popover we swap to a drilled
  // view showing that peer's `/games` payload. Null = peer-list view.
  let openPeer = $state<LanPeer | null>(null);
  let peerGames = $state<PeerGame[]>([]);
  let peerGamesLoading = $state(false);
  let peerGamesError = $state<string | null>(null);
  // In-flight LAN install. The backend serialises to one at a time;
  // the UI tracks the latest event payload + uses it to drive the
  // Install button states inside the peer view.
  let activeDownload = $state<DownloadProgress | null>(null);
  // ID of the game whose `start_peer_install` call is currently
  // awaiting the manifest. The backend can sit on this for tens of
  // seconds while it blake3-hashes a large game folder on first
  // request; we surface that as a spinner on the Install button so
  // the click feels responsive.
  let startingGameId = $state<string | null>(null);
  // Install tokens we've already shown a terminal toast for. Lives
  // outside `$state` because changing it never affects rendering —
  // it's just a dedup set for the `lan:download` event handler.
  const toastedDownloadTokens = new Set<string>();
  // Active uploads — peers currently downloading FROM us. The host
  // side of LAN sharing.
  let activeUploads = $state<UploadSnapshot[]>([]);

  // ── Sync server status (chrome cloud icon) ──────────────────────
  // Polled in the backend every 30s; we listen to status-change
  // events to update the icon tint + tooltip.
  let syncStatus = $state<SyncStatus>({
    reachability: 'unconfigured',
    server_version: null,
    error: null,
    last_ok_ago_secs: null,
  });
  const syncOk = $derived(syncStatus.reachability === 'online');
  const syncOff = $derived(syncStatus.reachability === 'offline');
  const syncTitle = $derived(
    syncStatus.reachability === 'unconfigured'
      ? 'Sync server not configured — open Settings to set it up'
      : syncOk
        ? `Sync server online${syncStatus.server_version ? ` · v${syncStatus.server_version}` : ''}`
        : `Sync server unreachable${syncStatus.error ? ` · ${syncStatus.error}` : ''}`,
  );

  // ── Transfers central hub (per the redesign) ─────────────────────
  // Title-bar pill + slide-out panel. Houses BOTH directions —
  // incoming installs and outgoing peer transfers — in one place,
  // mirroring Steam's downloads tray. The LAN peers popover stays
  // for peer discovery only.
  let transfersOpen = $state(false);
  let transferPillEl: HTMLSpanElement | undefined = $state();
  let transfersPanelEl: HTMLDivElement | undefined = $state();

  // Derived aggregates for the title-bar pill.
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
  // Uploads currently carry no byte progress — show a steady 60% as
  // a "transfer happening" hint so the pill strip isn't always
  // empty when active.
  const uploadPercent = $derived(uploadCount > 0 ? 60 : 0);

  function openContextMenu(e: MouseEvent, g: GameEntry) {
    e.preventDefault();
    ctxMenu = { game: g, x: e.clientX, y: e.clientY };
  }

  // ── Derived ────────────────────────────────────────────────────────────
  const filteredGames = $derived.by(() => {
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
  });

  const selectedGame = $derived(
    selectedId ? games.find((g) => g.id === selectedId) ?? null : null,
  );

  // ── Data fetch + event subscription ─────────────────────────────────────
  async function refresh() {
    try {
      games = await api.listGames();
      // Keep selection if still present, else pick the first game.
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

  let unlistenLibraryChanged: (() => void) | undefined;
  let unlistenRunPhase: (() => void) | undefined;
  let unlistenTrayIntro: (() => void) | undefined;
  let unlistenLanPeers: (() => void) | undefined;
  let unlistenLanDownload: (() => void) | undefined;
  let unlistenLanUploads: (() => void) | undefined;
  let unlistenSyncStatus: (() => void) | undefined;

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

  function handleLanOutside(e: MouseEvent) {
    if (!lanOpen) return;
    if (lanPopoverEl?.contains(e.target as Node)) return;
    if (lanWifiBtn?.contains(e.target as Node)) return;
    closeLanPopover();
  }

  function closeLanPopover() {
    lanOpen = false;
    openPeer = null;
    peerGames = [];
    peerGamesError = null;
  }

  function handleTransfersOutside(e: MouseEvent) {
    if (!transfersOpen) return;
    if (transfersPanelEl?.contains(e.target as Node)) return;
    if (transferPillEl?.contains(e.target as Node)) return;
    transfersOpen = false;
  }

  /** Drill into a peer — fetch their /games and swap the popover. */
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

  /** Opens (or focuses) the Settings child window. */
  async function openSettingsWindow() {
    const existing = await WebviewWindow.getByLabel('settings');
    if (existing) {
      await existing.setFocus();
      return;
    }
    new WebviewWindow('settings', {
      url: '/settings',
      title: 'Spool — Settings',
      width: 1180,
      height: 760,
      minWidth: 900,
      minHeight: 600,
      decorations: false,
      resizable: true,
      center: true,
      backgroundColor: '#0b0c0e',
    });
  }

  /** Opens (or focuses) the Browse Games child window. */
  async function openBrowseWindow() {
    const existing = await WebviewWindow.getByLabel('browse-games');
    if (existing) {
      await existing.setFocus();
      return;
    }
    new WebviewWindow('browse-games', {
      url: '/browse',
      title: 'Browse Games',
      width: 1280,
      height: 800,
      minWidth: 1100,
      minHeight: 600,
      decorations: false,
      resizable: true,
      center: true,
      backgroundColor: '#0b0c0e',
    });
  }

  /** Asks the backend to cancel the active install. The download task
   *  cleans up its `.partial` dir before emitting the final
   *  `status: "canceled"` event, which we surface as a toast.  */
  async function cancelActiveInstall() {
    if (!activeDownload) return;
    try {
      await api.cancelPeerInstall(activeDownload.install_token);
    } catch (e) {
      console.error('[lan] cancel install failed:', e);
    }
  }

  /** Kicks off a LAN install for the given peer + game. */
  async function installFromPeer(peer: LanPeer, game: PeerGame) {
    if (activeDownload && activeDownload.status !== 'done' && activeDownload.status !== 'error') {
      // Backend would reject too, but a friendly toast saves the round-trip.
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

  onMount(() => {
    refresh();
    api
      .getConfig()
      .then((c) => (config = c))
      .catch((e) => console.error('[library] getConfig failed:', e));
    // Run the updater check a moment after mount so the library
    // refresh has settled. Fire-and-forget — failures are surfaced
    // via the toast system; nothing here blocks the UI.
    setTimeout(() => {
      checkForUpdateOnStartup().catch((e) =>
        console.error('[updater] startup check failed:', e),
      );
    }, 2000);
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
        // Listener is registered — safe to pick up a cold-start --run
        // queue without losing the resulting run:phase events.
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
    // First-time close-to-tray explainer. Backend fires this exactly
    // once (on the first hide; idempotent via config.tray_intro_seen).
    // We make it sticky so it survives the window being hidden — when
    // the user next reopens, they see it in the toast stack.
    listen<null>('tray:first-hide', () => {
      toasts.show({
        kind: 'info',
        label: 'TRAY',
        title: 'Spool is still running',
        sub: "Click the tray icon to bring the window back. You can quit fully from the tray menu.",
        duration: 0,
      });
    })
      .then((fn) => (unlistenTrayIntro = fn))
      .catch((e) => console.error('[tray] intro listener failed:', e));

    // LAN peer registry — initial fetch + event-driven refresh.
    refreshLanPeers();
    listen<null>('lan:peers-changed', () => refreshLanPeers())
      .then((fn) => (unlistenLanPeers = fn))
      .catch((e) => console.error('[lan] peers listener failed:', e));

    // LAN download progress — pick up any in-flight install on mount,
    // then track live via events. Terminal states drive a toast.
    api
      .currentPeerDownload()
      .then((p) => {
        if (p) activeDownload = p;
      })
      .catch((e) => console.error('[lan] currentPeerDownload failed:', e));
    // Sync server status — initial fetch then event-driven updates.
    api
      .currentSyncStatus()
      .then((s) => (syncStatus = s))
      .catch((e) => console.error('[sync] currentSyncStatus failed:', e));
    listen<SyncStatus>('sync:status-changed', (event) => {
      syncStatus = event.payload;
    })
      .then((fn) => (unlistenSyncStatus = fn))
      .catch((e) => console.error('[sync] status listener failed:', e));

    // Active uploads (the host side of LAN sharing).
    refreshActiveUploads();
    listen<null>('lan:uploads-changed', () => {
      // No auto-switch any more — the transfers pill in the chrome
      // shows a live count + progress strip and the user can open the
      // panel themselves. Auto-popping a popover felt intrusive.
      refreshActiveUploads();
    })
      .then((fn) => (unlistenLanUploads = fn))
      .catch((e) => console.error('[lan] uploads listener failed:', e));

    listen<DownloadProgress>('lan:download', (event) => {
      const p = event.payload;
      // Toast on the first terminal event for a given install token.
      // Track which tokens we've already toasted so a hot-reload
      // duplicate or a stray redelivery doesn't fire twice. The
      // previous check compared the new token against the last token
      // observed in `activeDownload` — but by the time `done` arrived,
      // earlier events had already set `activeDownload` to that same
      // token, so the toast never fired in the normal flow.
      const isTerminal =
        p.status === 'done' || p.status === 'error' || p.status === 'canceled';
      const firstTerminal = isTerminal && !toastedDownloadTokens.has(p.install_token);
      if (firstTerminal) toastedDownloadTokens.add(p.install_token);
      activeDownload = p;
      if (p.status === 'done' && firstTerminal) {
        toasts.show({
          kind: 'ok',
          label: 'LAN',
          title: 'Install complete',
          sub: `${p.game_name} · from ${p.source_device_name}`,
        });
      } else if (p.status === 'error' && firstTerminal) {
        toasts.show({
          kind: 'bad',
          label: 'LAN',
          title: 'Install failed',
          sub: p.message ?? `${p.game_name} could not be installed`,
        });
      } else if (p.status === 'canceled' && firstTerminal) {
        toasts.show({
          kind: 'info',
          label: 'LAN',
          title: 'Install cancelled',
          sub: `${p.game_name} · partial files cleaned up`,
        });
      }
    })
      .then((fn) => (unlistenLanDownload = fn))
      .catch((e) => console.error('[lan] download listener failed:', e));

    document.addEventListener('mousedown', handleLanOutside, true);
    document.addEventListener('mousedown', handleTransfersOutside, true);

    return () => {
      unlistenLibraryChanged?.();
      unlistenRunPhase?.();
      unlistenTrayIntro?.();
      unlistenLanPeers?.();
      unlistenLanDownload?.();
      unlistenLanUploads?.();
      unlistenSyncStatus?.();
      document.removeEventListener('mousedown', handleLanOutside, true);
      document.removeEventListener('mousedown', handleTransfersOutside, true);
    };
  });

  /**
   * Shows a toast for a Run workflow error. Detects known patterns
   * (cloud-conflict) and attaches a smarter CTA. Falls back to a plain
   * error toast for everything else.
   */
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
      title: 'Couldn’t launch game',
      sub: message,
    });
  }

  // ── Add Game popup ─────────────────────────────────────────────────────
  function openAddGame() {
    WebviewWindow.getByLabel('add-game').then((win) => {
      if (win) {
        win.setFocus();
        return;
      }
      new WebviewWindow('add-game', {
        url: '/add',
        title: 'Add Game · Spool',
        width: 720,
        height: 560,
        minWidth: 600,
        minHeight: 480,
        decorations: false,
        resizable: true,
        center: true,
        backgroundColor: '#0b0c0e',
      });
    });
  }

  const filters: { id: typeof filter; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'recent', label: 'Recent' },
    { id: 'played', label: 'Played' },
  ];
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0">
  <WindowChrome sub="LIBRARY">
    <div class="flex h-full items-center justify-end gap-1.5 pr-2">
      <!-- Browse Games: opens the Hydra source aggregator as a child
           window. Hidden when no feeds are configured so users
           without TorBox/Hydra setup don't see a dead button. -->
      {#if config && config.download_sources.length > 0}
        <button
          type="button"
          onclick={openBrowseWindow}
          title="Browse games · {config.download_sources.length} feeds"
          aria-label="Browse games"
          class="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
          data-tauri-drag-region="false"
        >
          <BookOpen size={14} />
        </button>
      {/if}
      <!-- Transfers pill — central hub for both downloads and uploads -->
      <span bind:this={transferPillEl} class="inline-flex">
        <TransferPill
          {downloadCount}
          {downloadPercent}
          {uploadCount}
          {uploadPercent}
          open={transfersOpen}
          onclick={() => (transfersOpen = !transfersOpen)}
        />
      </span>
      <button
          bind:this={lanWifiBtn}
          type="button"
          onclick={() => (lanOpen ? closeLanPopover() : (lanOpen = true))}
          aria-label={`${lanPeers.length} LAN peer${lanPeers.length === 1 ? '' : 's'}`}
          title={`${lanPeers.length} LAN peer${lanPeers.length === 1 ? '' : 's'}`}
          class="relative inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
          data-tauri-drag-region="false"
        >
          <Wifi size={14} />
          {#if lanPeers.length > 0}
            <span
              class="font-mono absolute -right-px -top-px inline-flex h-3 min-w-3 items-center justify-center rounded-full px-1 text-[8px] font-bold text-bg-0"
              style:background="var(--color-spool)"
            >
              {lanPeers.length}
            </span>
          {/if}
        </button>
        <!-- Sync server status — cloud icon, tinted by reachability.
             Clicking opens Settings → Sync Server. -->
        <button
          onclick={openSettingsWindow}
          aria-label="Sync server status"
          title={syncTitle}
          class="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-sm border-none bg-transparent transition-colors hover:bg-white/10"
          style:color={syncOk
            ? 'var(--color-ok)'
            : syncOff
              ? 'var(--color-bad)'
              : 'var(--color-ink-3)'}
          data-tauri-drag-region="false"
        >
          {#if syncOff}
            <CloudOff size={14} />
          {:else}
            <Cloud size={14} />
          {/if}
        </button>
        <button
          onclick={openSettingsWindow}
          aria-label="Settings"
          class="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
          data-tauri-drag-region="false"
        >
          <Settings size={14} />
        </button>
    </div>
  </WindowChrome>

  {#if transfersOpen}
    <div
      bind:this={transfersPanelEl}
      class="fixed top-[44px] z-40"
      style:right="92px"
    >
      <TransfersPanel
        download={activeDownload}
        uploads={activeUploads}
        onCancelDownload={cancelActiveInstall}
        onCancelUpload={(u) => kickUpload(u)}
        coverFor={(id) => {
          const g = games.find((g) => g.id === id);
          return assetUrl(g?.cover_image_path);
        }}
      />
    </div>
  {/if}

  {#if lanOpen}
    <div
      bind:this={lanPopoverEl}
      role="dialog"
      class="fixed right-3 top-[44px] z-40 w-[320px] overflow-hidden rounded-md border border-line-2 bg-bg-1"
      style:box-shadow="0 18px 48px rgb(0 0 0 / 0.6)"
    >
      {#if openPeer}
        <!-- Drilled view: one peer's library -->
        <header class="flex items-center gap-2 border-b border-line-1 px-2.5 py-2">
          <button
            type="button"
            onclick={backToPeerList}
            class="flex h-6 w-6 items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-bg-2 hover:text-ink-0"
            aria-label="Back to LAN peers"
            title="Back"
          >
            <ArrowLeft size={13} />
          </button>
          <div class="min-w-0 flex-1">
            <div class="truncate text-[12.5px] text-ink-0" title={openPeer.device_name}>
              {openPeer.device_name}
            </div>
            <div class="font-mono mt-0.5 text-[10px] text-ink-3 tracking-[0.04em]">
              {openPeer.addr}:{openPeer.file_server_port}
            </div>
          </div>
        </header>
        {#if peerGamesLoading}
          <div class="flex items-center justify-center gap-2 px-3.5 py-6 text-[12px] text-ink-3">
            <Loader2 size={14} class="animate-[spool-spin_1s_linear_infinite]" />
            Loading library…
          </div>
        {:else if peerGamesError}
          <div class="px-3.5 py-4 text-[11.5px] text-ink-2">
            <div class="font-medium text-ink-1">Couldn't reach peer</div>
            <div class="mt-1 text-[11px] text-ink-3">{peerGamesError}</div>
          </div>
        {:else if peerGames.length === 0}
          <div class="px-3.5 py-4 text-center text-[12px] text-ink-3">
            Peer isn't sharing any games.
          </div>
        {:else}
          <ul class="max-h-[360px] overflow-y-auto py-1">
            {#each peerGames as game (game.id)}
              {@const dl =
                activeDownload &&
                activeDownload.source_game_id === game.id &&
                openPeer &&
                activeDownload.source_device_id === openPeer.device_id
                  ? activeDownload
                  : null}
              {@const inflight =
                dl && (dl.status === 'starting' || dl.status === 'transferring')}
              {@const starting = startingGameId === game.id}
              {@const busy =
                !!startingGameId ||
                (activeDownload &&
                  (activeDownload.status === 'starting' ||
                    activeDownload.status === 'transferring'))}
              <li class="flex items-start gap-2.5 px-3.5 py-2">
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[12.5px] text-ink-0" title={game.game_name}>
                    {game.game_name}
                  </div>
                  <div class="font-mono mt-0.5 flex gap-2 text-[10px] text-ink-3 tracking-[0.04em]">
                    <span>{fmtCatalog(game.catalog_number)}</span>
                    {#if game.developer}
                      <span>·</span>
                      <span class="truncate">{game.developer}</span>
                    {/if}
                    {#if game.install_size_mb > 0}
                      <span>·</span>
                      <span>
                        {game.install_size_mb >= 1024
                          ? (game.install_size_mb / 1024).toFixed(1) + ' GB'
                          : game.install_size_mb.toFixed(0) + ' MB'}
                      </span>
                    {/if}
                  </div>
                  {#if inflight && dl}
                    <div class="mt-1.5">
                      <div class="h-1 w-full overflow-hidden rounded-full bg-bg-2">
                        <div
                          class="h-full transition-[width] duration-150 ease-out"
                          style:width={dl.bytes_total > 0
                            ? Math.min(100, (dl.bytes_done / dl.bytes_total) * 100) + '%'
                            : '0%'}
                          style:background="var(--color-spool)"
                        ></div>
                      </div>
                      <div
                        class="font-mono mt-1 flex justify-between gap-2 text-[9.5px] text-ink-3 tracking-[0.04em]"
                      >
                        <span class="truncate" title={dl.current_file}>
                          {dl.current_file || '…'}
                        </span>
                        <span class="shrink-0 whitespace-nowrap">
                          {fmtRate(dl.bytes_per_second)}
                          {#if dl.bytes_total > 0}
                            · {Math.round((dl.bytes_done / dl.bytes_total) * 100)}%
                          {/if}
                        </span>
                      </div>
                    </div>
                  {/if}
                </div>
                {#if !inflight}
                  <button
                    type="button"
                    onclick={() => openPeer && installFromPeer(openPeer, game)}
                    disabled={!game.shareable || !!busy}
                    title={!game.shareable
                      ? 'Source peer has no install folder configured'
                      : starting
                        ? 'Fetching manifest from peer…'
                        : busy
                          ? 'Another install is in progress'
                          : 'Install on this device'}
                    class="inline-flex h-7 shrink-0 items-center gap-1 rounded-sm border border-line-2 bg-bg-2 px-2 text-[11px] text-ink-1 transition-colors enabled:hover:border-line-3 enabled:hover:text-ink-0 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {#if starting}
                      <Loader2 size={11} class="animate-[spool-spin_1s_linear_infinite]" />
                      Starting…
                    {:else}
                      <Download size={11} />
                      Install
                    {/if}
                  </button>
                {:else if dl}
                  <button
                    type="button"
                    onclick={cancelActiveInstall}
                    aria-label="Cancel install"
                    title="Cancel install"
                    class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-2 transition-colors hover:border-bad/60 hover:text-bad"
                  >
                    <X size={13} />
                  </button>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        <div class="border-t border-dashed border-line-1 px-3.5 py-2 text-[10.5px] text-ink-3">
          Installs land in <code class="text-ink-2">lan-games/</code> by default.
        </div>
      {:else}
        <!-- Peer list (uploads moved to the Transfers panel) -->
        <header class="flex items-center justify-between border-b border-line-1 px-3.5 py-2">
          <MonoLabel size={10}>LAN peers</MonoLabel>
          <span class="font-mono text-[10px] text-ink-3">
            {lanPeers.length}
          </span>
        </header>
        {#if lanPeers.length === 0}
          <div class="px-3.5 py-4 text-center text-[12px] text-ink-3">
            Nobody else on the LAN.
          </div>
        {:else}
          <ul class="max-h-[320px] overflow-y-auto py-1">
            {#each lanPeers as peer (peer.device_id)}
              {@const browsable = peer.file_server_port !== 0}
              <li>
                <button
                  type="button"
                  onclick={() => openPeerView(peer)}
                  disabled={!browsable}
                  class="flex w-full items-start gap-2.5 px-3.5 py-2 text-left transition-colors enabled:hover:bg-bg-2 disabled:cursor-not-allowed"
                >
                  <Wifi
                    size={12}
                    class={'mt-0.5 shrink-0 ' + (browsable ? 'text-ink-2' : 'text-ink-3')}
                  />
                  <div class="min-w-0 flex-1">
                    <div class="truncate text-[12.5px] text-ink-0" title={peer.device_name}>
                      {peer.device_name}
                    </div>
                    <div class="font-mono mt-0.5 flex gap-2 text-[10px] text-ink-3 tracking-[0.04em]">
                      <span>{peer.addr}</span>
                      <span>·</span>
                      <span>{peer.game_count} game{peer.game_count === 1 ? '' : 's'}</span>
                      <span>·</span>
                      <span>{peer.last_seen_ago_secs}s ago</span>
                    </div>
                    {#if !browsable}
                      <div class="font-mono mt-0.5 text-[9.5px] text-ink-3 tracking-[0.04em]">
                        DISCOVERY ONLY
                      </div>
                    {/if}
                  </div>
                  {#if browsable}
                    <ChevronRight size={13} class="mt-0.5 shrink-0 text-ink-3" />
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
          <div class="border-t border-dashed border-line-1 px-3.5 py-2 text-[10.5px] text-ink-3">
            Click a peer to browse and install games from their library.
          </div>
        {/if}
      {/if}
    </div>
  {/if}

  <div class="grid min-h-0 flex-1" style:grid-template-columns="320px 1fr">
    <!-- ── Sidebar ────────────────────────────────────────────────── -->
    <aside class="flex min-h-0 flex-col border-r border-line-1 bg-bg-1">
      <!-- Search + filters -->
      <div class="flex flex-col gap-2.5 px-3 py-3">
        <div
          class="flex h-[30px] items-center gap-2 rounded-sm border border-line-1 bg-bg-2 px-2.5"
        >
          <Search size={14} class="text-ink-2" />
          <input
            bind:value={searchQuery}
            placeholder={`Search ${games.length || 0} games`}
            class="font-sans min-w-0 flex-1 bg-transparent text-[12.5px] text-ink-0 outline-none placeholder:text-ink-3"
          />
        </div>
        <div class="flex gap-1">
          {#each filters as f (f.id)}
            {@const active = filter === f.id}
            <button
              type="button"
              onclick={() => (filter = f.id)}
              class="inline-flex items-center gap-1.5 rounded-sm border px-2 py-1 text-[11.5px] font-medium transition-colors"
              style:background={active ? 'var(--color-bg-3)' : 'transparent'}
              style:border-color={active ? 'var(--color-line-2)' : 'transparent'}
              style:color={active ? 'var(--color-ink-0)' : 'var(--color-ink-2)'}
            >
              {f.label}
              <span
                class="font-mono text-[9.5px]"
                style:color={active ? 'var(--color-ink-2)' : 'var(--color-ink-3)'}
              >
                {f.id === 'all'
                  ? games.length
                  : f.id === 'recent'
                    ? games.filter((g) => g.last_played_at || g.added_at).length
                    : games.filter((g) => g.playtime_minutes > 0).length}
              </span>
            </button>
          {/each}
        </div>
      </div>

      <!-- Section header -->
      <div class="flex items-center justify-between px-3.5 pb-1.5 pt-2.5">
        <MonoLabel size={9.5}>
          {filter === 'recent' ? 'By last activity' : 'By catalog'}
        </MonoLabel>
        <span class="text-[11px] text-ink-3">{filteredGames.length}</span>
      </div>

      <!-- List -->
      <div class="min-h-0 flex-1 overflow-y-auto pb-2">
        {#if !loaded}
          <p
            class="font-mono px-4 py-3 text-[10px] uppercase tracking-[0.12em] text-ink-3"
          >
            Loading…
          </p>
        {:else if error}
          <p class="px-4 py-3 text-[12px] text-bad">{error}</p>
        {:else if filteredGames.length === 0 && games.length === 0}
          <div class="flex flex-col items-center gap-2 px-4 py-10 text-center">
            <MonoLabel>Empty shelf</MonoLabel>
            <p class="text-[12px] text-ink-2">No games yet.</p>
          </div>
        {:else if filteredGames.length === 0}
          <p class="px-4 py-3 text-[12px] text-ink-3">No matches.</p>
        {:else}
          {#each filteredGames as g (g.id)}
            {@const selected = selectedId === g.id}
            {@const cover = assetUrl(g.cover_image_path)}
            {@const rowAccent = g.accent_color ?? '#d7c9a0'}
            {@const badgeColor =
              g.sync_badge === 'synced'
                ? 'var(--color-ok)'
                : g.sync_badge === 'cloud-newer'
                  ? 'var(--color-info)'
                  : g.sync_badge === 'local-newer'
                    ? 'var(--color-warn)'
                    : null}
            {@const badgeTitle =
              g.sync_badge === 'synced'
                ? 'Saves synced'
                : g.sync_badge === 'cloud-newer'
                  ? 'Cloud has newer saves — restore on launch'
                  : g.sync_badge === 'local-newer'
                    ? 'Local saves newer than cloud — backup pending'
                    : ''}
            <button
              type="button"
              onclick={() => (selectedId = g.id)}
              oncontextmenu={(e) => openContextMenu(e, g)}
              class="flex w-full items-center gap-2.5 border-l-2 px-3 py-2 text-left transition-colors"
              style:background={selected
                ? `color-mix(in srgb, ${rowAccent} 12%, transparent)`
                : 'transparent'}
              style:border-left-color={selected ? rowAccent : 'transparent'}
            >
              <div
                class="relative h-11 w-8 shrink-0 overflow-hidden rounded-sm border border-line-1 bg-bg-2"
              >
                {#if cover}
                  <img
                    src={cover}
                    alt={g.game_name}
                    class="h-full w-full object-cover"
                  />
                {:else}
                  <div
                    class="flex h-full w-full items-center justify-center"
                    style:background="linear-gradient(160deg, #2a2622 0%, #0a0807 100%)"
                  >
                    <span
                      class="font-mono text-[7px] uppercase tracking-[0.1em] text-ink-3"
                    >
                      {g.game_name.slice(0, 1)}
                    </span>
                  </div>
                {/if}
                {#if badgeColor}
                  <!-- Sync status dot: top-right of the cover thumb.
                       The 1px ink-0 border keeps it readable against
                       any cover artwork. -->
                  <span
                    class="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full border"
                    style:background={badgeColor}
                    style:border-color="var(--color-bg-0)"
                    title={badgeTitle}
                  ></span>
                {/if}
              </div>
              <div class="min-w-0 flex-1">
                <div
                  class="truncate text-[12.5px] font-medium text-ink-0"
                  title={g.game_name}
                >
                  {g.game_name}
                </div>
                <div
                  class="font-mono mt-0.5 flex items-center gap-1.5 text-[9.5px] tracking-[0.06em] text-ink-3"
                >
                  <span>{fmtCatalog(g.catalog_number)}</span>
                  <span>·</span>
                  <span>{g.last_played_at ? relDate(g.last_played_at) : 'unplayed'}</span>
                </div>
              </div>
            </button>
          {/each}
        {/if}
      </div>

      <!-- Footer -->
      <div class="border-t border-line-1 bg-bg-0 px-3 py-2.5">
        <button
          type="button"
          onclick={openAddGame}
          class="inline-flex h-8 w-full cursor-pointer items-center justify-center gap-1.5 rounded-sm bg-spool px-3 text-[12.5px] font-medium text-bg-0 transition-colors hover:brightness-95"
        >
          <Plus size={14} />
          Add a game
        </button>
      </div>
    </aside>

    <!-- ── Detail pane ──────────────────────────────────────────────── -->
    {#if selectedGame}
      <GameDetail
        game={selectedGame}
        runPhase={runningId === selectedGame.id ? runningPhase : null}
      />
    {:else if loaded && games.length === 0}
      <div class="flex flex-col items-center justify-center gap-3 text-center">
        <MonoLabel>Empty library</MonoLabel>
        <p class="max-w-md text-sm text-ink-2">
          No games yet. Add an executable to start your collection.
        </p>
        <button
          type="button"
          onclick={openAddGame}
          class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-sm bg-spool px-3 text-[12.5px] font-medium text-bg-0 transition-colors hover:brightness-95"
        >
          <Plus size={14} />
          Add your first game
        </button>
      </div>
    {:else}
      <div class="flex items-center justify-center">
        <MonoLabel>Pick a game from the sidebar</MonoLabel>
      </div>
    {/if}
  </div>
</div>

{#if ctxMenu}
  <LibraryContextMenu
    game={ctxMenu.game}
    x={ctxMenu.x}
    y={ctxMenu.y}
    onclose={() => (ctxMenu = null)}
  />
{/if}

<script lang="ts">
  import { onMount } from 'svelte';
  import {
    ArrowLeft,
    ChevronRight,
    Cloud,
    CloudOff,
    Download,
    Loader2,
    Package,
    Plus,
    Search,
    Settings,
    Wifi,
    X,
  } from '@lucide/svelte';
  import { openView } from '$lib/nav';
  import { api, assetUrl } from '$lib/api';
  import { fmtCatalog, fmtRate, relDate } from '$lib/format';
  import type { GameEntry } from '$lib/types';
  import type { Library } from '$lib/library.svelte';
  import AppChrome from '$lib/components/AppChrome.svelte';
  import MonoLabel from '$lib/components/MonoLabel.svelte';
  import GameDetail from '$lib/components/GameDetail.svelte';
  import LibraryContextMenu from '$lib/components/LibraryContextMenu.svelte';
  import TransferPill from '$lib/components/TransferPill.svelte';
  import TransfersPanel from '$lib/components/TransfersPanel.svelte';
  import { gamepadScope } from '$lib/gamepad';

  let { lib }: { lib: Library } = $props();

  // UI-only state (not in controller)
  // The guided repack installer runs through Proton, so the button is Linux-only.
  let isLinux = $state(false);
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

  // Controller B / Escape at the library root. The LAN popover and transfers
  // panel push their own scopes (so B closes them first); this only needs to
  // dismiss the context menu, which isn't a scope of its own.
  function libraryBack() {
    if (ctxMenu) ctxMenu = null;
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
    api.appPlatform().then((p) => (isLinux = p === 'linux'));
  });

  onMount(() => {
    document.addEventListener('mousedown', handleLanOutside, true);
    document.addEventListener('mousedown', handleTransfersOutside, true);
    return () => {
      document.removeEventListener('mousedown', handleLanOutside, true);
      document.removeEventListener('mousedown', handleTransfersOutside, true);
    };
  });
</script>

<div class="flex h-screen flex-col bg-bg-0 text-ink-0" use:gamepadScope={{ onBack: libraryBack }}>
  <AppChrome
    sub="LIBRARY"
    peers={lib.lanPeers.length}
    transfers={lib.activeDownload?.status === 'starting' || lib.activeDownload?.status === 'transferring' ? 1 : 0}
    conflict={lib.syncOff}
  >
    <div class="flex h-full items-center justify-end gap-1.5 pr-2">
      <!-- Transfers pill — central hub for both downloads and uploads -->
      <span bind:this={transferPillEl} class="inline-flex">
        <TransferPill
          downloadCount={lib.downloadCount}
          downloadPercent={lib.downloadPercent}
          uploadCount={lib.uploadCount}
          uploadPercent={lib.uploadPercent}
          open={transfersOpen}
          onclick={() => (transfersOpen = !transfersOpen)}
        />
      </span>
      <button
          bind:this={lanWifiBtn}
          type="button"
          onclick={() => (lanOpen ? closeLanPopover() : (lanOpen = true))}
          aria-label={`${lib.lanPeers.length} LAN peer${lib.lanPeers.length === 1 ? '' : 's'}`}
          title={`${lib.lanPeers.length} LAN peer${lib.lanPeers.length === 1 ? '' : 's'}`}
          class="relative inline-flex cursor-pointer items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
          style:height="var(--control-h-icon)"
          style:width="var(--control-h-icon)"
          data-tauri-drag-region="false"
        >
          <Wifi size={14} />
          {#if lib.lanPeers.length > 0}
            <span
              class="font-mono absolute -right-px -top-px inline-flex h-3 min-w-3 items-center justify-center rounded-full px-1 text-[8px] font-bold text-bg-0"
              style:background="var(--color-spool)"
            >
              {lib.lanPeers.length}
            </span>
          {/if}
        </button>
        <!-- Cloud remote status — cloud icon, tinted by reachability.
             Clicking opens Settings. -->
        <button
          onclick={() => openView('settings')}
          aria-label="Cloud remote status"
          title={lib.syncTitle}
          class="inline-flex cursor-pointer items-center justify-center rounded-sm border-none bg-transparent transition-colors hover:bg-white/10"
          style:height="var(--control-h-icon)"
          style:width="var(--control-h-icon)"
          style:color={lib.syncOk
            ? 'var(--color-ok)'
            : lib.syncOff
              ? 'var(--color-bad)'
              : 'var(--color-ink-3)'}
          data-tauri-drag-region="false"
        >
          {#if lib.syncOff}
            <CloudOff size={14} />
          {:else}
            <Cloud size={14} />
          {/if}
        </button>
        <button
          onclick={() => openView('settings')}
          aria-label="Settings"
          class="inline-flex cursor-pointer items-center justify-center rounded-sm border-none bg-transparent text-ink-2 transition-colors hover:bg-white/10 hover:text-ink-0"
          style:height="var(--control-h-icon)"
          style:width="var(--control-h-icon)"
          data-tauri-drag-region="false"
        >
          <Settings size={14} />
        </button>
    </div>
  </AppChrome>

  {#if transfersOpen}
    <div
      bind:this={transfersPanelEl}
      class="fixed z-40"
      style:right="92px"
      style:top="var(--chrome-h)"
      use:gamepadScope={{ onBack: () => (transfersOpen = false) }}
    >
      <TransfersPanel
        download={lib.activeDownload}
        uploads={lib.activeUploads}
        onCancelDownload={lib.cancelActiveInstall}
        onCancelUpload={(u) => lib.kickUpload(u)}
        coverFor={(id) => {
          const g = lib.games.find((g) => g.id === id);
          return assetUrl(g?.cover_image_path);
        }}
      />
    </div>
  {/if}

  {#if lanOpen}
    <div
      bind:this={lanPopoverEl}
      role="dialog"
      class="fixed right-3 z-40 w-[320px] overflow-hidden rounded-md border border-line-2 bg-bg-1"
      style:box-shadow="0 18px 48px rgb(0 0 0 / 0.6)"
      style:top="var(--chrome-h)"
      use:gamepadScope={{ onBack: closeLanPopover }}
    >
      {#if lib.openPeer}
        <!-- Drilled view: one peer's library -->
        <header class="flex items-center gap-2 border-b border-line-1 px-2.5 py-2">
          <button
            type="button"
            onclick={lib.backToPeerList}
            class="flex h-6 w-6 items-center justify-center rounded-sm text-ink-2 transition-colors hover:bg-bg-2 hover:text-ink-0"
            aria-label="Back to LAN peers"
            title="Back"
          >
            <ArrowLeft size={13} />
          </button>
          <div class="min-w-0 flex-1">
            <div class="truncate text-[12.5px] text-ink-0" title={lib.openPeer.device_name}>
              {lib.openPeer.device_name}
            </div>
            <div class="font-mono mt-0.5 text-[10px] text-ink-3 tracking-[0.04em]">
              {lib.openPeer.addr}:{lib.openPeer.file_server_port}
            </div>
          </div>
        </header>
        {#if lib.peerGamesLoading}
          <div class="flex items-center justify-center gap-2 px-3.5 py-6 text-[12px] text-ink-3">
            <Loader2 size={14} class="animate-[spool-spin_1s_linear_infinite]" />
            Loading library…
          </div>
        {:else if lib.peerGamesError}
          <div class="px-3.5 py-4 text-[11.5px] text-ink-2">
            <div class="font-medium text-ink-1">Couldn't reach peer</div>
            <div class="mt-1 text-[11px] text-ink-3">{lib.peerGamesError}</div>
          </div>
        {:else if lib.peerGames.length === 0}
          <div class="px-3.5 py-4 text-center text-[12px] text-ink-3">
            Peer isn't sharing any games.
          </div>
        {:else}
          <ul class="max-h-[360px] overflow-y-auto py-1">
            {#each lib.peerGames as game (game.id)}
              {@const dl =
                lib.activeDownload &&
                lib.activeDownload.source_game_id === game.id &&
                lib.openPeer &&
                lib.activeDownload.source_device_id === lib.openPeer.device_id
                  ? lib.activeDownload
                  : null}
              {@const inflight =
                dl && (dl.status === 'starting' || dl.status === 'transferring')}
              {@const starting = lib.startingGameId === game.id}
              {@const busy =
                !!lib.startingGameId ||
                (lib.activeDownload &&
                  (lib.activeDownload.status === 'starting' ||
                    lib.activeDownload.status === 'transferring'))}
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
                    onclick={() => lib.openPeer && lib.installFromPeer(lib.openPeer, game)}
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
                    onclick={lib.cancelActiveInstall}
                    aria-label="Cancel install"
                    title="Cancel install"
                    class="inline-flex shrink-0 items-center justify-center rounded-sm border border-line-2 bg-bg-2 text-ink-2 transition-colors hover:border-bad/60 hover:text-bad"
                    style:height="var(--control-h-icon)"
                    style:width="var(--control-h-icon)"
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
            {lib.lanPeers.length}
          </span>
        </header>
        {#if lib.lanPeers.length === 0}
          <div class="px-3.5 py-4 text-center text-[12px] text-ink-3">
            Nobody else on the LAN.
          </div>
        {:else}
          <ul class="max-h-[320px] overflow-y-auto py-1">
            {#each lib.lanPeers as peer (peer.device_id)}
              {@const browsable = peer.file_server_port !== 0}
              <li>
                <button
                  type="button"
                  onclick={() => lib.openPeerView(peer)}
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
          class="flex items-center gap-2 rounded-sm border border-line-1 bg-bg-2 px-2.5"
          style:height="var(--control-h)"
        >
          <Search size={14} class="text-ink-2" />
          <input
            bind:value={lib.searchQuery}
            placeholder={`Search ${lib.games.length || 0} games`}
            class="font-sans min-w-0 flex-1 bg-transparent text-[length:var(--text-base)] text-ink-0 outline-none placeholder:text-ink-3"
          />
        </div>
        <div class="flex gap-1">
          {#each filters as f (f.id)}
            {@const active = lib.filter === f.id}
            <button
              type="button"
              onclick={() => (lib.filter = f.id)}
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
                  ? lib.games.length
                  : f.id === 'recent'
                    ? lib.games.filter((g) => g.last_played_at || g.added_at).length
                    : lib.games.filter((g) => g.playtime_minutes > 0).length}
              </span>
            </button>
          {/each}
        </div>
      </div>

      <!-- Section header -->
      <div class="flex items-center justify-between px-3.5 pb-1.5 pt-2.5">
        <MonoLabel size={9.5}>
          {lib.filter === 'recent' ? 'By last activity' : 'By catalog'}
        </MonoLabel>
        <span class="text-[11px] text-ink-3">{lib.filteredGames.length}</span>
      </div>

      <!-- List -->
      <div class="min-h-0 flex-1 overflow-y-auto pb-2">
        {#if !lib.loaded}
          <p
            class="font-mono px-4 py-3 text-[10px] uppercase tracking-[0.12em] text-ink-3"
          >
            Loading…
          </p>
        {:else if lib.error}
          <p class="px-4 py-3 text-[12px] text-bad">{lib.error}</p>
        {:else if lib.filteredGames.length === 0 && lib.games.length === 0}
          <div class="flex flex-col items-center gap-2 px-4 py-10 text-center">
            <MonoLabel>Empty shelf</MonoLabel>
            <p class="text-[12px] text-ink-2">No games yet.</p>
          </div>
        {:else if lib.filteredGames.length === 0}
          <p class="px-4 py-3 text-[12px] text-ink-3">No matches.</p>
        {:else}
          {#each lib.filteredGames as g, i (g.id)}
            {@const selected = lib.selectedId === g.id}
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
              data-testid="game-row"
              data-game-name={g.game_name}
              data-gp-autofocus={(lib.selectedId ? selected : i === 0) ? '' : undefined}
              onclick={() => (lib.selectedId = g.id)}
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
      <div class="flex flex-col gap-1.5 border-t border-line-1 bg-bg-0 px-3 py-2.5">
        {#if isLinux}
          <button
            type="button"
            onclick={() => openView('install')}
            class="inline-flex h-8 w-full cursor-pointer items-center justify-center gap-1.5 rounded-sm border border-line-2 bg-bg-1 px-3 text-[12.5px] font-medium text-ink-1 transition-colors hover:bg-bg-2 hover:text-ink-0"
          >
            <Package size={14} />
            Install game
          </button>
        {/if}
        <button
          type="button"
          onclick={() => openView('add')}
          class="inline-flex h-8 w-full cursor-pointer items-center justify-center gap-1.5 rounded-sm bg-spool px-3 text-[12.5px] font-medium text-bg-0 transition-colors hover:brightness-95"
        >
          <Plus size={14} />
          Add a game
        </button>
      </div>
    </aside>

    <!-- ── Detail pane ──────────────────────────────────────────────── -->
    {#if lib.selectedGame}
      <GameDetail
        game={lib.selectedGame}
        runPhase={lib.runningId === lib.selectedGame.id ? lib.runningPhase : null}
      />
    {:else if lib.loaded && lib.games.length === 0}
      <div class="flex flex-col items-center justify-center gap-3 text-center">
        <MonoLabel>Empty library</MonoLabel>
        <p class="max-w-md text-sm text-ink-2">
          No games yet. Add an executable to start your collection.
        </p>
        <div class="flex items-center gap-2">
          {#if isLinux}
            <button
              type="button"
              onclick={() => openView('install')}
              class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-sm border border-line-2 bg-bg-1 px-3 text-[12.5px] font-medium text-ink-1 transition-colors hover:bg-bg-2 hover:text-ink-0"
            >
              <Package size={14} />
              Install a repack
            </button>
          {/if}
          <button
            type="button"
            onclick={() => openView('add')}
            class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-sm bg-spool px-3 text-[12.5px] font-medium text-bg-0 transition-colors hover:brightness-95"
          >
            <Plus size={14} />
            Add your first game
          </button>
        </div>
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
